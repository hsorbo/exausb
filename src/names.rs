// SPDX-License-Identifier: GPL-2.0-or-later
//! USB name database lookups.
//!
//! Port of usbutils' `names.c` (Copyright (C) 1999, 2000 Thomas Sailer,
//! Copyright (C) 2013 Tom Gundersen).
//!
//! Upstream looks vendor/product/class names up in the udev hwdb, which is
//! generated from the `usb.ids` file.  udev is Linux-only, so this port reads
//! `usb.ids` directly.  The remaining tables (HID, audio/video terminals, ...)
//! are compiled in from `usb-spec.h`, exactly as upstream does.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::sysfs;
use crate::usb::{self, Device, Handle};
use crate::usb_spec;
use crate::usbmisc::get_dev_string;

/// Environment variable that may point at a `usb.ids` file.
pub const USB_IDS_ENV: &str = "USB_IDS";

/// Places where distributions (and Homebrew/MacPorts) install `usb.ids`.
const USB_IDS_PATHS: &[&str] = &[
    "/usr/share/hwdata/usb.ids",
    "/usr/share/misc/usb.ids",
    "/usr/share/usb.ids",
    "/usr/share/libosinfo/usb.ids",
    "/usr/local/share/hwdata/usb.ids",
    "/usr/local/share/misc/usb.ids",
    "/usr/local/share/usb.ids",
    "/opt/homebrew/share/hwdata/usb.ids",
    "/opt/homebrew/share/misc/usb.ids",
    "/opt/homebrew/share/usb.ids",
    "/opt/local/share/hwdata/usb.ids",
    "/opt/local/share/usb.ids",
    "/var/lib/usbutils/usb.ids",
];

/// Parsed contents of a `usb.ids` file (the parts lsusb uses).
#[derive(Default, Debug)]
pub struct UsbIds {
    vendors: HashMap<u16, String>,
    products: HashMap<(u16, u16), String>,
    classes: HashMap<u8, String>,
    subclasses: HashMap<(u8, u8), String>,
    protocols: HashMap<(u8, u8, u8), String>,
}

impl UsbIds {
    /// Parse the text of a `usb.ids` file.
    pub fn parse(text: &str) -> UsbIds {
        enum Section {
            Vendor(u16),
            Class(u8, Option<u8>),
            Other,
        }

        fn hex<T: TryFrom<u32>>(s: &str, digits: usize) -> Option<T> {
            if s.len() != digits {
                return None;
            }
            u32::from_str_radix(s, 16)
                .ok()
                .and_then(|v| T::try_from(v).ok())
        }

        /// Split "xxxx  name" into its id and name parts.
        fn split(line: &str) -> Option<(&str, &str)> {
            let (id, name) = line.split_once(char::is_whitespace)?;
            Some((id, name.trim()))
        }

        let mut ids = UsbIds::default();
        let mut section = Section::Other;

        for line in text.lines() {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let tabs = line.bytes().take_while(|&b| b == b'\t').count();
            let rest = &line[tabs..];

            match tabs {
                0 => {
                    section = Section::Other;
                    if let Some(class) = rest.strip_prefix("C ") {
                        if let Some((id, name)) = split(class) {
                            if let Some(c) = hex::<u8>(id, 2) {
                                ids.classes.insert(c, name.to_string());
                                section = Section::Class(c, None);
                            }
                        }
                    } else if let Some((id, name)) = split(rest) {
                        if let Some(v) = hex::<u16>(id, 4) {
                            ids.vendors.insert(v, name.to_string());
                            section = Section::Vendor(v);
                        }
                    }
                }
                1 => {
                    let Some((id, name)) = split(rest) else {
                        continue;
                    };
                    match &mut section {
                        Section::Vendor(v) => {
                            if let Some(p) = hex::<u16>(id, 4) {
                                ids.products.insert((*v, p), name.to_string());
                            }
                        }
                        Section::Class(c, sub) => {
                            if let Some(s) = hex::<u8>(id, 2) {
                                ids.subclasses.insert((*c, s), name.to_string());
                                *sub = Some(s);
                            }
                        }
                        Section::Other => {}
                    }
                }
                2 => {
                    if let Section::Class(c, Some(s)) = section {
                        if let Some((id, name)) = split(rest) {
                            if let Some(p) = hex::<u8>(id, 2) {
                                ids.protocols.insert((c, s, p), name.to_string());
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        ids
    }

    pub fn load(path: &Path) -> std::io::Result<UsbIds> {
        let bytes = std::fs::read(path)?;
        Ok(UsbIds::parse(&String::from_utf8_lossy(&bytes)))
    }
}

static DB: OnceLock<Option<UsbIds>> = OnceLock::new();

fn candidate_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(p) = std::env::var_os(USB_IDS_ENV) {
        paths.push(PathBuf::from(p));
    }
    paths.extend(USB_IDS_PATHS.iter().map(PathBuf::from));
    paths
}

/// Load the name database.  Returns `Err` if no `usb.ids` could be read;
/// lookups then simply return `None`.
pub fn names_init() -> Result<(), ()> {
    let db = DB.get_or_init(|| candidate_paths().iter().find_map(|p| UsbIds::load(p).ok()));
    if db.is_some() {
        Ok(())
    } else {
        Err(())
    }
}

fn db() -> Option<&'static UsbIds> {
    DB.get().and_then(Option::as_ref)
}

fn generic(table: &'static [(u32, &'static str)], idx: u32) -> Option<&'static str> {
    table.iter().find(|(n, _)| *n == idx).map(|(_, s)| *s)
}

pub fn names_hid(hidd: u8) -> Option<&'static str> {
    generic(usb_spec::HIDDESCRIPTORS, hidd as u32)
}

pub fn names_reporttag(rt: u8) -> Option<&'static str> {
    generic(usb_spec::REPORTS, rt as u32)
}

pub fn names_huts(data: u32) -> Option<&'static str> {
    generic(usb_spec::HUTS, data)
}

pub fn names_hutus(data: u32) -> Option<&'static str> {
    generic(usb_spec::HUTUS, data)
}

#[allow(dead_code)]
pub fn names_langid(langid: u16) -> Option<&'static str> {
    generic(usb_spec::LANGIDS, langid as u32)
}

#[allow(dead_code)]
pub fn names_physdes(ph: u8) -> Option<&'static str> {
    generic(usb_spec::PHYSDESS, ph as u32)
}

#[allow(dead_code)]
pub fn names_bias(b: u8) -> Option<&'static str> {
    generic(usb_spec::BIASS, b as u32)
}

pub fn names_countrycode(countrycode: u32) -> Option<&'static str> {
    generic(usb_spec::COUNTRYCODES, countrycode)
}

pub fn names_audioterminal(termt: u16) -> Option<&'static str> {
    usb_spec::AUDIOTERMINALS
        .iter()
        .find(|(t, _)| *t == termt)
        .map(|(_, s)| *s)
}

pub fn names_videoterminal(termt: u16) -> Option<&'static str> {
    usb_spec::VIDEOTERMINALS
        .iter()
        .find(|(t, _)| *t == termt)
        .map(|(_, s)| *s)
}

pub fn names_vendor(vendorid: u16) -> Option<&'static str> {
    db()?.vendors.get(&vendorid).map(String::as_str)
}

pub fn names_product(vendorid: u16, productid: u16) -> Option<&'static str> {
    db()?
        .products
        .get(&(vendorid, productid))
        .map(String::as_str)
}

pub fn names_class(classid: u8) -> Option<&'static str> {
    db()?.classes.get(&classid).map(String::as_str)
}

pub fn names_subclass(classid: u8, subclassid: u8) -> Option<&'static str> {
    db()?
        .subclasses
        .get(&(classid, subclassid))
        .map(String::as_str)
}

pub fn names_protocol(classid: u8, subclassid: u8, protocolid: u8) -> Option<&'static str> {
    db()?
        .protocols
        .get(&(classid, subclassid, protocolid))
        .map(String::as_str)
}

// ---------------------------------------------------------------------------

/// Vendor name, or "" if unknown.
pub fn get_vendor_string(vid: u16) -> String {
    names_vendor(vid).unwrap_or("").to_string()
}

/// Product name, or "" if unknown.
pub fn get_product_string(vid: u16, pid: u16) -> String {
    names_product(vid, pid).unwrap_or("").to_string()
}

/// Class name, or "[unknown]".
pub fn get_class_string(cls: u8) -> String {
    names_class(cls).unwrap_or("[unknown]").to_string()
}

/// Subclass name, or "[unknown]".
pub fn get_subclass_string(cls: u8, subcls: u8) -> String {
    names_subclass(cls, subcls)
        .unwrap_or("[unknown]")
        .to_string()
}

/// Protocol name, or "".
pub fn get_protocol_string(cls: u8, subcls: u8, proto: u8) -> String {
    names_protocol(cls, subcls, proto).unwrap_or("").to_string()
}

/// Video terminal name, or "".
pub fn get_videoterminal_string(termt: u16) -> String {
    names_videoterminal(termt).unwrap_or("").to_string()
}

/// Get friendly vendor and product names from the name database.  If either
/// or both are not present, fall back to the device's own strings: from
/// sysfs where available (Linux, as upstream), otherwise from the device's
/// string descriptors.
pub fn get_vendor_product_with_fallback(dev: &Device, handle: Option<&Handle>) -> (String, String) {
    let desc = usb::device_descriptor(dev);

    let mut vendor = names_vendor(desc.id_vendor).map(str::to_string);
    let mut product = names_product(desc.id_vendor, desc.id_product).map(str::to_string);

    if let (Some(v), Some(p)) = (&vendor, &product) {
        return (v.clone(), p.clone());
    }

    if sysfs::available() {
        if let Some(name) = sysfs::get_sysfs_name(dev) {
            if vendor.is_none() {
                vendor = Some(sysfs::read_sysfs_prop(&name, "manufacturer"));
            }
            if product.is_none() {
                product = Some(sysfs::read_sysfs_prop(&name, "product"));
            }
        }
    } else {
        let opened;
        let handle = match handle {
            Some(h) => Some(h),
            None => {
                opened = dev.open().ok();
                opened.as_ref()
            }
        };
        if let Some(h) = handle {
            let read = |idx: u8| {
                Some(get_dev_string(Some(h), idx)).filter(|s| !s.is_empty() && s != "(error)")
            };
            if vendor.is_none() {
                vendor = read(desc.i_manufacturer);
            }
            if product.is_none() {
                product = read(desc.i_product);
            }
        }
    }

    (
        vendor.unwrap_or_else(|| "[unknown]".to_string()),
        product.unwrap_or_else(|| "[unknown]".to_string()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
# comment
1d6b  Linux Foundation
\t0002  2.0 root hub
\t0003  3.0 root hub
\t\t00  some interface
05ac  Apple, Inc.
\t12a8  iPhone 5/5C/5S/6/SE/7/8/X/XR

C 03  Human Interface Device
\t01  Boot Interface Subclass
\t\t01  Keyboard
\t\t02  Mouse
C 09  Hub
\t00  Unused
\t\t00  Full speed (or root) hub
AT 0101  USB Streaming
HID 21  HID
";

    #[test]
    fn parse_usb_ids() {
        let ids = UsbIds::parse(SAMPLE);
        assert_eq!(ids.vendors.get(&0x1d6b).unwrap(), "Linux Foundation");
        assert_eq!(ids.products.get(&(0x1d6b, 0x0003)).unwrap(), "3.0 root hub");
        assert_eq!(
            ids.products.get(&(0x05ac, 0x12a8)).unwrap(),
            "iPhone 5/5C/5S/6/SE/7/8/X/XR"
        );
        assert_eq!(ids.classes.get(&0x03).unwrap(), "Human Interface Device");
        assert_eq!(
            ids.subclasses.get(&(0x03, 0x01)).unwrap(),
            "Boot Interface Subclass"
        );
        assert_eq!(ids.protocols.get(&(0x03, 0x01, 0x02)).unwrap(), "Mouse");
        assert_eq!(
            ids.protocols.get(&(0x09, 0x00, 0x00)).unwrap(),
            "Full speed (or root) hub"
        );
        // Non vendor/class sections must not leak into the tables.
        assert_eq!(ids.vendors.len(), 2);
        assert_eq!(ids.classes.len(), 2);
    }

    #[test]
    fn spec_tables() {
        assert_eq!(names_hid(0x22), Some("Report"));
        assert_eq!(names_huts(0x01), Some("Generic Desktop Controls"));
        assert_eq!(names_hutus((0x01 << 16) + 0x02), Some("Mouse"));
        assert_eq!(names_audioterminal(0x0101), Some("USB Streaming"));
        assert_eq!(names_countrycode(0), Some("Not supported"));
        assert_eq!(names_reporttag(0x04), Some("Usage Page"));
    }
}
