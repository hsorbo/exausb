// SPDX-License-Identifier: GPL-2.0-or-later
//! Command line handling for `exausb`.

use std::process::ExitCode;

use anstyle::{AnsiColor, Style};
use clap::{ColorChoice, Parser};
use rusb::UsbContext;

use crate::names::{self, get_vendor_product_with_fallback};
use crate::render::{self, print};
use crate::usb::{self, Context};
use crate::{dump, tree};

/// List USB devices.
#[derive(Parser, Debug)]
#[command(name = "exausb", version, about)]
struct Args {
    /// Show all descriptors of each device
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    /// Show the physical USB device hierarchy as a tree
    #[arg(short, long)]
    tree: bool,

    /// Browse devices interactively
    #[arg(short, long, conflicts_with_all = ["verbose", "tree"])]
    interactive: bool,

    /// Only show devices with this bus and/or device number (decimal)
    #[arg(short = 's', value_name = "[[BUS]:][DEVNUM]", value_parser = parse_select)]
    select: Option<Select>,

    /// Only show devices with this vendor and/or product ID (hex)
    #[arg(short = 'd', value_name = "VENDOR[:PRODUCT]", value_parser = parse_ids)]
    ids: Option<Ids>,

    /// When to use colors
    #[arg(long, value_name = "WHEN", default_value_t = ColorChoice::Auto)]
    color: ColorChoice,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Select {
    bus: Option<u8>,
    devnum: Option<u8>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Ids {
    vendor: Option<u16>,
    product: Option<u16>,
}

/// Parse `[[bus]:][devnum]`.
fn parse_select(s: &str) -> Result<Select, String> {
    let num = |s: &str| -> Result<Option<u8>, String> {
        if s.is_empty() {
            Ok(None)
        } else {
            s.parse()
                .map(Some)
                .map_err(|_| format!("invalid number '{s}'"))
        }
    };
    match s.split_once(':') {
        Some((bus, dev)) => Ok(Select {
            bus: num(bus)?,
            devnum: num(dev)?,
        }),
        None => Ok(Select {
            bus: None,
            devnum: num(s)?,
        }),
    }
}

/// Parse `vendor[:product]` (hex, optional `0x`).
fn parse_ids(s: &str) -> Result<Ids, String> {
    let hex = |s: &str| -> Result<Option<u16>, String> {
        let digits = s.trim_start_matches("0x").trim_start_matches("0X");
        if s.is_empty() {
            Ok(None)
        } else {
            u16::from_str_radix(digits, 16)
                .map(Some)
                .map_err(|_| format!("invalid ID '{s}'"))
        }
    };
    let (vendor, product) = s.split_once(':').unwrap_or((s, ""));
    Ok(Ids {
        vendor: hex(vendor)?,
        product: hex(product)?,
    })
}

impl Args {
    fn matches(&self, dev: &usb::Device) -> bool {
        let desc = usb::device_descriptor(dev);
        let sel = self.select.unwrap_or_default();
        let ids = self.ids.unwrap_or_default();
        sel.bus.is_none_or(|b| b == dev.bus_number())
            && sel.devnum.is_none_or(|d| d == dev.address())
            && ids.vendor.is_none_or(|v| v == desc.id_vendor)
            && ids.product.is_none_or(|p| p == desc.id_product)
    }
}

fn list(devices: &[usb::Device]) {
    let rows: Vec<Vec<String>> = devices
        .iter()
        .map(|dev| {
            let desc = usb::device_descriptor(dev);
            let (vendor, product) = get_vendor_product_with_fallback(dev, None);
            vec![
                format!("{:03}", dev.bus_number()),
                format!("{:03}", dev.address()),
                format!("{:04x}:{:04x}", desc.id_vendor, desc.id_product),
                tree::speed_short(usb::device_speed(dev)).to_string(),
                vendor,
                product,
            ]
        })
        .collect();
    let plain = Style::new();
    print(&render::table(
        &["BUS", "DEV", "ID", "SPEED", "VENDOR", "PRODUCT"],
        &rows,
        &[
            plain,
            plain,
            AnsiColor::Yellow.on_default(),
            AnsiColor::Green.on_default(),
            plain,
            Style::new().bold(),
        ],
    ));
}

/// Run exausb with the process' command line arguments.
pub fn run() -> ExitCode {
    let args = Args::parse();
    anstream::ColorChoice::write_global(match args.color {
        ColorChoice::Auto => anstream::ColorChoice::Auto,
        ColorChoice::Always => anstream::ColorChoice::Always,
        ColorChoice::Never => anstream::ColorChoice::Never,
    });

    if let Err(e) = names::names_init() {
        eprintln!(
            "exausb: can't read {}, using the bundled usb.ids: {e}",
            names::USB_IDS_ENV
        );
    }

    let ctx = match Context::new() {
        Ok(ctx) => ctx,
        Err(e) => {
            eprintln!("exausb: can't initialize libusb: {e}");
            return ExitCode::FAILURE;
        }
    };

    if args.interactive {
        return match crate::tui::run(ctx) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("exausb: {e}");
                ExitCode::FAILURE
            }
        };
    }

    if args.tree {
        return match tree::tree(&ctx, true) {
            Ok(busses) => {
                print(&busses.iter().map(render::render).collect::<String>());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("exausb: can't list devices: {e}");
                ExitCode::FAILURE
            }
        };
    }

    let mut devices: Vec<usb::Device> = match ctx.devices() {
        Ok(list) => list.iter().filter(|d| args.matches(d)).collect(),
        Err(e) => {
            eprintln!("exausb: can't list devices: {e}");
            return ExitCode::FAILURE;
        }
    };
    devices.sort_by_key(|dev| (dev.bus_number(), dev.address()));
    if devices.is_empty() {
        // Like lsusb: nothing found is an error.
        return ExitCode::FAILURE;
    }

    if args.verbose > 0 {
        let dumps: Vec<String> = devices
            .iter()
            .map(|dev| render::render(&dump::dumpdev(dev)))
            .collect();
        print(&dumps.join("\n"));
    } else {
        list(&devices);
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_definition() {
        Args::command().debug_assert();
    }

    #[test]
    fn flags() {
        let a = Args::try_parse_from(["exausb", "-vv", "-t"]).unwrap();
        assert_eq!((a.verbose, a.tree), (2, true));
        assert!(Args::try_parse_from(["exausb", "extra"]).is_err());
        assert!(Args::try_parse_from(["exausb", "-x"]).is_err());
    }

    #[test]
    fn select() {
        let sel = |s| parse_select(s).unwrap();
        assert_eq!(
            sel("1:2"),
            Select {
                bus: Some(1),
                devnum: Some(2)
            }
        );
        assert_eq!(
            sel("3:"),
            Select {
                bus: Some(3),
                devnum: None
            }
        );
        assert_eq!(
            sel(":7"),
            Select {
                bus: None,
                devnum: Some(7)
            }
        );
        assert_eq!(
            sel("9"),
            Select {
                bus: None,
                devnum: Some(9)
            }
        );
        assert_eq!(
            sel("004:010"),
            Select {
                bus: Some(4),
                devnum: Some(10)
            }
        );
        assert!(parse_select("x").is_err());
        assert!(parse_select("300").is_err());
    }

    #[test]
    fn ids() {
        let ids = |s| parse_ids(s).unwrap();
        assert_eq!(
            ids("1d6b:0002"),
            Ids {
                vendor: Some(0x1d6b),
                product: Some(2)
            }
        );
        assert_eq!(
            ids("0x05ac:"),
            Ids {
                vendor: Some(0x05ac),
                product: None
            }
        );
        assert_eq!(
            ids("05ac"),
            Ids {
                vendor: Some(0x05ac),
                product: None
            }
        );
        assert_eq!(
            ids(":12a8"),
            Ids {
                vendor: None,
                product: Some(0x12a8)
            }
        );
        assert!(parse_ids("xyz").is_err());
    }
}
