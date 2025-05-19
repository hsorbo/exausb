// SPDX-License-Identifier: GPL-2.0-or-later
//! Video class descriptor dumping.

use super::{
    check_descriptor_length, dump_bytes, dump_junk, get_guid, USB_DT_CS_ENDPOINT,
    USB_DT_CS_INTERFACE,
};
use crate::names::get_videoterminal_string;
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

fn too_short() {
    println!("      Warning: Descriptor too short");
}

/// Print the names of the set bits among the first `count` bits.
fn print_bits(value: u32, names: &[&str], count: usize) {
    for (i, name) in names.iter().enumerate().take(count) {
        if (value >> i) & 1 != 0 {
            println!("          {name}");
        }
    }
}

fn ep_dir(addr: u8) -> &'static str {
    if addr & 0x80 != 0 {
        "IN"
    } else {
        "OUT"
    }
}

pub(super) fn dump_videocontrol_interface(dev: Option<&Handle>, buf: &[u8], protocol: u8) {
    if buf[1] != USB_DT_CS_INTERFACE {
        println!("      Warning: Invalid descriptor");
    }
    if buf[0] < 3 {
        too_short();
        return;
    }
    println!("      VideoControl Interface Descriptor:");
    println!("        bLength             {:5}", buf[0]);
    println!("        bDescriptorType     {:5}", buf[1]);
    print!("        bDescriptorSubtype  {:5} ", buf[2]);

    let len = buf[0] as usize;
    match buf[2] {
        0x01 => {
            // HEADER
            println!("(HEADER)");
            if len < 12 {
                too_short();
                return;
            }
            let n = buf[11] as usize;
            if len < 12 + n {
                too_short();
                return;
            }
            let freq = le32(&buf[7..]);
            println!("        bcdUVC              {:2x}.{:02x}", buf[4], buf[3]);
            println!("        wTotalLength       0x{:04x}", le16(&buf[5..]));
            println!(
                "        dwClockFrequency    {:5}.{:06}MHz",
                freq / 1_000_000,
                freq % 1_000_000
            );
            println!("        bInCollection       {:5}", n);
            for i in 0..n {
                println!("        baInterfaceNr({:2})   {:5}", i, buf[12 + i]);
            }
            dump_junk(buf, "        ", 12 + n);
        }
        0x02 => {
            // INPUT_TERMINAL
            println!("(INPUT_TERMINAL)");
            if len < 8 {
                too_short();
                return;
            }
            let term = get_dev_string(dev, buf[7]);
            let termt = le16(&buf[4..]);
            let mut n = if termt == 0x0201 { 7 } else { 0 };
            let termts = get_videoterminal_string(termt as u16);
            if len < 8 + n {
                too_short();
                return;
            }
            println!("        bTerminalID         {:5}", buf[3]);
            println!("        wTerminalType      0x{:04x} {}", termt, termts);
            println!("        bAssocTerminal      {:5}", buf[6]);
            println!("        iTerminal           {:5} {}", buf[7], term);
            if termt == 0x0201 {
                n += buf[14] as usize;
                if len < 8 + n {
                    too_short();
                    return;
                }
                println!("        wObjectiveFocalLengthMin  {:5}", le16(&buf[8..]));
                println!("        wObjectiveFocalLengthMax  {:5}", le16(&buf[10..]));
                println!("        wOcularFocalLength        {:5}", le16(&buf[12..]));
                println!("        bControlSize              {:5}", buf[14]);
                let mut ctrls: u32 = 0;
                for i in 0..(buf[14] as usize).min(3) {
                    ctrls = (ctrls << 8) | buf[8 + n - i - 1] as u32;
                }
                println!("        bmControls           0x{:08x}", ctrls);
                let count = if protocol == USB_VIDEO_PROTOCOL_15 {
                    22
                } else {
                    19
                };
                print_bits(ctrls, &CAMCTRLNAMES, count);
            }
            dump_junk(buf, "        ", 8 + n);
        }
        0x03 => {
            // OUTPUT_TERMINAL
            println!("(OUTPUT_TERMINAL)");
            if len < 9 {
                too_short();
                return;
            }
            let term = get_dev_string(dev, buf[8]);
            let termt = le16(&buf[4..]);
            let termts = get_videoterminal_string(termt as u16);
            println!("        bTerminalID         {:5}", buf[3]);
            println!("        wTerminalType      0x{:04x} {}", termt, termts);
            println!("        bAssocTerminal      {:5}", buf[6]);
            println!("        bSourceID           {:5}", buf[7]);
            println!("        iTerminal           {:5} {}", buf[8], term);
            dump_junk(buf, "        ", 9);
        }
        0x04 => {
            // SELECTOR_UNIT
            println!("(SELECTOR_UNIT)");
            if len < 5 {
                too_short();
                return;
            }
            let p = buf[4] as usize;
            if len < 6 + p {
                too_short();
                return;
            }
            let term = get_dev_string(dev, buf[5 + p]);

            println!("        bUnitID             {:5}", buf[3]);
            println!("        bNrInPins           {:5}", p);
            for i in 0..p {
                println!("        baSource({:2})        {:5}", i, buf[5 + i]);
            }
            println!("        iSelector           {:5} {}", buf[5 + p], term);
            dump_junk(buf, "        ", 6 + p);
        }
        0x05 => {
            // PROCESSING_UNIT
            println!("(PROCESSING_UNIT)");
            if len < 8 {
                too_short();
                return;
            }
            let n = buf[7] as usize;
            // Upstream means to require 10 + n bytes for UVC >= 1.1, but
            // the version it checks is only set while dumping a HEADER, so
            // it always requires 9 + n.
            let p = 9;
            if !check_descriptor_length(buf[0] as u32, (p + n) as u32) {
                return;
            }
            let term = get_dev_string(dev, buf[8 + n]);
            println!("        bUnitID             {:5}", buf[3]);
            println!("        bSourceID           {:5}", buf[4]);
            println!("        wMaxMultiplier      {:5}", le16(&buf[5..]));
            println!("        bControlSize        {:5}", n);
            let mut ctrls: u32 = 0;
            for i in 0..n.min(3) {
                ctrls = (ctrls << 8) | buf[8 + n - i - 1] as u32;
            }
            println!("        bmControls     0x{:08x}", ctrls);
            let count = if protocol == USB_VIDEO_PROTOCOL_15 {
                19
            } else {
                18
            };
            print_bits(ctrls, &CTRLNAMES, count);
            let stds = buf[9 + n] as u32;
            println!("        iProcessing         {:5} {}", buf[8 + n], term);
            println!("        bmVideoStandards     0x{:02x}", stds);
            print_bits(stds, &STDNAMES, 6);
        }
        0x06 => {
            // EXTENSION_UNIT
            println!("(EXTENSION_UNIT)");
            if len < 22 {
                too_short();
                return;
            }
            let p = buf[21] as usize;
            if len < 23 + p {
                too_short();
                return;
            }
            let n = buf[22 + p] as usize;
            if len < 24 + p + n {
                too_short();
                return;
            }
            let term = get_dev_string(dev, buf[23 + p + n]);
            println!("        bUnitID             {:5}", buf[3]);
            println!("        guidExtensionCode         {}", get_guid(&buf[4..]));
            println!("        bNumControls        {:5}", buf[20]);
            println!("        bNrInPins           {:5}", buf[21]);
            for i in 0..p {
                println!("        baSourceID({:2})      {:5}", i, buf[22 + i]);
            }
            println!("        bControlSize        {:5}", buf[22 + p]);
            for i in 0..n {
                println!(
                    "        bmControls({:2})       0x{:02x}",
                    i,
                    buf[23 + p + i]
                );
            }
            println!("        iExtension          {:5} {}", buf[23 + p + n], term);
            dump_junk(buf, "        ", 24 + p + n);
        }
        0x07 => {
            // ENCODING UNIT
            println!("(ENCODING UNIT)");
            if len < 13 {
                too_short();
                return;
            }
            let term = get_dev_string(dev, buf[5]);
            println!("        bUnitID             {:5}", buf[3]);
            println!("        bSourceID           {:5}", buf[4]);
            println!("        iEncoding           {:5} {}", buf[5], term);
            println!("        bControlSize        {:5}", buf[6]);
            let mut ctrls: u32 = 0;
            for i in 0..3 {
                ctrls = (ctrls << 8) | buf[9 - i] as u32;
            }
            println!("        bmControls              0x{:08x}", ctrls);
            print_bits(ctrls, &ENCTRLNAMES, 20);
            // Note: like upstream, `ctrls` is not reset here.
            for i in 0..3 {
                ctrls = (ctrls << 8) | buf[12 - i] as u32;
            }
            println!("        bmControlsRuntime       0x{:08x}", ctrls);
            print_bits(ctrls, &ENCTRLNAMES, 20);
        }
        _ => {
            print!("(unknown)\n        Invalid desc subtype:");
            if buf[0] > 3 {
                dump_bytes(&buf[3..], len - 3);
            } else {
                println!();
            }
        }
    }
}

pub(super) fn dump_videocontrol_interrupt_endpoint(buf: &[u8]) {
    if buf[1] != USB_DT_CS_ENDPOINT {
        println!("      Warning: Invalid descriptor");
    }
    if buf[0] < 5 {
        too_short();
        return;
    }
    let w_max_transfer_size = le16(&buf[3..]);
    println!("        VideoControl Endpoint Descriptor:");
    println!("          bLength             {:5}", buf[0]);
    println!("          bDescriptorType     {:5}", buf[1]);
    println!(
        "          bDescriptorSubtype  {:5} ({})",
        buf[2],
        if buf[2] == 3 {
            "EP_INTERRUPT"
        } else {
            "Invalid"
        }
    );
    println!("          wMaxTransferSize    {:5}", w_max_transfer_size);
    dump_junk(buf, "          ", 5);
}

fn print_interlace_flags(flags: u8, one_field_bit_means_one: bool) {
    println!(
        "          Interlaced stream or variable: {}",
        if flags & (1 << 0) != 0 { "Yes" } else { "No" }
    );
    // Upstream disagrees with itself on the meaning of bit 1 between the
    // uncompressed/frame-based and MJPEG formats; keep that behaviour.
    let fields = match (flags & (1 << 1) != 0, one_field_bit_means_one) {
        (true, true) | (false, false) => 1,
        _ => 2,
    };
    println!("          Fields per frame: {fields} fields");
    println!(
        "          Field 1 first: {}",
        if flags & (1 << 2) != 0 { "Yes" } else { "No" }
    );
    print!("          Field pattern: ");
    match (flags >> 4) & 0x03 {
        0 => println!("Field 1 only"),
        1 => println!("Field 2 only"),
        2 => println!("Regular pattern of fields 1 and 2"),
        _ => println!("Random pattern of fields 1 and 2"),
    }
}

pub(super) fn dump_videostreaming_interface(buf: &[u8]) {
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

    if buf[1] != USB_DT_CS_INTERFACE {
        println!("      Warning: Invalid descriptor");
    }
    if buf[0] < 3 {
        too_short();
        return;
    }
    println!("      VideoStreaming Interface Descriptor:");
    println!("        bLength                         {:5}", buf[0]);
    println!("        bDescriptorType                 {:5}", buf[1]);
    print!("        bDescriptorSubtype              {:5} ", buf[2]);

    let blen = buf[0] as usize;
    match buf[2] {
        0x01 => {
            // INPUT_HEADER
            println!("(INPUT_HEADER)");
            if blen < 13 {
                too_short();
                return;
            }
            let p = buf[3] as usize;
            let n = buf[12] as usize;
            if blen < 13 + p * n {
                too_short();
                return;
            }
            println!("        bNumFormats                     {:5}", p);
            println!(
                "        wTotalLength                   0x{:04x}",
                le16(&buf[4..])
            );
            println!(
                "        bEndpointAddress                 0x{:02x}  EP {} {}",
                buf[6],
                buf[6] & 0x0f,
                ep_dir(buf[6])
            );
            println!("        bmInfo                          {:5}", buf[7]);
            println!("        bTerminalLink                   {:5}", buf[8]);
            println!("        bStillCaptureMethod             {:5}", buf[9]);
            println!("        bTriggerSupport                 {:5}", buf[10]);
            println!("        bTriggerUsage                   {:5}", buf[11]);
            println!("        bControlSize                    {:5}", n);
            for i in 0..p {
                println!(
                    "        bmaControls({:2})                 {:5}",
                    i,
                    buf[13 + i * n]
                );
            }
            dump_junk(buf, "        ", 13 + p * n);
        }
        0x02 => {
            // OUTPUT_HEADER
            println!("(OUTPUT_HEADER)");
            if blen < 9 {
                too_short();
                return;
            }
            let p = buf[3] as usize;
            let n = buf[8] as usize;
            if blen < 9 + p * n {
                too_short();
                return;
            }
            println!("        bNumFormats                 {:5}", p);
            println!(
                "        wTotalLength               0x{:04x}",
                le16(&buf[4..])
            );
            println!(
                "        bEndpointAddress             0x{:02x}  EP {} {}",
                buf[6],
                buf[6] & 0x0f,
                ep_dir(buf[6])
            );
            println!("        bTerminalLink               {:5}", buf[7]);
            println!("        bControlSize                {:5}", n);
            for i in 0..p {
                println!(
                    "        bmaControls({:2})             {:5}",
                    i,
                    buf[9 + i * n]
                );
            }
            dump_junk(buf, "        ", 9 + p * n);
        }
        0x03 => {
            // STILL_IMAGE_FRAME
            println!("(STILL_IMAGE_FRAME)");
            if blen < 5 {
                too_short();
                return;
            }
            let n = buf[4] as usize;
            if blen < 6 + 4 * n {
                too_short();
                return;
            }
            let m = buf[5 + 4 * n] as usize;
            if blen < 6 + 4 * n + m {
                too_short();
                return;
            }
            println!(
                "        bEndpointAddress                 0x{:02x}  EP {} {}",
                buf[3],
                buf[3] & 0x0f,
                ep_dir(buf[3])
            );
            println!("        bNumImageSizePatterns             {:3}", n);
            for i in 0..n {
                println!(
                    "        wWidth({:2})                      {:5}",
                    i,
                    le16(&buf[5 + 4 * i..])
                );
                println!(
                    "        wHeight({:2})                     {:5}",
                    i,
                    le16(&buf[7 + 4 * i..])
                );
            }
            println!("        bNumCompressionPatterns           {:3}", m);
            for i in 0..m {
                println!(
                    "        bCompression({:2})                {:5}",
                    i,
                    buf[6 + 4 * n + i]
                );
            }
            dump_junk(buf, "        ", 6 + 4 * n + m);
        }
        0x04 | 0x10 => {
            // FORMAT_UNCOMPRESSED / FORMAT_FRAME_BASED
            let len = if buf[2] == 0x04 {
                println!("(FORMAT_UNCOMPRESSED)");
                27
            } else {
                println!("(FORMAT_FRAME_BASED)");
                28
            };
            if blen < len {
                too_short();
                return;
            }
            let flags = buf[25];
            println!("        bFormatIndex                    {:5}", buf[3]);
            println!("        bNumFrameDescriptors            {:5}", buf[4]);
            println!(
                "        guidFormat                            {}",
                get_guid(&buf[5..])
            );
            println!("        bBitsPerPixel                   {:5}", buf[21]);
            println!("        bDefaultFrameIndex              {:5}", buf[22]);
            println!("        bAspectRatioX                   {:5}", buf[23]);
            println!("        bAspectRatioY                   {:5}", buf[24]);
            println!("        bmInterlaceFlags                 0x{:02x}", flags);
            print_interlace_flags(flags, true);
            println!("        bCopyProtect                    {:5}", buf[26]);
            if buf[2] == 0x10 {
                println!("        bVariableSize                 {:5}", buf[27]);
            }
            dump_junk(buf, "        ", len);
        }
        0x05 | 0x07 | 0x11 => {
            // FRAME_UNCOMPRESSED / FRAME_MJPEG / FRAME_FRAME_BASED
            let n = match buf[2] {
                0x05 => {
                    println!("(FRAME_UNCOMPRESSED)");
                    25
                }
                0x07 => {
                    println!("(FRAME_MJPEG)");
                    25
                }
                _ => {
                    println!("(FRAME_FRAME_BASED)");
                    21
                }
            };
            if blen < n + 1 {
                too_short();
                return;
            }
            let len = if buf[n] != 0 {
                26 + buf[n] as usize * 4
            } else {
                38
            };
            if blen < len {
                too_short();
                return;
            }
            let flags = buf[4];
            println!("        bFrameIndex                     {:5}", buf[3]);
            println!("        bmCapabilities                   0x{:02x}", flags);
            println!(
                "          Still image {}supported",
                if flags & (1 << 0) != 0 { "" } else { "un" }
            );
            if flags & (1 << 1) != 0 {
                println!("          Fixed frame-rate");
            }
            println!(
                "        wWidth                          {:5}",
                le16(&buf[5..])
            );
            println!(
                "        wHeight                         {:5}",
                le16(&buf[7..])
            );
            println!("        dwMinBitRate                {:9}", le32(&buf[9..]));
            println!("        dwMaxBitRate                {:9}", le32(&buf[13..]));
            if buf[2] == 0x11 {
                println!("        dwDefaultFrameInterval      {:9}", le32(&buf[17..]));
                println!("        bFrameIntervalType              {:5}", buf[21]);
                println!("        dwBytesPerLine              {:9}", le32(&buf[22..]));
            } else {
                println!("        dwMaxVideoFrameBufferSize   {:9}", le32(&buf[17..]));
                println!("        dwDefaultFrameInterval      {:9}", le32(&buf[21..]));
                println!("        bFrameIntervalType              {:5}", buf[25]);
            }
            if buf[n] == 0 {
                println!("        dwMinFrameInterval          {:9}", le32(&buf[26..]));
                println!("        dwMaxFrameInterval          {:9}", le32(&buf[30..]));
                println!("        dwFrameIntervalStep         {:9}", le32(&buf[34..]));
            } else {
                for i in 0..buf[n] as usize {
                    println!(
                        "        dwFrameInterval({:2})         {:9}",
                        i,
                        le32(&buf[26 + 4 * i..])
                    );
                }
            }
            dump_junk(buf, "        ", len);
        }
        0x06 => {
            // FORMAT_MJPEG
            println!("(FORMAT_MJPEG)");
            if blen < 11 {
                too_short();
                return;
            }
            let flags = buf[5];
            println!("        bFormatIndex                    {:5}", buf[3]);
            println!("        bNumFrameDescriptors            {:5}", buf[4]);
            println!("        bFlags                          {:5}", flags);
            println!(
                "          Fixed-size samples: {}",
                if flags & (1 << 0) != 0 { "Yes" } else { "No" }
            );
            let flags = buf[9];
            println!("        bDefaultFrameIndex              {:5}", buf[6]);
            println!("        bAspectRatioX                   {:5}", buf[7]);
            println!("        bAspectRatioY                   {:5}", buf[8]);
            println!("        bmInterlaceFlags                 0x{:02x}", flags);
            print_interlace_flags(flags, false);
            println!("        bCopyProtect                    {:5}", buf[10]);
            dump_junk(buf, "        ", 11);
        }
        0x0a => {
            // FORMAT_MPEG2TS
            println!("(FORMAT_MPEG2TS)");
            let len = if blen < 23 { 7 } else { 23 };
            if blen < len {
                too_short();
                return;
            }
            println!("        bFormatIndex                    {:5}", buf[3]);
            println!("        bDataOffset                     {:5}", buf[4]);
            println!("        bPacketLength                   {:5}", buf[5]);
            println!("        bStrideLength                   {:5}", buf[6]);
            if len > 7 {
                println!(
                    "        guidStrideFormat                      {}",
                    get_guid(&buf[7..])
                );
            }
            dump_junk(buf, "        ", len);
        }
        0x0d => {
            // COLORFORMAT
            println!("(COLORFORMAT)");
            if blen < 6 {
                too_short();
                return;
            }
            let name =
                |table: &[&'static str], v: u8| table.get(v as usize).copied().unwrap_or("Unknown");
            println!(
                "        bColorPrimaries                 {:5} ({})",
                buf[3],
                name(&COLOR_PRIMS, buf[3])
            );
            println!(
                "        bTransferCharacteristics        {:5} ({})",
                buf[4],
                name(&TRANSFER_CHARS, buf[4])
            );
            println!(
                "        bMatrixCoefficients             {:5} ({})",
                buf[5],
                name(&MATRIX_COEFFS, buf[5])
            );
            dump_junk(buf, "        ", 6);
        }
        0x12 => {
            // FORMAT_STREAM_BASED
            println!("(FORMAT_STREAM_BASED)");
            if blen < 24 {
                too_short();
                return;
            }
            println!("        bFormatIndex                    {:5}", buf[3]);
            println!(
                "        guidFormat                            {}",
                get_guid(&buf[4..])
            );
            println!("        dwPacketLength                {:7}", buf[20]);
            dump_junk(buf, "        ", 24);
        }
        _ => {
            print!("        Invalid desc subtype:");
            if buf[0] > 3 {
                dump_bytes(&buf[3..], blen - 3);
            } else {
                println!();
            }
        }
    }
}
