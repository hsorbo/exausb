// SPDX-License-Identifier: GPL-2.0-or-later
//! HID class descriptor dumping.

use super::dump_junk;
use crate::names::{names_countrycode, names_hid, names_huts, names_hutus, names_reporttag};
use crate::usb::{self, control_in, Handle, InterfaceDescriptor};

/// Upstream has a (never cleared) flag for this.
const DO_REPORT_DESC: bool = true;

/// Print a HID unit value.
fn dump_unit(data: u32, len: u32) {
    const SYSTEMS: [&str; 5] = [
        "None",
        "SI Linear",
        "SI Rotation",
        "English Linear",
        "English Rotation",
    ];
    const UNITS: [[&str; 8]; 5] = [
        [
            "None", "None", "None", "None", "None", "None", "None", "None",
        ],
        [
            "None",
            "Centimeter",
            "Gram",
            "Seconds",
            "Kelvin",
            "Ampere",
            "Candela",
            "None",
        ],
        [
            "None", "Radians", "Gram", "Seconds", "Kelvin", "Ampere", "Candela", "None",
        ],
        [
            "None",
            "Inch",
            "Slug",
            "Seconds",
            "Fahrenheit",
            "Ampere",
            "Candela",
            "None",
        ],
        [
            "None",
            "Degrees",
            "Slug",
            "Seconds",
            "Fahrenheit",
            "Ampere",
            "Candela",
            "None",
        ],
    ];

    // First nibble tells us which system we're in.
    let sys = (data & 0xf) as usize;
    let mut data = data >> 4;

    if sys > 4 {
        if sys == 0xf {
            println!("System: Vendor defined, Unit: (unknown)");
        } else {
            println!("System: Reserved, Unit: (unknown)");
        }
        return;
    }
    print!("System: {}, Unit: ", SYSTEMS[sys]);

    let mut earlier_unit = 0;
    for unit in UNITS[sys].iter().take((len * 2) as usize).skip(1) {
        let nibble = (data & 0xf) as i32;
        data >>= 4;
        if nibble != 0 {
            if earlier_unit > 0 {
                print!("*");
            }
            earlier_unit += 1;
            print!("{unit}");
            if nibble != 1 {
                // This is a _signed_ nibble(!)
                let mut val = nibble & 0x7;
                if nibble & 0x08 != 0 {
                    val = -((0x7 & !val) + 1);
                }
                print!("^{val}");
            }
        }
    }
    if earlier_unit == 0 {
        print!("(None)");
    }
    println!();
}

/// Print a HID report descriptor.
pub(crate) fn dump_report_desc(b: &[u8]) {
    const TYPES: [&str; 4] = ["Main", "Global", "Local", "reserved"];
    const INDENT: &str = "                            ";
    let l = b.len();

    // Like upstream, `data` keeps its value across items without data.
    let mut data: u32 = 0xffff;
    let mut hut: u32 = 0xffff;

    println!("          Report Descriptor: (length is {l})");
    let mut i = 0;
    while i < l {
        let bsize = match b[i] & 0x03 {
            3 => 4,
            n => n as usize,
        };
        if i + 1 + bsize > l {
            // Truncated item: avoid OOB reads of b[i+1+j]
            println!("            ** TRUNCATED at offset {i} **");
            break;
        }
        let btype = (b[i] & (0x03 << 2)) as usize;
        let btag = b[i] & !0x03; // 2 LSB bits encode length
        print!(
            "            Item({:<6}): {}, data=",
            TYPES[btype >> 2],
            names_reporttag(btag).unwrap_or("Unknown")
        );
        if bsize > 0 {
            print!(" [ ");
            data = 0;
            for j in 0..bsize {
                print!("0x{:02x} ", b[i + 1 + j]);
                data = data.wrapping_add((b[i + 1 + j] as u32) << (8 * j));
            }
            print!("] {}", data as i32);
        } else {
            print!("none");
        }
        println!();
        match btag {
            0x04 => {
                // Usage Page
                println!("{INDENT}{}", names_huts(data).unwrap_or("Unknown"));
                hut = data;
            }
            0x08 | 0x18 | 0x28 => {
                // Usage, Usage Minimum, Usage Maximum
                println!(
                    "{INDENT}{}",
                    names_hutus((hut << 16).wrapping_add(data)).unwrap_or("Unknown")
                );
            }
            0x54 => {
                // Unit Exponent
                println!("{INDENT}Unit Exponent: {}", data as u8 as i8);
            }
            0x64 => {
                // Unit
                print!("{INDENT}");
                dump_unit(data, bsize as u32);
            }
            0xa0 => {
                // Collection
                print!("{INDENT}");
                match data {
                    0x00 => println!("Physical"),
                    0x01 => println!("Application"),
                    0x02 => println!("Logical"),
                    0x03 => println!("Report"),
                    0x04 => println!("Named Array"),
                    0x05 => println!("Usage Switch"),
                    0x06 => println!("Usage Modifier"),
                    _ if data & 0x80 != 0 => println!("Vendor defined"),
                    _ => println!("Reserved for future use."),
                }
            }
            0x80 | 0x90 | 0xb0 => {
                // Input, Output, Feature
                let f = |bit: u32, set: &'static str, clear: &'static str| {
                    if data & bit != 0 {
                        set
                    } else {
                        clear
                    }
                };
                println!(
                    "{INDENT}{} {} {} {} {}\n{INDENT}{} {} {} {}",
                    f(0x01, "Constant", "Data"),
                    f(0x02, "Variable", "Array"),
                    f(0x04, "Relative", "Absolute"),
                    f(0x08, "Wrap", "No_Wrap"),
                    f(0x10, "Non_Linear", "Linear"),
                    f(0x20, "No_Preferred_State", "Preferred_State"),
                    f(0x40, "Null_State", "No_Null_Position"),
                    f(0x80, "Volatile", "Non_Volatile"),
                    f(0x100, "Buffered Bytes", "Bitfield"),
                );
            }
            _ => {}
        }
        i += 1 + bsize;
    }
}

pub(super) fn dump_hid_device(dev: Option<&Handle>, interface: &InterfaceDescriptor, buf: &[u8]) {
    if buf[1] != usb::DT_HID {
        println!("      Warning: Invalid descriptor");
    }
    if buf[0] < 6 {
        println!("      Warning: Descriptor too short");
        return;
    }
    let num = buf[5] as usize;
    let len = buf[0] as usize;
    if len < 6 + 3 * num {
        println!("      Warning: Descriptor too short");
    }
    println!("        HID Device Descriptor:");
    println!("          bLength             {:5}", buf[0]);
    println!("          bDescriptorType     {:5}", buf[1]);
    println!("          bcdHID              {:2x}.{:02x}", buf[3], buf[2]);
    println!(
        "          bCountryCode        {:5} {}",
        buf[4],
        names_countrycode(buf[4] as u32).unwrap_or("Unknown")
    );
    println!("          bNumDescriptors     {:5}", buf[5]);
    let descriptors = (0..num).take_while(|i| 6 + 3 * i + 3 <= len);
    for i in descriptors.clone() {
        println!(
            "          bDescriptorType     {:5} {}",
            buf[6 + 3 * i],
            names_hid(buf[6 + 3 * i]).unwrap_or("Unknown")
        );
        println!(
            "          wDescriptorLength   {:5}",
            usb::le16(&buf[7 + 3 * i..])
        );
    }
    dump_junk(buf, "        ", 6 + 3 * num);
    if !DO_REPORT_DESC {
        return;
    }

    let Some(dev) = dev else {
        println!("          Report Descriptors: \n            ** UNAVAILABLE **");
        return;
    };

    for i in descriptors {
        // We are just interested in report descriptors.
        if buf[6 + 3 * i] != usb::DT_REPORT {
            continue;
        }
        let len = usb::le16(&buf[7 + 3 * i..]) as usize;
        let mut dbuf = vec![0u8; 8192];
        if len > dbuf.len() {
            println!("report descriptor too long");
            continue;
        }
        let ifnum = interface.b_interface_number;
        if dev.claim_interface(ifnum).is_ok() {
            let mut retries = 4;
            let mut n: i64 = 0;
            let mut err = None;
            while n < len as i64 && retries > 0 {
                retries -= 1;
                n = match control_in(
                    dev,
                    usb::REQUEST_TYPE_STANDARD | usb::RECIPIENT_INTERFACE,
                    usb::REQUEST_GET_DESCRIPTOR,
                    (usb::DT_REPORT as u16) << 8,
                    ifnum as u16,
                    &mut dbuf[..len],
                ) {
                    Ok(n) => n as i64,
                    Err(e) => {
                        err = Some(e);
                        -1
                    }
                };
            }

            if n > 0 {
                if (n as usize) < len {
                    println!("          Warning: incomplete report descriptor");
                }
                dump_report_desc(&dbuf[..n as usize]);
            } else {
                println!(
                    "          Warning: can't get report descriptor, {}",
                    if n == 0 {
                        "LIBUSB_SUCCESS"
                    } else {
                        err.map_or("LIBUSB_ERROR_OTHER", usb::error_name)
                    }
                );
            }
            let _ = dev.release_interface(ifnum);
        } else {
            // Recent Linuxes require claim() for RECIP_INTERFACE, so
            // "rmmod hid" will often make these available.
            println!("          Report Descriptors: \n            ** UNAVAILABLE **");
        }
    }
}
