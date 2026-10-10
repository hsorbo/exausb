// SPDX-License-Identifier: GPL-2.0-or-later
//! USB descriptor dumping.
//!
//! Derived from usbutils' `desc-dump.c`
//! (Copyright (C) 2017-2018 Michael Drake <michael.drake@codethink.co.uk>).

use crate::desc_defs::{Desc, DescType, DESC_UNDEFINED};
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

/// Print a description of a bmControls field value.
///
/// `BmControl1` has one bit per string; `BmControl2` has 2 bits per string,
/// with the additional bit specifying whether the control is read-only.
fn desc_bmcontrol_dump(bmcontrols: u64, strings: &[&str], two_bits: bool, indent: u32) {
    const SETTING: [&str; 3] = ["read-only", "ILLEGAL VALUE (0b10)", "read/write"];
    let pad = (indent * 2) as usize;

    for (count, s) in strings.iter().enumerate() {
        if s.is_empty() {
            continue;
        }
        if !two_bits {
            if (bmcontrols.checked_shr(count as u32).unwrap_or(0)) & 0x1 != 0 {
                println!("{:pad$}{} Control", "", s);
            }
        } else {
            let control = bmcontrols.checked_shr((count * 2) as u32).unwrap_or(0) & 0x3;
            if control != 0 {
                println!(
                    "{:pad$}{} Control ({})",
                    "",
                    s,
                    SETTING[control as usize - 1]
                );
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

/// Number of characters needed to render the largest array index, "[N]".
fn get_char_count_for_array_index(array_entries: u32) -> u32 {
    if array_entries > 100 {
        5
    } else if array_entries > 10 {
        4
    } else {
        3
    }
}

/// Render a field's name.
fn field_render(entry: u32, entries: u32, field_len: u32, current: &Desc, indent: u32) {
    let pad = (indent * 2) as usize;
    if current.array.array {
        let needed_chars = field_len
            .saturating_sub(get_char_count_for_array_index(entries))
            .saturating_sub(current.field.len() as u32) as usize;
        print!(
            "{:pad$}{}({}){:needed_chars$}",
            "", current.field, entry, ""
        );
    } else {
        print!("{:pad$}{:<w$}", "", current.field, w = field_len as usize);
    }
}

/// Print a value as hex, right-aligned within `width` characters.
fn hex_renderer(buf: &[u8], width: u32, offset: usize, bytes: u32) {
    let align = width.saturating_sub(bytes * 2) as usize;
    let digits = (bytes * 2) as usize;
    print!(
        " {:align$}0x{:0digits$x}",
        "",
        get_n_bytes_as_ull(buf, offset, bytes)
    );
}

/// Print a number: single bytes in decimal, anything else as hex.
fn number_renderer(buf: &[u8], width: u32, offset: usize, bytes: u32) {
    if bytes == 1 {
        print!("   {:>w$}", byte(buf, offset), w = width as usize);
    } else {
        hex_renderer(buf, width, offset, bytes);
    }
}

/// Render a field's value according to its type.
#[allow(clippy::too_many_arguments)]
fn value_renderer(
    dev: Option<&Handle>,
    current: &Desc,
    current_size: u32,
    buf: &[u8],
    buf_len: u32,
    desc: &[Desc],
    indent: u32,
    offset: usize,
) {
    // Maximum amount of characters to right align numerical values by.
    const SIZE_CHARS: u32 = 4;

    match current.ty {
        DescType::Number | DescType::Constant => {
            number_renderer(buf, SIZE_CHARS, offset, current_size);
            println!();
        }
        DescType::NumberPostfix(postfix) => {
            number_renderer(buf, SIZE_CHARS, offset, current_size);
            println!("{postfix}");
        }
        DescType::NumberStrings(strings) => {
            let value = get_n_bytes_as_ull(buf, offset, current_size);
            number_renderer(buf, SIZE_CHARS, offset, current_size);
            if let Some(s) = usize::try_from(value).ok().and_then(|v| strings.get(v)) {
                print!(" {s}");
            }
            println!();
        }
        DescType::Bcd => {
            let size = current_size as usize;
            let at = |i: usize| {
                (offset + size)
                    .checked_sub(1 + i)
                    .map_or(0, |idx| byte(buf, idx))
            };
            print!("  {:2x}", at(0));
            for i in 1..size {
                print!(".{:02x}", at(i));
            }
            println!();
        }
        DescType::Bitmap => {
            hex_renderer(buf, SIZE_CHARS, offset, current_size);
            println!();
        }
        DescType::BmControl1(strings) | DescType::BmControl2(strings) => {
            hex_renderer(buf, SIZE_CHARS, offset, current_size);
            println!();
            desc_bmcontrol_dump(
                get_n_bytes_as_ull(buf, offset, current_size),
                strings,
                matches!(current.ty, DescType::BmControl2(_)),
                indent + 1,
            );
        }
        DescType::BitmapStrings(strings) => {
            let value = get_n_bytes_as_ull(buf, offset, current_size);
            hex_renderer(buf, SIZE_CHARS, offset, current_size);
            println!();
            let pad = ((indent + 1) * 2) as usize;
            for (i, s) in strings.iter().enumerate() {
                let Some(s) = s else { continue };
                if value.checked_shr(i as u32).unwrap_or(0) & 0x1 == 0 {
                    continue;
                }
                println!("{:pad$}{}", "", s);
            }
        }
        DescType::StrDescIndex => {
            number_renderer(buf, SIZE_CHARS, offset, current_size);
            println!(" {}", get_dev_string(dev, byte(buf, offset)));
        }
        DescType::CsStrDescId => {
            number_renderer(buf, SIZE_CHARS, offset, current_size);
            // TODO: Add support for UAC3 class-specific String descriptor
            println!();
        }
        DescType::TerminalStr => {
            number_renderer(buf, SIZE_CHARS, offset, current_size);
            let term_name =
                names_audioterminal(get_n_bytes_as_ull(buf, offset, current_size) as u16);
            println!(" {}", term_name.unwrap_or("(unknown)"));
        }
        DescType::Extension { type_field, d } => {
            let ty = get_value_from_field(buf, buf_len, desc, type_field) as u32;
            // If the type didn't match a known type, use the undefined
            // descriptor.
            let ext_desc = d
                .iter()
                .find(|ext| ext.ty == ty)
                .map_or(DESC_UNDEFINED, |ext| ext.desc);
            desc_dump(
                dev,
                ext_desc,
                buf.get(offset..).unwrap_or(&[]),
                buf_len.saturating_sub(offset as u32),
                indent,
            );
        }
        DescType::Snowflake(f) => {
            number_renderer(buf, SIZE_CHARS, offset, current_size);
            f(get_n_bytes_as_ull(buf, offset, current_size), indent + 1);
        }
    }
}

fn dump_hex_bytes(buf: &[u8], from: usize, to: usize) {
    for i in from..to {
        print!("{:02x} ", byte(buf, i));
    }
    println!();
}

/// Dump descriptor `buf` according to the descriptor definition `desc`.
///
/// `buf_len` is the byte length of the descriptor data, or
/// [`DESC_BUF_LEN_FROM_BUF`] to read it from the first field.
pub fn desc_dump(dev: Option<&Handle>, desc: &[Desc], buf: &[u8], buf_len: u32, indent: u32) {
    let mut buf_len = buf_len;
    let mut field_len: u32 = 18;
    let mut offset: usize = 0;

    // Find the buffer length, if we've been instructed to read it from the
    // first field.
    if buf_len == DESC_BUF_LEN_FROM_BUF {
        if let Some(first) = desc.first() {
            buf_len = get_n_bytes_as_ull(buf, offset, first.size) as u32;
        }
    }

    // Increase `field_len` to be sufficient for character length of longest
    // field name for this descriptor.
    for current in desc {
        let mut needed_chars = 0;
        if current.array.array {
            let entries = get_array_entry_count(buf, buf_len, desc, current);
            needed_chars = get_char_count_for_array_index(entries);
        }
        field_len = field_len.max(current.field.len() as u32 + needed_chars);
    }

    let warn_pad = (indent.saturating_sub(1) * 2) as usize;

    // Step through each field, and dump it.
    for current in desc {
        let entries = if current.array.array {
            // Array type fields may have more than one entry.
            get_array_entry_count(buf, buf_len, desc, current)
        } else {
            1
        };

        let current_size = get_entry_size(buf, buf_len, desc, current);

        for entry in 0..entries {
            // Check there's enough data in buf for this entry.
            if offset + current_size as usize > buf_len as usize {
                println!(
                    "{:warn_pad$}Warning: Length insufficient for descriptor type.",
                    ""
                );
                dump_hex_bytes(buf, offset, buf_len as usize);
                return;
            }

            // Dump the field name.
            let is_extension = matches!(current.ty, DescType::Extension { .. });
            if !is_extension {
                field_render(entry, entries, field_len, current, indent);
            }

            // Dump the value.
            value_renderer(
                dev,
                current,
                current_size,
                buf,
                buf_len,
                desc,
                indent,
                offset,
            );

            if is_extension {
                // A desc extension consumes all remaining value buffer.
                offset = buf_len as usize;
            } else {
                // Advance offset in buffer.
                offset += current_size as usize;
            }
        }
    }

    // Check for junk at end of descriptor.
    if offset < buf_len as usize {
        println!(
            "{:warn_pad$}Warning: Junk at end of descriptor ({} bytes):",
            "",
            buf_len as usize - offset
        );
        print!("{:w$}", "", w = (indent * 2) as usize);
        dump_hex_bytes(buf, offset, buf_len as usize);
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
