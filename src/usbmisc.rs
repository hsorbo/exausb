// SPDX-License-Identifier: GPL-2.0-or-later
//! Misc USB routines.
//!
//! Derived from usbutils' `usbmisc.c` (Copyright (C) 2003 Aurelien Jarno).

use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use crate::sysfs::sanitize;
use crate::usb::{self, Context, Device, Handle};

const DEVBUSUSB: &str = "/dev/bus/usb";

/// SYMLOOP_MAX is typically 40 on Linux (MAXSYMLINKS).
const MAX_READLINK_DEPTH: usize = 40;

/// Timeout libusb uses for its string descriptor helpers.
const STRING_TIMEOUT: Duration = Duration::from_millis(1000);

/// Follow a chain of symlinks (relative links are resolved against the
/// directory containing the link).
fn readlink_recursive(path: &Path) -> PathBuf {
    let mut path = path.to_path_buf();
    for _ in 0..=MAX_READLINK_DEPTH {
        match std::fs::read_link(&path) {
            Ok(target) if target.is_absolute() => path = target,
            Ok(target) => {
                path = match path.parent() {
                    Some(dir) => dir.join(target),
                    None => target,
                }
            }
            Err(_) => break,
        }
    }
    path
}

/// Make a path absolute and lexically remove `.`, `..` and duplicate
/// slashes, without resolving symlinks.
fn get_absolute_path(path: &Path) -> PathBuf {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    };
    let mut out = PathBuf::from("/");
    for comp in path.components() {
        match comp {
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(c) => out.push(c),
            Component::RootDir | Component::CurDir | Component::Prefix(_) => {}
        }
    }
    out
}

/// Find the device corresponding to a `/dev/bus/usb/BBB/DDD` path (or a
/// symlink to one).
pub fn get_usb_device(ctx: &Context, path: &str) -> Option<Device> {
    use rusb::UsbContext;

    let absolute = get_absolute_path(&readlink_recursive(Path::new(path)));
    let list = ctx.devices().ok()?;
    list.iter().find(|dev| {
        let p = format!("{DEVBUSUSB}/{:03}/{:03}", dev.bus_number(), dev.address());
        Path::new(&p) == absolute
    })
}

/// `libusb_get_string_descriptor()`: a raw GET_DESCRIPTOR(STRING) request.
fn get_string_descriptor(
    dev: &Handle,
    index: u8,
    langid: u16,
    buf: &mut [u8],
) -> Result<usize, rusb::Error> {
    dev.read_control(
        usb::ENDPOINT_IN,
        usb::REQUEST_GET_DESCRIPTOR,
        (usb::DT_STRING as u16) << 8 | index as u16,
        langid,
        buf,
        STRING_TIMEOUT,
    )
}

fn get_any_langid(dev: &Handle) -> u16 {
    let mut buf = [0u8; 4];
    match get_string_descriptor(dev, 0, 0, &mut buf) {
        Ok(4) => buf[2] as u16 | (buf[3] as u16) << 8,
        _ => 0,
    }
}

/// Decode a UTF-16LE string; `None` if it isn't valid UTF-16.  Like the
/// iconv-based C version, the result ends at the first NUL.
fn usb_string_to_native(units: &[u8]) -> Option<String> {
    let units: Vec<u16> = units
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    let mut s = String::from_utf16(&units).ok()?;
    if let Some(nul) = s.find('\0') {
        s.truncate(nul);
    }
    Some(s)
}

/// Fetch string descriptor `id` from the device and convert it to UTF-8.
///
/// Returns "" when there's no device handle or the index is 0, and
/// "(error)" when the string can't be read.  Control characters are
/// replaced with '?'.
pub fn get_dev_string(dev: Option<&Handle>, id: u8) -> String {
    let Some(dev) = dev else {
        return String::new();
    };
    if id == 0 {
        return String::new();
    }

    let langid = get_any_langid(dev);
    if langid == 0 {
        return "(error)".to_string();
    }

    // Some devices lie about their string size, so initialize the buffer
    // with all 0 to account for that.
    let mut unicode_buf = [0u8; 254];
    match get_string_descriptor(dev, id, langid, &mut unicode_buf) {
        Ok(n) if n >= 2 => {}
        _ => return "(error)".to_string(),
    }
    if unicode_buf[0] < 2 || unicode_buf[1] != usb::DT_STRING {
        return "(error)".to_string();
    }

    let nchars = (unicode_buf[0] as usize - 2) / 2;
    let s = match usb_string_to_native(&unicode_buf[2..2 + nchars * 2]) {
        Some(s) => s,
        None => match dev.read_string_descriptor_ascii(id) {
            Ok(s) => s,
            Err(_) => return "(error)".to_string(),
        },
    };
    sanitize(&s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_decoding() {
        let raw: Vec<u8> = "Bl\u{e5}b\u{e6}r"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        assert_eq!(usb_string_to_native(&raw).unwrap(), "Bl\u{e5}b\u{e6}r");
        // Stops at NUL.
        assert_eq!(
            usb_string_to_native(&[b'a', 0, 0, 0, b'b', 0]).unwrap(),
            "a"
        );
        // Unpaired surrogate.
        assert!(usb_string_to_native(&[0x00, 0xd8]).is_none());
    }

    #[test]
    fn absolute_path() {
        assert_eq!(
            get_absolute_path(Path::new("/dev//bus/./usb/../usb/001/002")),
            PathBuf::from("/dev/bus/usb/001/002")
        );
    }
}
