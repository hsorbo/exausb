// SPDX-License-Identifier: GPL-2.0-or-later
//! Helpers for querying USB properties from sysfs (Linux only).
//!
//! Derived from usbutils' `sysfs.c`.

use std::path::Path;

use crate::usb::Device;

pub const SYS_BUS_USB_DEVICES: &str = "/sys/bus/usb/devices";

/// Whether the sysfs USB tree exists on this system.
pub fn available() -> bool {
    Path::new(SYS_BUS_USB_DEVICES).is_dir()
}

/// The sysfs name of a device: `usbN` for root hubs, `BUS-PORT.PORT...`
/// otherwise.  Returns `None` if the port path is too deep.
pub fn get_sysfs_name(dev: &Device) -> Option<String> {
    let bnum = dev.bus_number();
    let pnums = dev.port_numbers().ok()?;
    Some(sysfs_name(bnum, &pnums))
}

pub fn sysfs_name(bnum: u8, pnums: &[u8]) -> String {
    if pnums.is_empty() {
        // Special-case root devices.
        return format!("usb{bnum}");
    }
    let ports: Vec<String> = pnums.iter().map(u8::to_string).collect();
    format!("{bnum}-{}", ports.join("."))
}

/// Read a sysfs attribute of a USB device.  Returns "" if it can't be read.
/// The trailing newline is stripped and control characters are replaced
/// with '?'.
pub fn read_sysfs_prop(sysfs_name: &str, propname: &str) -> String {
    let path = format!("{SYS_BUS_USB_DEVICES}/{sysfs_name}/{propname}");
    let Ok(bytes) = std::fs::read(path) else {
        return String::new();
    };
    let mut s = String::from_utf8_lossy(&bytes).into_owned();
    if s.ends_with('\n') {
        s.pop();
    }
    sanitize(&s)
}

/// Replace control characters with '?', like upstream does for strings it
/// prints.
pub fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if (c as u32) < 0x20 || c == '\x7f' {
                '?'
            } else {
                c
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(sysfs_name(3, &[]), "usb3");
        assert_eq!(sysfs_name(1, &[4]), "1-4");
        assert_eq!(sysfs_name(2, &[1, 3, 2]), "2-1.3.2");
    }

    #[test]
    fn sanitize_control_chars() {
        assert_eq!(sanitize("a\tb\x7fc\u{e5}"), "a?b?c\u{e5}");
    }
}
