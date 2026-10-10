// SPDX-License-Identifier: GPL-2.0-or-later
//! HID class descriptor dumping.

use super::{junk, too_short};
use crate::model::{bcd, join, Node};
use crate::names::{names_countrycode, names_hid, names_huts, names_hutus, names_reporttag};
use crate::usb::{self, control_in, Handle, InterfaceDescriptor};

/// Upstream has a (never cleared) flag for this.
const DO_REPORT_DESC: bool = true;

/// Describe a HID unit value.
fn dump_unit(data: u32, len: u32) -> String {
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
        let system = if sys == 0xf {
            "Vendor defined"
        } else {
            "Reserved"
        };
        return format!("System: {system}, Unit: (unknown)");
    }

    let mut units = Vec::new();
    for unit in UNITS[sys].iter().take((len * 2) as usize).skip(1) {
        let nibble = (data & 0xf) as i32;
        data >>= 4;
        if nibble == 1 {
            units.push(unit.to_string());
        } else if nibble != 0 {
            // This is a _signed_ nibble(!)
            let mut val = nibble & 0x7;
            if nibble & 0x08 != 0 {
                val = -((0x7 & !val) + 1);
            }
            units.push(format!("{unit}^{val}"));
        }
    }
    let units = if units.is_empty() {
        "(None)".to_string()
    } else {
        units.join("*")
    };
    format!("System: {}, Unit: {units}", SYSTEMS[sys])
}

/// The meaning of an Input, Output or Feature item's data bits.
fn main_item_flags(data: u32) -> String {
    let f = |bit: u32, set: &'static str, clear: &'static str| {
        if data & bit != 0 {
            set
        } else {
            clear
        }
    };
    [
        f(0x01, "Constant", "Data"),
        f(0x02, "Variable", "Array"),
        f(0x04, "Relative", "Absolute"),
        f(0x08, "Wrap", "No_Wrap"),
        f(0x10, "Non_Linear", "Linear"),
        f(0x20, "No_Preferred_State", "Preferred_State"),
        f(0x40, "Null_State", "No_Null_Position"),
        f(0x80, "Volatile", "Non_Volatile"),
        f(0x100, "Buffered Bytes", "Bitfield"),
    ]
    .join(" ")
}

/// Add a "Report Descriptor" section decoding the items of a HID report
/// descriptor.
pub(crate) fn dump_report_desc(b: &[u8], node: &mut Node) {
    const TYPES: [&str; 4] = ["Main", "Global", "Local", "reserved"];
    let l = b.len();

    // Like upstream, `data` keeps its value across items without data.
    let mut data: u32 = 0xffff;
    let mut hut: u32 = 0xffff;

    let s = node.section("Report Descriptor").note(format!("{l} bytes"));
    let mut i = 0;
    while i < l {
        let bsize = match b[i] & 0x03 {
            3 => 4,
            n => n as usize,
        };
        if i + 1 + bsize > l {
            // Truncated item: avoid OOB reads of b[i+1+j]
            s.warn(format!("Truncated item at offset {i}"));
            break;
        }
        let btype = (b[i] & (0x03 << 2)) as usize;
        let btag = b[i] & !0x03; // 2 LSB bits encode length
        let value = if bsize > 0 {
            data = 0;
            for (j, byte) in b[i + 1..i + 1 + bsize].iter().enumerate() {
                data = data.wrapping_add((*byte as u32) << (8 * j));
            }
            (data as i32).to_string()
        } else {
            String::new()
        };
        let meaning = match btag {
            0x04 => {
                // Usage Page
                hut = data;
                names_huts(data).unwrap_or("Unknown").to_string()
            }
            0x08 | 0x18 | 0x28 => {
                // Usage, Usage Minimum, Usage Maximum
                names_hutus((hut << 16).wrapping_add(data))
                    .unwrap_or("Unknown")
                    .to_string()
            }
            // Unit Exponent
            0x54 => format!("10^{}", data as u8 as i8),
            // Unit
            0x64 => dump_unit(data, bsize as u32),
            // Collection
            0xa0 => match data {
                0x00 => "Physical",
                0x01 => "Application",
                0x02 => "Logical",
                0x03 => "Report",
                0x04 => "Named Array",
                0x05 => "Usage Switch",
                0x06 => "Usage Modifier",
                _ if data & 0x80 != 0 => "Vendor defined",
                _ => "Reserved for future use",
            }
            .to_string(),
            // Input, Output, Feature
            0x80 | 0x90 | 0xb0 => main_item_flags(data),
            _ => String::new(),
        };
        s.field(names_reporttag(btag).unwrap_or("Unknown"), value)
            .note(join(&[TYPES[btype >> 2], &meaning]));
        i += 1 + bsize;
    }
}

/// Fetch and dump one report descriptor of `len` bytes.
fn dump_report(dev: &Handle, ifnum: u8, len: usize, s: &mut Node) {
    let mut dbuf = vec![0u8; 8192];
    if len > dbuf.len() {
        s.warn("Report descriptor too long");
        return;
    }
    if dev.claim_interface(ifnum).is_err() {
        // Recent Linuxes require claim() for RECIP_INTERFACE, so
        // "rmmod hid" will often make these available.
        s.warn("Report descriptor unavailable");
        return;
    }

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
            s.warn("Incomplete report descriptor");
        }
        dump_report_desc(&dbuf[..n as usize], s);
    } else {
        s.warn(format!(
            "Can't get report descriptor: {}",
            if n == 0 {
                "LIBUSB_SUCCESS"
            } else {
                err.map_or("LIBUSB_ERROR_OTHER", usb::error_name)
            }
        ));
    }
    let _ = dev.release_interface(ifnum);
}

pub(super) fn dump_hid_device(
    dev: Option<&Handle>,
    interface: &InterfaceDescriptor,
    buf: &[u8],
    node: &mut Node,
) {
    let s = node.section("HID Device");
    if buf[1] != usb::DT_HID {
        s.warn("Invalid descriptor type");
    }
    if buf[0] < 6 {
        too_short(s);
        return;
    }
    let num = buf[5] as usize;
    let len = buf[0] as usize;
    if len < 6 + 3 * num {
        too_short(s);
    }
    let version = bcd(usb::le16(&buf[2..]) as u16);
    s.note(format!("HID {version}"));
    s.field("bLength", buf[0]);
    s.field("bDescriptorType", buf[1]);
    s.field("bcdHID", version);
    s.field("bCountryCode", buf[4])
        .note(names_countrycode(buf[4] as u32).unwrap_or("Unknown"));
    s.field("bNumDescriptors", buf[5]);
    let descriptors = (0..num).take_while(|i| 6 + 3 * i + 3 <= len);
    for i in descriptors.clone() {
        let ty = buf[6 + 3 * i];
        s.field(format!("bDescriptorType[{i}]"), ty)
            .note(names_hid(ty).unwrap_or("Unknown"));
        s.field(
            format!("wDescriptorLength[{i}]"),
            usb::le16(&buf[7 + 3 * i..]),
        );
    }
    junk(buf, 6 + 3 * num, s);
    if !DO_REPORT_DESC {
        return;
    }

    let Some(dev) = dev else {
        s.warn("Report descriptor unavailable");
        return;
    };

    for i in descriptors {
        // We are just interested in report descriptors.
        if buf[6 + 3 * i] != usb::DT_REPORT {
            continue;
        }
        let len = usb::le16(&buf[7 + 3 * i..]) as usize;
        dump_report(dev, interface.b_interface_number, len, s);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units() {
        // Centimeters per second squared (acceleration), SI Linear.
        assert_eq!(
            dump_unit(0xe011, 2),
            "System: SI Linear, Unit: Centimeter*Seconds^-2"
        );
        assert_eq!(dump_unit(0x0001, 1), "System: SI Linear, Unit: (None)");
        assert_eq!(
            dump_unit(0x000f, 1),
            "System: Vendor defined, Unit: (unknown)"
        );
    }

    #[test]
    fn mouse_report_descriptor() {
        let desc = [
            0x05, 0x01, // Usage Page (Generic Desktop)
            0x09, 0x02, // Usage (Mouse)
            0xa1, 0x01, // Collection (Application)
            0x05, 0x09, // Usage Page (Buttons)
            0x19, 0x01, // Usage Minimum (1)
            0x15, 0x00, // Logical Minimum (0)
            0x81, 0x02, // Input (Data, Variable, Absolute)
            0xc0, // End Collection
            0x95, // truncated Report Count
        ];
        let mut node = Node::new("dev");
        dump_report_desc(&desc, &mut node);
        let s = node.get("Report Descriptor").unwrap();
        assert_eq!(s.note, "16 bytes");

        let items: Vec<_> = s
            .children
            .iter()
            .map(|c| (c.name.as_str(), c.value.as_str()))
            .collect();
        assert_eq!(items[0], ("Usage Page", "1"));
        assert_eq!(items[1], ("Usage", "2"));
        assert_eq!(items[7], ("End Collection", ""));

        assert_eq!(s.children[0].note, "Global · Generic Desktop Controls");
        assert_eq!(s.children[1].note, "Local · Mouse");
        assert_eq!(s.children[2].note, "Main · Application");
        assert!(s.children[6]
            .note
            .starts_with("Main · Data Variable Absolute"));
        assert!(s.has_warnings());
    }
}
