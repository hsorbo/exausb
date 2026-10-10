// SPDX-License-Identifier: GPL-2.0-or-later
//! USB descriptor dumping.
//!
//! Derived from usbutils' `desc-dump.c`
//! (Copyright (C) 2017-2018 Michael Drake <michael.drake@codethink.co.uk>).

use crate::desc_defs::{Desc, DescType, DESC_UNDEFINED};
use crate::model::{hex_bytes, Node};
use crate::names::names_audioterminal;
use crate::usb::Handle;
use crate::usbmisc::get_dev_string;

/// Buffer length value indicating that the buffer length should be read
/// from the value of the first field in the buffer.
pub const DESC_BUF_LEN_FROM_BUF: u32 = 0xffff_ffff;

/// Byte at `i`, or 0 if out of range.
fn byte(buf: &[u8], i: usize) -> u8 {
    buf.get(i).copied().unwrap_or(0)
}

/// Decode a bmControls value into "X Control" lines.
///
/// `BmControl1` has one bit per string; `BmControl2` has 2 bits per string,
/// with the additional bit specifying whether the control is read-only.
fn desc_bmcontrol_dump(bmcontrols: u64, strings: &[&str], two_bits: bool, field: &mut Node) {
    const SETTING: [&str; 3] = ["read-only", "ILLEGAL VALUE (0b10)", "read/write"];

    for (count, s) in strings.iter().enumerate() {
        if s.is_empty() {
            continue;
        }
        if !two_bits {
            if (bmcontrols.checked_shr(count as u32).unwrap_or(0)) & 0x1 != 0 {
                field.text(format!("{s} Control"));
            }
        } else {
            let control = bmcontrols.checked_shr((count * 2) as u32).unwrap_or(0) & 0x3;
            if control != 0 {
                field.text(format!("{s} Control ({})", SETTING[control as usize - 1]));
            }
        }
    }
}

/// Read up to 8 little-endian bytes from `buf` at `offset`.
fn get_n_bytes_as_ull(buf: &[u8], offset: usize, bytes: u32) -> u64 {
    let bytes = bytes.min(8) as usize;
    (0..bytes).fold(0u64, |acc, i| {
        acc | (byte(buf, offset + i) as u64) << (8 * i)
    })
}

/// Read the value of the field named `field`.
fn get_value_from_field(buf: &[u8], buf_len: u32, desc: &[Desc], field: &str) -> u64 {
    let mut offset: usize = 0;
    for current in desc {
        if current.field == field {
            if offset + current.size as usize <= buf_len as usize {
                return get_n_bytes_as_ull(buf, offset, current.size);
            }
            return 0;
        }
        // Keep track of our offset in the descriptor data as we look for
        // the field we want.
        offset += get_entry_size(buf, buf_len, desc, current) as usize;
    }
    0
}

/// Size of a field in bytes; variable-size fields take it from another field.
fn get_entry_size(buf: &[u8], buf_len: u32, desc: &[Desc], entry: &Desc) -> u32 {
    if let Some(size_field) = entry.size_field {
        return get_value_from_field(buf, buf_len, desc, size_field) as u32;
    }
    if entry.size == 0 {
        eprintln!(
            "Bad descriptor definition; '{}' field has zero size.",
            entry.field
        );
        std::process::exit(1);
    }
    entry.size
}

/// Number of entries in an array field, either from its length field(s) or
/// inferred from the descriptor length.  Returns `0xffffffff` if the
/// definition has an unresolvable ambiguity.
fn get_array_entry_count(buf: &[u8], buf_len: u32, desc: &[Desc], array_entry: &Desc) -> u32 {
    if let Some(length_field1) = array_entry.array.length_field1 {
        // We can get the array size from the length_field1.
        let mut entries = get_value_from_field(buf, buf_len, desc, length_field1) as u32;

        if let Some(length_field2) = array_entry.array.length_field2 {
            // There's a second field specifying length.  The two lengths
            // are multiplied.
            entries = entries
                .wrapping_mul(get_value_from_field(buf, buf_len, desc, length_field2) as u32);
        }

        // If the bits flag is set, then the entry count so far was a bit
        // count, and we need to get a byte count.
        if array_entry.array.bits {
            entries = (entries / 8) + u32::from(entries & 0x7 != 0);
        }
        entries
    } else {
        // Inferred array length.  Start with the descriptor's byte-length,
        // and subtract the sizes of all the other fields.
        let mut size = buf_len;

        for current in desc {
            if std::ptr::eq(current, array_entry) {
                continue;
            }

            let field_size = if current.array.array {
                // We can't deal with two inferred-length arrays in one
                // descriptor definition.
                if current.array.length_field1.is_none() {
                    return 0xffff_ffff;
                }
                let count = get_array_entry_count(buf, buf_len, desc, current);
                if count == 0xffff_ffff {
                    eprintln!("Bad descriptor definition; multiple inferred-length arrays.");
                    std::process::exit(1);
                }
                get_entry_size(buf, buf_len, desc, current).wrapping_mul(count)
            } else {
                get_entry_size(buf, buf_len, desc, current)
            };

            if field_size > size {
                return 0;
            }
            size -= field_size;
        }

        let field_size = get_entry_size(buf, buf_len, desc, array_entry);
        size.checked_div(field_size).unwrap_or(0)
    }
}

/// A number: single bytes in decimal, anything else as zero-padded hex.
fn number(buf: &[u8], offset: usize, bytes: u32) -> String {
    let value = get_n_bytes_as_ull(buf, offset, bytes);
    if bytes == 1 {
        value.to_string()
    } else {
        hex(value, bytes)
    }
}

fn hex(value: u64, bytes: u32) -> String {
    format!("0x{:0w$x}", value, w = (bytes * 2) as usize)
}

/// Add a field to `node` with its value rendered according to its type.
#[allow(clippy::too_many_arguments)]
fn value_renderer(
    dev: Option<&Handle>,
    current: &Desc,
    name: String,
    size: u32,
    buf: &[u8],
    buf_len: u32,
    desc: &[Desc],
    offset: usize,
    node: &mut Node,
) {
    let value = get_n_bytes_as_ull(buf, offset, size);

    match current.ty {
        DescType::Number | DescType::Constant => {
            node.field(name, number(buf, offset, size));
        }
        DescType::NumberPostfix(postfix) => {
            node.field(name, number(buf, offset, size))
                .note(postfix.trim());
        }
        DescType::NumberStrings(strings) => {
            let s = usize::try_from(value)
                .ok()
                .and_then(|v| strings.get(v))
                .copied();
            node.field(name, number(buf, offset, size))
                .note(s.unwrap_or(""));
        }
        DescType::Bcd => {
            let size = size as usize;
            let at = |i: usize| {
                (offset + size)
                    .checked_sub(1 + i)
                    .map_or(0, |idx| byte(buf, idx))
            };
            let mut s = format!("{:x}", at(0));
            for i in 1..size {
                s += &format!(".{:02x}", at(i));
            }
            node.field(name, s);
        }
        DescType::Bitmap => {
            node.field(name, hex(value, size));
        }
        DescType::BmControl1(strings) | DescType::BmControl2(strings) => {
            let field = node.field(name, hex(value, size));
            let two_bits = matches!(current.ty, DescType::BmControl2(_));
            desc_bmcontrol_dump(value, strings, two_bits, field);
        }
        DescType::BitmapStrings(strings) => {
            let field = node.field(name, hex(value, size));
            for (i, s) in strings.iter().enumerate() {
                if let Some(s) = s {
                    if value.checked_shr(i as u32).unwrap_or(0) & 0x1 != 0 {
                        field.text(*s);
                    }
                }
            }
        }
        DescType::StrDescIndex => {
            let s = get_dev_string(dev, byte(buf, offset));
            node.field(name, number(buf, offset, size)).note(s);
        }
        DescType::CsStrDescId => {
            // TODO: Add support for UAC3 class-specific String descriptor
            node.field(name, number(buf, offset, size));
        }
        DescType::TerminalStr => {
            let term_name = names_audioterminal(value as u16);
            node.field(name, number(buf, offset, size))
                .note(term_name.unwrap_or(""));
        }
        DescType::Extension { type_field, d } => {
            let ty = get_value_from_field(buf, buf_len, desc, type_field) as u32;
            // If the type didn't match a known type, use the undefined
            // descriptor (whose bytes then show up as junk).
            let ext_desc = d
                .iter()
                .find(|ext| ext.ty == ty)
                .map_or(DESC_UNDEFINED, |ext| ext.desc);
            desc_dump(
                dev,
                ext_desc,
                buf.get(offset..).unwrap_or(&[]),
                buf_len.saturating_sub(offset as u32),
                node,
            );
        }
        DescType::Snowflake(f) => {
            f(value, node.field(name, number(buf, offset, size)));
        }
    }
}

/// Decode descriptor `buf` according to the definition `desc`, adding a
/// field to `node` for each value.
///
/// `buf_len` is the byte length of the descriptor data, or
/// [`DESC_BUF_LEN_FROM_BUF`] to read it from the first field.
pub fn desc_dump(dev: Option<&Handle>, desc: &[Desc], buf: &[u8], buf_len: u32, node: &mut Node) {
    let mut buf_len = buf_len;
    let mut offset: usize = 0;

    // Find the buffer length, if we've been instructed to read it from the
    // first field.
    if buf_len == DESC_BUF_LEN_FROM_BUF {
        if let Some(first) = desc.first() {
            buf_len = get_n_bytes_as_ull(buf, offset, first.size) as u32;
        }
    }

    for current in desc {
        let entries = if current.array.array {
            // Array type fields may have more than one entry.
            get_array_entry_count(buf, buf_len, desc, current)
        } else {
            1
        };

        let size = get_entry_size(buf, buf_len, desc, current);

        for entry in 0..entries {
            // Check there's enough data in buf for this entry.
            if offset + size as usize > buf_len as usize {
                let rest = buf.get(offset..buf_len as usize).unwrap_or(&[]);
                let mut msg = format!("Descriptor too short for {}", current.field);
                if !rest.is_empty() {
                    msg += &format!(": {}", hex_bytes(rest));
                }
                node.warn(msg);
                return;
            }

            let name = if current.array.array {
                format!("{}[{entry}]", current.field)
            } else {
                current.field.to_string()
            };
            value_renderer(dev, current, name, size, buf, buf_len, desc, offset, node);

            if matches!(current.ty, DescType::Extension { .. }) {
                // A desc extension consumes all remaining value buffer.
                offset = buf_len as usize;
            } else {
                offset += size as usize;
            }
        }
    }

    // Check for junk at end of descriptor.
    if offset < buf_len as usize {
        let junk = buf.get(offset..buf_len as usize).unwrap_or(&[]);
        node.warn(format!("Junk at end of descriptor: {}", hex_bytes(junk)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn n_bytes() {
        let buf = [0x34, 0x12, 0x78, 0x56, 0xff];
        assert_eq!(get_n_bytes_as_ull(&buf, 0, 2), 0x1234);
        assert_eq!(get_n_bytes_as_ull(&buf, 0, 4), 0x5678_1234);
        assert_eq!(get_n_bytes_as_ull(&buf, 3, 4), 0xff56); // out of range reads as 0
    }

    static TEST_DESC: &[Desc] = &[
        Desc::new("bLength", 1, DescType::Number),
        Desc::new("bNrInPins", 1, DescType::Number),
        Desc::new("baSourceID", 1, DescType::Number).array_len("bNrInPins"),
        Desc::new("bmControls", 2, DescType::Bitmap),
        Desc::new("baTail", 1, DescType::Number).array(),
    ];

    #[test]
    fn array_counts() {
        // bLength=9, 3 source pins, 2 bytes controls, remaining 2 bytes tail.
        let buf = [9u8, 3, 1, 2, 3, 0xaa, 0xbb, 7, 8];
        assert_eq!(get_array_entry_count(&buf, 9, TEST_DESC, &TEST_DESC[2]), 3);
        assert_eq!(get_array_entry_count(&buf, 9, TEST_DESC, &TEST_DESC[4]), 2);
        assert_eq!(get_value_from_field(&buf, 9, TEST_DESC, "bNrInPins"), 3);
        // Like upstream, field lookup steps over arrays as if they had one
        // entry; length fields always precede arrays in real definitions.
        assert_eq!(
            get_value_from_field(&buf, 9, TEST_DESC, "bmControls"),
            0x0302
        );
    }
}
