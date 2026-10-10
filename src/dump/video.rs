// SPDX-License-Identifier: GPL-2.0-or-later
//! Video class descriptor dumping.

use super::{ep_dir, get_guid, junk, too_short, USB_DT_CS_ENDPOINT, USB_DT_CS_INTERFACE};
use crate::model::{hex2, hex4, hex8, hex_bytes, Node};
use crate::names::names_videoterminal;
use crate::usb::{le16, le32, Handle};
use crate::usbmisc::get_dev_string;

const USB_VIDEO_PROTOCOL_15: u8 = 0x01;

const CTRLNAMES: [&str; 19] = [
    "Brightness",
    "Contrast",
    "Hue",
    "Saturation",
    "Sharpness",
    "Gamma",
    "White Balance Temperature",
    "White Balance Component",
    "Backlight Compensation",
    "Gain",
    "Power Line Frequency",
    "Hue, Auto",
    "White Balance Temperature, Auto",
    "White Balance Component, Auto",
    "Digital Multiplier",
    "Digital Multiplier Limit",
    "Analog Video Standard",
    "Analog Video Lock Status",
    "Contrast, Auto",
];

const CAMCTRLNAMES: [&str; 22] = [
    "Scanning Mode",
    "Auto-Exposure Mode",
    "Auto-Exposure Priority",
    "Exposure Time (Absolute)",
    "Exposure Time (Relative)",
    "Focus (Absolute)",
    "Focus (Relative)",
    "Iris (Absolute)",
    "Iris (Relative)",
    "Zoom (Absolute)",
    "Zoom (Relative)",
    "PanTilt (Absolute)",
    "PanTilt (Relative)",
    "Roll (Absolute)",
    "Roll (Relative)",
    "Reserved",
    "Reserved",
    "Focus, Auto",
    "Privacy",
    "Focus, Simple",
    "Window",
    "Region of Interest",
];

const ENCTRLNAMES: [&str; 20] = [
    "Select Layer",
    "Profile and Toolset",
    "Video Resolution",
    "Minimum Frame Interval",
    "Slice Mode",
    "Rate Control Mode",
    "Average Bit Rate",
    "CPB Size",
    "Peak Bit Rate",
    "Quantization Parameter",
    "Synchronization and Long-Term Reference Frame",
    "Long-Term Buffer",
    "Picture Long-Term Reference",
    "LTR Validation",
    "Level IDC",
    "SEI Message",
    "QP Range",
    "Priority ID",
    "Start or Stop Layer/View",
    "Error Resiliency",
];

const STDNAMES: [&str; 6] = [
    "None",
    "NTSC - 525/60",
    "PAL - 625/50",
    "SECAM - 625/50",
    "NTSC - 625/50",
    "PAL - 525/60",
];

/// Add the names of the set bits among the first `count` bits of `value`.
fn bit_names(field: &mut Node, value: u32, names: &[&str], count: usize) {
    for (i, name) in names.iter().enumerate().take(count) {
        if (value >> i) & 1 != 0 {
            field.text(*name);
        }
    }
}

/// Collect up to three little-endian bitmap bytes ending just before `end`.
fn controls(buf: &[u8], end: usize, size: usize) -> u32 {
    (0..size.min(3)).fold(0, |ctrls, i| (ctrls << 8) | buf[end - i - 1] as u32)
}

fn yes_no(set: bool) -> &'static str {
    if set {
        "Yes"
    } else {
        "No"
    }
}

/// Add the common descriptor header fields, with the subtype's name.
fn header_fields(buf: &[u8], subtype: &str, s: &mut Node) {
    s.field("bLength", buf[0]);
    s.field("bDescriptorType", buf[1]);
    s.field("bDescriptorSubtype", buf[2]).note(subtype);
}

/// Warn about an unknown subtype, showing the bytes after the header.
fn invalid_subtype(buf: &[u8], s: &mut Node) {
    s.warn(format!(
        "Invalid descriptor subtype: {}",
        hex_bytes(&buf[3..buf[0] as usize])
    ));
}

pub(super) fn dump_videocontrol_interface(
    dev: Option<&Handle>,
    buf: &[u8],
    protocol: u8,
    node: &mut Node,
) {
    let subtype = match buf[2] {
        0x01 => "HEADER",
        0x02 => "INPUT_TERMINAL",
        0x03 => "OUTPUT_TERMINAL",
        0x04 => "SELECTOR_UNIT",
        0x05 => "PROCESSING_UNIT",
        0x06 => "EXTENSION_UNIT",
        0x07 => "ENCODING_UNIT",
        _ => "unknown",
    };
    let s = node.section("VideoControl Interface");
    if buf[1] != USB_DT_CS_INTERFACE {
        s.warn("Invalid descriptor type");
    }
    if buf[0] < 3 {
        too_short(s);
        return;
    }
    s.note(subtype);
    header_fields(buf, subtype, s);

    let len = buf[0] as usize;
    match buf[2] {
        0x01 => {
            // HEADER
            if len < 12 {
                too_short(s);
                return;
            }
            let n = buf[11] as usize;
            if len < 12 + n {
                too_short(s);
                return;
            }
            let freq = le32(&buf[7..]);
            s.field("bcdUVC", format!("{:x}.{:02x}", buf[4], buf[3]));
            s.field("wTotalLength", hex4(le16(&buf[5..])));
            s.field(
                "dwClockFrequency",
                format!("{}.{:06}MHz", freq / 1_000_000, freq % 1_000_000),
            );
            s.field("bInCollection", n);
            for i in 0..n {
                s.field(format!("baInterfaceNr[{i}]"), buf[12 + i]);
            }
            junk(buf, 12 + n, s);
        }
        0x02 => {
            // INPUT_TERMINAL
            if len < 8 {
                too_short(s);
                return;
            }
            let term = get_dev_string(dev, buf[7]);
            let termt = le16(&buf[4..]);
            // Camera terminals carry extra fields.
            let mut n = if termt == 0x0201 { 7 } else { 0 };
            if len < 8 + n {
                too_short(s);
                return;
            }
            s.field("bTerminalID", buf[3]);
            s.field("wTerminalType", hex4(termt))
                .note(names_videoterminal(termt as u16).unwrap_or(""));
            s.field("bAssocTerminal", buf[6]);
            s.field("iTerminal", buf[7]).note(term);
            if termt == 0x0201 {
                n += buf[14] as usize;
                if len < 8 + n {
                    too_short(s);
                    return;
                }
                s.field("wObjectiveFocalLengthMin", le16(&buf[8..]));
                s.field("wObjectiveFocalLengthMax", le16(&buf[10..]));
                s.field("wOcularFocalLength", le16(&buf[12..]));
                s.field("bControlSize", buf[14]);
                let ctrls = controls(buf, 8 + n, buf[14] as usize);
                let count = if protocol == USB_VIDEO_PROTOCOL_15 {
                    22
                } else {
                    19
                };
                let f = s.field("bmControls", hex8(ctrls));
                bit_names(f, ctrls, &CAMCTRLNAMES, count);
            }
            junk(buf, 8 + n, s);
        }
        0x03 => {
            // OUTPUT_TERMINAL
            if len < 9 {
                too_short(s);
                return;
            }
            let term = get_dev_string(dev, buf[8]);
            let termt = le16(&buf[4..]);
            s.field("bTerminalID", buf[3]);
            s.field("wTerminalType", hex4(termt))
                .note(names_videoterminal(termt as u16).unwrap_or(""));
            s.field("bAssocTerminal", buf[6]);
            s.field("bSourceID", buf[7]);
            s.field("iTerminal", buf[8]).note(term);
            junk(buf, 9, s);
        }
        0x04 => {
            // SELECTOR_UNIT
            if len < 5 {
                too_short(s);
                return;
            }
            let p = buf[4] as usize;
            if len < 6 + p {
                too_short(s);
                return;
            }
            let term = get_dev_string(dev, buf[5 + p]);
            s.field("bUnitID", buf[3]);
            s.field("bNrInPins", p);
            for i in 0..p {
                s.field(format!("baSource[{i}]"), buf[5 + i]);
            }
            s.field("iSelector", buf[5 + p]).note(term);
            junk(buf, 6 + p, s);
        }
        0x05 => {
            // PROCESSING_UNIT
            if len < 8 {
                too_short(s);
                return;
            }
            let n = buf[7] as usize;
            // Upstream means to require 10 + n bytes for UVC >= 1.1, but
            // the version it checks is only set while dumping a HEADER, so
            // it always requires 9 + n.
            let p = 9;
            if len < p + n {
                s.warn(format!(
                    "Descriptor too short ({} bytes, needs {})",
                    len,
                    p + n
                ));
                return;
            }
            let term = get_dev_string(dev, buf[8 + n]);
            s.field("bUnitID", buf[3]);
            s.field("bSourceID", buf[4]);
            s.field("wMaxMultiplier", le16(&buf[5..]));
            s.field("bControlSize", n);
            let ctrls = controls(buf, 8 + n, n);
            let count = if protocol == USB_VIDEO_PROTOCOL_15 {
                19
            } else {
                18
            };
            let f = s.field("bmControls", hex8(ctrls));
            bit_names(f, ctrls, &CTRLNAMES, count);
            let stds = buf[9 + n];
            s.field("iProcessing", buf[8 + n]).note(term);
            let f = s.field("bmVideoStandards", hex2(stds));
            bit_names(f, stds as u32, &STDNAMES, 6);
        }
        0x06 => {
            // EXTENSION_UNIT
            if len < 22 {
                too_short(s);
                return;
            }
            let p = buf[21] as usize;
            if len < 23 + p {
                too_short(s);
                return;
            }
            let n = buf[22 + p] as usize;
            if len < 24 + p + n {
                too_short(s);
                return;
            }
            let term = get_dev_string(dev, buf[23 + p + n]);
            s.field("bUnitID", buf[3]);
            s.field("guidExtensionCode", get_guid(&buf[4..]));
            s.field("bNumControls", buf[20]);
            s.field("bNrInPins", buf[21]);
            for i in 0..p {
                s.field(format!("baSourceID[{i}]"), buf[22 + i]);
            }
            s.field("bControlSize", buf[22 + p]);
            for i in 0..n {
                s.field(format!("bmControls[{i}]"), hex2(buf[23 + p + i]));
            }
            s.field("iExtension", buf[23 + p + n]).note(term);
            junk(buf, 24 + p + n, s);
        }
        0x07 => {
            // ENCODING UNIT
            if len < 13 {
                too_short(s);
                return;
            }
            let term = get_dev_string(dev, buf[5]);
            s.field("bUnitID", buf[3]);
            s.field("bSourceID", buf[4]);
            s.field("iEncoding", buf[5]).note(term);
            s.field("bControlSize", buf[6]);
            let mut ctrls = controls(buf, 10, 3);
            let f = s.field("bmControls", hex8(ctrls));
            bit_names(f, ctrls, &ENCTRLNAMES, 20);
            // Note: like upstream, `ctrls` is not reset here.
            for i in 0..3 {
                ctrls = (ctrls << 8) | buf[12 - i] as u32;
            }
            let f = s.field("bmControlsRuntime", hex8(ctrls));
            bit_names(f, ctrls, &ENCTRLNAMES, 20);
        }
        _ => invalid_subtype(buf, s),
    }
}

pub(super) fn dump_videocontrol_interrupt_endpoint(buf: &[u8], node: &mut Node) {
    let subtype = if buf[2] == 3 {
        "EP_INTERRUPT"
    } else {
        "Invalid"
    };
    let s = node.section("VideoControl Endpoint");
    if buf[1] != USB_DT_CS_ENDPOINT {
        s.warn("Invalid descriptor type");
    }
    if buf[0] < 5 {
        too_short(s);
        return;
    }
    s.note(subtype);
    header_fields(buf, subtype, s);
    s.field("wMaxTransferSize", le16(&buf[3..]));
    junk(buf, 5, s);
}

/// Decode `bmInterlaceFlags` into text under its field.
fn interlace_flags(flags: u8, one_field_bit_means_one: bool, s: &mut Node) {
    let f = s.field("bmInterlaceFlags", hex2(flags));
    f.text(format!(
        "Interlaced stream or variable: {}",
        yes_no(flags & (1 << 0) != 0)
    ));
    // Upstream disagrees with itself on the meaning of bit 1 between the
    // uncompressed/frame-based and MJPEG formats; keep that behaviour.
    let fields = match (flags & (1 << 1) != 0, one_field_bit_means_one) {
        (true, true) | (false, false) => 1,
        _ => 2,
    };
    f.text(format!("Fields per frame: {fields}"));
    f.text(format!("Field 1 first: {}", yes_no(flags & (1 << 2) != 0)));
    f.text(format!(
        "Field pattern: {}",
        match (flags >> 4) & 0x03 {
            0 => "Field 1 only",
            1 => "Field 2 only",
            2 => "Regular pattern of fields 1 and 2",
            _ => "Random pattern of fields 1 and 2",
        }
    ));
}

/// `bEndpointAddress` with its number and direction.
fn endpoint_field(addr: u8, s: &mut Node) {
    s.field("bEndpointAddress", hex2(addr))
        .note(format!("EP {} {}", addr & 0x0f, ep_dir(addr)));
}

pub(super) fn dump_videostreaming_interface(buf: &[u8], node: &mut Node) {
    const COLOR_PRIMS: [&str; 6] = [
        "Unspecified",
        "BT.709,sRGB",
        "BT.470-2 (M)",
        "BT.470-2 (B,G)",
        "SMPTE 170M",
        "SMPTE 240M",
    ];
    const TRANSFER_CHARS: [&str; 8] = [
        "Unspecified",
        "BT.709",
        "BT.470-2 (M)",
        "BT.470-2 (B,G)",
        "SMPTE 170M",
        "SMPTE 240M",
        "Linear",
        "sRGB",
    ];
    const MATRIX_COEFFS: [&str; 6] = [
        "Unspecified",
        "BT.709",
        "FCC",
        "BT.470-2 (B,G)",
        "SMPTE 170M (BT.601)",
        "SMPTE 240M",
    ];

    let subtype = match buf[2] {
        0x01 => "INPUT_HEADER",
        0x02 => "OUTPUT_HEADER",
        0x03 => "STILL_IMAGE_FRAME",
        0x04 => "FORMAT_UNCOMPRESSED",
        0x05 => "FRAME_UNCOMPRESSED",
        0x06 => "FORMAT_MJPEG",
        0x07 => "FRAME_MJPEG",
        0x0a => "FORMAT_MPEG2TS",
        0x0d => "COLORFORMAT",
        0x10 => "FORMAT_FRAME_BASED",
        0x11 => "FRAME_FRAME_BASED",
        0x12 => "FORMAT_STREAM_BASED",
        _ => "unknown",
    };
    let s = node.section("VideoStreaming Interface");
    if buf[1] != USB_DT_CS_INTERFACE {
        s.warn("Invalid descriptor type");
    }
    if buf[0] < 3 {
        too_short(s);
        return;
    }
    s.note(subtype);
    header_fields(buf, subtype, s);

    let blen = buf[0] as usize;
    match buf[2] {
        0x01 => {
            // INPUT_HEADER
            if blen < 13 {
                too_short(s);
                return;
            }
            let p = buf[3] as usize;
            let n = buf[12] as usize;
            if blen < 13 + p * n {
                too_short(s);
                return;
            }
            s.field("bNumFormats", p);
            s.field("wTotalLength", hex4(le16(&buf[4..])));
            endpoint_field(buf[6], s);
            s.field("bmInfo", buf[7]);
            s.field("bTerminalLink", buf[8]);
            s.field("bStillCaptureMethod", buf[9]);
            s.field("bTriggerSupport", buf[10]);
            s.field("bTriggerUsage", buf[11]);
            s.field("bControlSize", n);
            for i in 0..p {
                s.field(format!("bmaControls[{i}]"), buf[13 + i * n]);
            }
            junk(buf, 13 + p * n, s);
        }
        0x02 => {
            // OUTPUT_HEADER
            if blen < 9 {
                too_short(s);
                return;
            }
            let p = buf[3] as usize;
            let n = buf[8] as usize;
            if blen < 9 + p * n {
                too_short(s);
                return;
            }
            s.field("bNumFormats", p);
            s.field("wTotalLength", hex4(le16(&buf[4..])));
            endpoint_field(buf[6], s);
            s.field("bTerminalLink", buf[7]);
            s.field("bControlSize", n);
            for i in 0..p {
                s.field(format!("bmaControls[{i}]"), buf[9 + i * n]);
            }
            junk(buf, 9 + p * n, s);
        }
        0x03 => {
            // STILL_IMAGE_FRAME
            if blen < 5 {
                too_short(s);
                return;
            }
            let n = buf[4] as usize;
            if blen < 6 + 4 * n {
                too_short(s);
                return;
            }
            let m = buf[5 + 4 * n] as usize;
            if blen < 6 + 4 * n + m {
                too_short(s);
                return;
            }
            endpoint_field(buf[3], s);
            s.field("bNumImageSizePatterns", n);
            for i in 0..n {
                s.field(format!("wWidth[{i}]"), le16(&buf[5 + 4 * i..]));
                s.field(format!("wHeight[{i}]"), le16(&buf[7 + 4 * i..]));
            }
            s.field("bNumCompressionPatterns", m);
            for i in 0..m {
                s.field(format!("bCompression[{i}]"), buf[6 + 4 * n + i]);
            }
            junk(buf, 6 + 4 * n + m, s);
        }
        0x04 | 0x10 => {
            // FORMAT_UNCOMPRESSED / FORMAT_FRAME_BASED
            let len = if buf[2] == 0x04 { 27 } else { 28 };
            if blen < len {
                too_short(s);
                return;
            }
            s.field("bFormatIndex", buf[3]);
            s.field("bNumFrameDescriptors", buf[4]);
            s.field("guidFormat", get_guid(&buf[5..]));
            s.field("bBitsPerPixel", buf[21]);
            s.field("bDefaultFrameIndex", buf[22]);
            s.field("bAspectRatioX", buf[23]);
            s.field("bAspectRatioY", buf[24]);
            interlace_flags(buf[25], true, s);
            s.field("bCopyProtect", buf[26]);
            if buf[2] == 0x10 {
                s.field("bVariableSize", buf[27]);
            }
            junk(buf, len, s);
        }
        0x05 | 0x07 | 0x11 => {
            // FRAME_UNCOMPRESSED / FRAME_MJPEG / FRAME_FRAME_BASED
            // `n` is the offset of bFrameIntervalType.
            let n = if buf[2] == 0x11 { 21 } else { 25 };
            if blen < n + 1 {
                too_short(s);
                return;
            }
            // Zero intervals means a continuous range (min, max, step).
            let len = if buf[n] != 0 {
                26 + buf[n] as usize * 4
            } else {
                38
            };
            if blen < len {
                too_short(s);
                return;
            }
            let flags = buf[4];
            s.field("bFrameIndex", buf[3]);
            let f = s.field("bmCapabilities", hex2(flags));
            f.text(if flags & (1 << 0) != 0 {
                "Still image supported"
            } else {
                "Still image unsupported"
            });
            if flags & (1 << 1) != 0 {
                f.text("Fixed frame-rate");
            }
            s.field("wWidth", le16(&buf[5..]));
            s.field("wHeight", le16(&buf[7..]));
            s.field("dwMinBitRate", le32(&buf[9..]));
            s.field("dwMaxBitRate", le32(&buf[13..]));
            if buf[2] == 0x11 {
                s.field("dwDefaultFrameInterval", le32(&buf[17..]));
                s.field("bFrameIntervalType", buf[21]);
                s.field("dwBytesPerLine", le32(&buf[22..]));
            } else {
                s.field("dwMaxVideoFrameBufferSize", le32(&buf[17..]));
                s.field("dwDefaultFrameInterval", le32(&buf[21..]));
                s.field("bFrameIntervalType", buf[25]);
            }
            if buf[n] == 0 {
                s.field("dwMinFrameInterval", le32(&buf[26..]));
                s.field("dwMaxFrameInterval", le32(&buf[30..]));
                s.field("dwFrameIntervalStep", le32(&buf[34..]));
            } else {
                for i in 0..buf[n] as usize {
                    s.field(format!("dwFrameInterval[{i}]"), le32(&buf[26 + 4 * i..]));
                }
            }
            junk(buf, len, s);
        }
        0x06 => {
            // FORMAT_MJPEG
            if blen < 11 {
                too_short(s);
                return;
            }
            let flags = buf[5];
            s.field("bFormatIndex", buf[3]);
            s.field("bNumFrameDescriptors", buf[4]);
            s.field("bFlags", flags).text(format!(
                "Fixed-size samples: {}",
                yes_no(flags & (1 << 0) != 0)
            ));
            s.field("bDefaultFrameIndex", buf[6]);
            s.field("bAspectRatioX", buf[7]);
            s.field("bAspectRatioY", buf[8]);
            interlace_flags(buf[9], false, s);
            s.field("bCopyProtect", buf[10]);
            junk(buf, 11, s);
        }
        0x0a => {
            // FORMAT_MPEG2TS
            let len = if blen < 23 { 7 } else { 23 };
            if blen < len {
                too_short(s);
                return;
            }
            s.field("bFormatIndex", buf[3]);
            s.field("bDataOffset", buf[4]);
            s.field("bPacketLength", buf[5]);
            s.field("bStrideLength", buf[6]);
            if len > 7 {
                s.field("guidStrideFormat", get_guid(&buf[7..]));
            }
            junk(buf, len, s);
        }
        0x0d => {
            // COLORFORMAT
            if blen < 6 {
                too_short(s);
                return;
            }
            let name =
                |table: &[&'static str], v: u8| table.get(v as usize).copied().unwrap_or("Unknown");
            s.field("bColorPrimaries", buf[3])
                .note(name(&COLOR_PRIMS, buf[3]));
            s.field("bTransferCharacteristics", buf[4])
                .note(name(&TRANSFER_CHARS, buf[4]));
            s.field("bMatrixCoefficients", buf[5])
                .note(name(&MATRIX_COEFFS, buf[5]));
            junk(buf, 6, s);
        }
        0x12 => {
            // FORMAT_STREAM_BASED
            if blen < 24 {
                too_short(s);
                return;
            }
            s.field("bFormatIndex", buf[3]);
            s.field("guidFormat", get_guid(&buf[4..]));
            // Like upstream, only the first byte of dwPacketLength.
            s.field("dwPacketLength", buf[20]);
            junk(buf, 24, s);
        }
        _ => invalid_subtype(buf, s),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usb::padded;

    #[test]
    fn videocontrol_header() {
        let mut node = Node::new("x");
        let buf = padded(&[
            13, 0x24, 0x01, 0x00, 0x01, 0x4d, 0x00, 0x00, 0x6c, 0xdc, 0x02, 1, 1,
        ]);
        dump_videocontrol_interface(None, &buf, 0, &mut node);
        let s = node.get("VideoControl Interface").unwrap();
        assert_eq!(s.note, "HEADER");
        assert_eq!(s.get("bcdUVC").unwrap().value, "1.00");
        assert_eq!(s.get("wTotalLength").unwrap().value, "0x004d");
        assert_eq!(s.get("dwClockFrequency").unwrap().value, "48.000000MHz");
        assert_eq!(s.get("bInCollection").unwrap().value, "1");
        assert_eq!(s.get("baInterfaceNr[0]").unwrap().value, "1");
        assert!(!s.has_warnings());
    }

    #[test]
    fn frame_uncompressed_discrete() {
        let mut node = Node::new("x");
        let mut desc = vec![34, 0x24, 0x05, 1, 0x00];
        desc.extend_from_slice(&640u16.to_le_bytes());
        desc.extend_from_slice(&480u16.to_le_bytes());
        desc.extend_from_slice(&[0; 8]); // bit rates
        desc.extend_from_slice(&614_400u32.to_le_bytes());
        desc.extend_from_slice(&333_333u32.to_le_bytes());
        desc.push(2);
        desc.extend_from_slice(&333_333u32.to_le_bytes());
        desc.extend_from_slice(&666_666u32.to_le_bytes());
        assert_eq!(desc.len(), 34);
        dump_videostreaming_interface(&padded(&desc), &mut node);

        let s = node.get("VideoStreaming Interface").unwrap();
        assert_eq!(s.note, "FRAME_UNCOMPRESSED");
        let caps = s.get("bmCapabilities").unwrap();
        assert_eq!(caps.children[0].name, "Still image unsupported");
        assert_eq!(s.get("wWidth").unwrap().value, "640");
        assert_eq!(s.get("wHeight").unwrap().value, "480");
        assert_eq!(s.get("bFrameIntervalType").unwrap().value, "2");
        assert_eq!(s.get("dwFrameInterval[0]").unwrap().value, "333333");
        assert_eq!(s.get("dwFrameInterval[1]").unwrap().value, "666666");
        assert!(!s.has_warnings());
    }

    #[test]
    fn short_header() {
        let mut node = Node::new("x");
        dump_videocontrol_interface(None, &padded(&[11, 0x24, 0x01]), 0, &mut node);
        assert!(node.get("VideoControl Interface").unwrap().has_warnings());
    }
}
