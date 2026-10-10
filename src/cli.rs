// SPDX-License-Identifier: GPL-2.0-or-later
//! Command line handling for `lsusb`.

use std::process::ExitCode;

use rusb::UsbContext;

use crate::names::get_vendor_product_with_fallback;
use crate::usb::Context;
use crate::{dump, names, tree, usb, usbmisc};

const USAGE: &str = "\
Usage: lsusb [options]...
List USB devices
  -v, --verbose
      Increase verbosity (show descriptors)
  -s [[bus]:][devnum]
      Show only devices with specified device and/or
      bus numbers (in decimal)
  -d vendor:[product]
      Show only devices with the specified vendor and
      product ID numbers (in hexadecimal)
  -D device
      Selects which device lsusb will examine
  -t, --tree
      Dump the physical USB device hierarchy as a tree
  -V, --version
      Show version of program
  -h, --help
      Show usage and help
";

/// Parsed command line.
#[derive(Debug, Default, PartialEq, Eq)]
struct Options {
    verblevel: u32,
    help: bool,
    version: bool,
    treemode: bool,
    bus: Option<u32>,
    devnum: Option<u32>,
    vendor: Option<u32>,
    product: Option<u32>,
    devdump: Option<String>,
    /// Number of errors (unknown options, missing arguments, ...).
    err: u32,
}

/// C `strtoul()`: parse a leading number, ignoring trailing garbage.
fn strtoul(s: &str, radix: u32) -> u32 {
    let s = s.trim_start();
    let s = s.strip_prefix('+').unwrap_or(s);
    let s = if radix == 16 {
        s.strip_prefix("0x")
            .or_else(|| s.strip_prefix("0X"))
            .unwrap_or(s)
    } else {
        s
    };
    let digits = s.find(|c: char| !c.is_digit(radix)).unwrap_or(s.len());
    u64::from_str_radix(&s[..digits], radix).map_or(0, |v| v as u32)
}

/// Parse arguments like upstream's `getopt_long(argc, argv, "D:vtP:p:s:d:Vh", ...)`.
fn parse_args<I: IntoIterator<Item = String>>(args: I) -> Options {
    const LONG: [(&str, char); 4] = [
        ("version", 'V'),
        ("verbose", 'v'),
        ("help", 'h'),
        ("tree", 't'),
    ];
    const WITH_ARG: &str = "DPpsd";
    const NO_ARG: &str = "vtVh";

    let mut opts = Options::default();
    let mut positional = 0;
    let mut args = args.into_iter();

    let apply = |opts: &mut Options, c: char, optarg: Option<String>| match c {
        'V' => opts.version = true,
        'v' => opts.verblevel += 1,
        'h' => opts.help = true,
        't' => opts.treemode = true,
        's' => {
            let optarg = optarg.unwrap_or_default();
            match optarg.split_once(':') {
                Some((bus, dev)) => {
                    if !bus.is_empty() {
                        opts.bus = Some(strtoul(bus, 10));
                    }
                    if !dev.is_empty() {
                        opts.devnum = Some(strtoul(dev, 10));
                    }
                }
                None => {
                    if !optarg.is_empty() {
                        opts.devnum = Some(strtoul(&optarg, 10));
                    }
                }
            }
        }
        'd' => {
            let optarg = optarg.unwrap_or_default();
            match optarg.split_once(':') {
                Some((vendor, product)) => {
                    if !vendor.is_empty() {
                        opts.vendor = Some(strtoul(vendor, 16));
                    }
                    if !product.is_empty() {
                        opts.product = Some(strtoul(product, 16));
                    }
                }
                None => opts.err += 1,
            }
        }
        'D' => opts.devdump = optarg,
        // -P and -p are accepted by upstream's getopt string but unhandled.
        _ => opts.err += 1,
    };

    while let Some(arg) = args.next() {
        if arg == "--" {
            positional += args.by_ref().count();
            break;
        }
        if let Some(long) = arg.strip_prefix("--") {
            let (name, value) = match long.split_once('=') {
                Some((n, v)) => (n, Some(v)),
                None => (long, None),
            };
            // Like getopt_long, accept unambiguous prefixes.
            let exact = LONG.iter().find(|(l, _)| *l == name);
            let matches: Vec<_> = LONG.iter().filter(|(l, _)| l.starts_with(name)).collect();
            let found = exact.or(if matches.len() == 1 {
                Some(matches[0])
            } else {
                None
            });
            match found {
                Some((_, c)) if value.is_none() => apply(&mut opts, *c, None),
                Some((l, _)) => {
                    eprintln!("lsusb: option '--{l}' doesn't allow an argument");
                    opts.err += 1;
                }
                None if matches.len() > 1 => {
                    eprintln!("lsusb: option '--{name}' is ambiguous");
                    opts.err += 1;
                }
                None => {
                    eprintln!("lsusb: unrecognized option '--{name}'");
                    opts.err += 1;
                }
            }
        } else if arg.len() > 1 && arg.starts_with('-') {
            for (pos, c) in arg[1..].char_indices() {
                if WITH_ARG.contains(c) {
                    let rest = &arg[1 + pos + c.len_utf8()..];
                    let optarg = if !rest.is_empty() {
                        Some(rest.to_string())
                    } else {
                        args.next()
                    };
                    match optarg {
                        Some(a) => apply(&mut opts, c, Some(a)),
                        None => {
                            eprintln!("lsusb: option requires an argument -- '{c}'");
                            opts.err += 1;
                        }
                    }
                    break;
                } else if NO_ARG.contains(c) {
                    apply(&mut opts, c, None);
                } else {
                    eprintln!("lsusb: invalid option -- '{c}'");
                    opts.err += 1;
                }
            }
        } else {
            positional += 1;
        }
    }
    if positional > 0 {
        opts.err += 1;
    }
    opts
}

fn dump_one_device(ctx: &Context, path: &str) -> u8 {
    let Some(dev) = usbmisc::get_usb_device(ctx, path) else {
        eprintln!("Cannot open {path}");
        return 1;
    };
    let desc = usb::device_descriptor(&dev);
    let (vendor, product) = get_vendor_product_with_fallback(&dev, None);
    println!(
        "Device: ID {:04x}:{:04x} {} {}",
        desc.id_vendor, desc.id_product, vendor, product
    );
    dump::dumpdev(&dev);
    0
}

fn list_devices(ctx: &Context, opts: &Options) -> u8 {
    // 1: device not found, 0: device found
    let mut status = 1;

    let Ok(list) = ctx.devices() else {
        return status;
    };
    let mut devices: Vec<_> = list.iter().collect();
    devices.sort_by_key(|dev| (dev.bus_number(), dev.address()));

    for dev in devices {
        let bnum = dev.bus_number();
        let dnum = dev.address();

        if opts.bus.is_some_and(|b| b != bnum as u32)
            || opts.devnum.is_some_and(|d| d != dnum as u32)
        {
            continue;
        }
        let desc = usb::device_descriptor(&dev);
        if opts.vendor.is_some_and(|v| v != desc.id_vendor as u32)
            || opts.product.is_some_and(|p| p != desc.id_product as u32)
        {
            continue;
        }
        status = 0;

        let (vendor, product) = get_vendor_product_with_fallback(&dev, None);

        if opts.verblevel > 0 {
            println!();
        }
        println!(
            "Bus {:03} Device {:03}: ID {:04x}:{:04x} {} {}",
            bnum, dnum, desc.id_vendor, desc.id_product, vendor, product
        );
        if opts.verblevel > 0 {
            dump::dumpdev(&dev);
        }
    }
    status
}

/// Run lsusb with the process' command line arguments.
pub fn run() -> ExitCode {
    // Behave like a C program when the output pipe is closed (`lsusb | head`)
    // instead of panicking in println!.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }

    let opts = parse_args(std::env::args().skip(1));

    if opts.version {
        println!(
            "lsusb ({}) {}",
            env!("CARGO_PKG_NAME"),
            env!("CARGO_PKG_VERSION")
        );
        return ExitCode::SUCCESS;
    }
    if opts.err > 0 || opts.help {
        eprint!("{USAGE}");
        return if opts.help && opts.err == 0 {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
    }

    dump::set_verblevel(opts.verblevel);

    // By default, print names as well as numbers.
    if let Err(e) = names::names_init() {
        eprintln!(
            "unable to read {}, using the bundled usb.ids: {e}",
            names::USB_IDS_ENV
        );
    }

    if opts.treemode {
        return ExitCode::from(tree::lsusb_t() as u8);
    }

    let ctx = match Context::new() {
        Ok(ctx) => ctx,
        Err(e) => {
            eprintln!("unable to initialize libusb: {e}");
            return ExitCode::FAILURE;
        }
    };

    let status = match &opts.devdump {
        Some(path) => dump_one_device(&ctx, path),
        None => list_devices(&ctx, &opts),
    };
    ExitCode::from(status)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Options {
        parse_args(args.iter().map(|s| s.to_string()))
    }

    #[test]
    fn verbosity_and_flags() {
        let o = parse(&["-vv", "--verbose", "-t"]);
        assert_eq!(o.verblevel, 3);
        assert!(o.treemode);
        assert_eq!(o.err, 0);
        assert!(parse(&["--verb"]).verblevel == 1);
        assert!(parse(&["--ver"]).err > 0); // ambiguous: version/verbose
        assert!(parse(&["--help"]).help);
    }

    #[test]
    fn select_bus_dev() {
        let o = parse(&["-s", "1:2"]);
        assert_eq!((o.bus, o.devnum), (Some(1), Some(2)));
        let o = parse(&["-s3:"]);
        assert_eq!((o.bus, o.devnum), (Some(3), None));
        let o = parse(&["-s", ":7"]);
        assert_eq!((o.bus, o.devnum), (None, Some(7)));
        let o = parse(&["-s", "9"]);
        assert_eq!((o.bus, o.devnum), (None, Some(9)));
        let o = parse(&["-vs", "004:010"]);
        assert_eq!((o.verblevel, o.bus, o.devnum), (1, Some(4), Some(10)));
    }

    #[test]
    fn select_vendor_product() {
        let o = parse(&["-d", "1d6b:0002"]);
        assert_eq!((o.vendor, o.product), (Some(0x1d6b), Some(2)));
        let o = parse(&["-d", "0x05ac:"]);
        assert_eq!((o.vendor, o.product), (Some(0x05ac), None));
        assert_eq!(parse(&["-d", "1234"]).err, 1);
    }

    #[test]
    fn errors() {
        assert_eq!(parse(&["-x"]).err, 1);
        assert_eq!(parse(&["-s"]).err, 1);
        assert_eq!(parse(&["-P", "x"]).err, 1);
        assert_eq!(parse(&["extra"]).err, 1);
        assert_eq!(parse(&["--", "-v"]).err, 1);
        assert_eq!(
            parse(&["-D", "/dev/bus/usb/001/002"]).devdump.as_deref(),
            Some("/dev/bus/usb/001/002")
        );
    }

    #[test]
    fn strtoul_like_c() {
        assert_eq!(strtoul("12abc", 10), 12);
        assert_eq!(strtoul("abc", 10), 0);
        assert_eq!(strtoul("0x1F", 16), 0x1f);
        assert_eq!(strtoul("ffff", 16), 0xffff);
    }
}
