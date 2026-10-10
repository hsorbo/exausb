// SPDX-License-Identifier: GPL-2.0-or-later
//! Communications (CDC), smart card (CCID), printer, DFU and wireless class
//! descriptor dumping.

use super::{get_guid, junk, too_short, unrecognized, USB_DT_CS_DEVICE};
use crate::model::{bcd, hex2, hex4, hex8, hex_bytes, join, Node};
use crate::usb::{le16, le32, Handle, InterfaceDescriptor};
use crate::usbmisc::get_dev_string;

/// A little-endian BCD version at the start of `buf`.
fn bcd_at(buf: &[u8]) -> String {
    bcd(le16(buf) as u16)
}

/// Add a text line under `node` for each flag in `flags` set in `value`.
fn flag_texts(node: &mut Node, value: u32, flags: &[(u32, &str)]) {
    for &(bit, name) in flags {
        if value & bit != 0 {
            node.text(name);
        }
    }
}

/// The names of the flags in `flags` set in `value`, joined for a note.
fn flag_note(value: u32, flags: &[(u32, &str)]) -> String {
    let names: Vec<&str> = flags
        .iter()
        .filter(|(bit, _)| value & bit != 0)
        .map(|&(_, name)| name)
        .collect();
    join(&names)
}

pub(super) fn dump_wire_adapter(buf: &[u8], node: &mut Node) {
    let s = node.section("Wire Adapter");
    if buf[0] < 14 {
        too_short(s);
        return;
    }
    s.field("bLength", buf[0]);
    s.field("bDescriptorType", buf[1]);
    s.field("bcdWAVersion", bcd_at(&buf[2..]));
    s.field("bNumPorts", buf[4]);
    s.field("bmAttributes", buf[5]);
    s.field("wNumRPipes", le16(&buf[6..]));
    s.field("wRPipeMaxBlock", le16(&buf[8..]));
    s.field("bRPipeBlockSize", buf[10]);
    s.field("bPwrOn2PwrGood", buf[11]);
    s.field("bNumMMCIEs", buf[12]);
    s.field("DeviceRemovable", buf[13]);
}

pub(super) fn dump_rc_interface(buf: &[u8], node: &mut Node) {
    let s = node.section("Radio Control Interface");
    if buf[0] < 4 {
        too_short(s);
        return;
    }
    s.field("bLength", buf[0]);
    s.field("bDescriptorType", buf[1]);
    s.field("bcdRCIVersion", bcd_at(&buf[2..]));
}

pub(super) fn dump_dfu_interface(buf: &[u8], node: &mut Node) {
    let s = node.section("Device Firmware Upgrade");
    if buf[1] != USB_DT_CS_DEVICE {
        s.warn("Invalid descriptor type");
    }
    if buf[0] < 7 {
        too_short(s);
        return;
    }
    s.field("bLength", buf[0]);
    s.field("bDescriptorType", buf[1]);
    let attr = s.field("bmAttributes", buf[2]);
    if buf[2] & 0xf0 != 0 {
        attr.warn("Unknown attributes");
    }
    attr.text(if buf[2] & 0x08 != 0 {
        "Will Detach"
    } else {
        "Will Not Detach"
    });
    attr.text(if buf[2] & 0x04 != 0 {
        "Manifestation Tolerant"
    } else {
        "Manifestation Intolerant"
    });
    attr.text(if buf[2] & 0x02 != 0 {
        "Upload Supported"
    } else {
        "Upload Unsupported"
    });
    attr.text(if buf[2] & 0x01 != 0 {
        "Download Supported"
    } else {
        "Download Unsupported"
    });
    s.field("wDetachTimeout", format!("{}ms", le16(&buf[3..])));
    s.field("wTransferSize", format!("{} bytes", le16(&buf[5..])));

    // DFU 1.0 defines no version code, DFU 1.1 does.
    if buf[0] >= 9 {
        s.field("bcdDFUVersion", bcd_at(&buf[7..]));
    }
}

pub(super) fn dump_ccid_device(buf: &[u8], node: &mut Node) {
    let s = node.section("ChipCard Interface");
    if buf[0] < 54 {
        too_short(s);
        return;
    }
    s.field("bLength", buf[0]);
    s.field("bDescriptorType", buf[1]);
    let version = s.field("bcdCCID", bcd_at(&buf[2..]));
    if buf[3] != 1 || (buf[2] != 0 && buf[2] != 0x10) {
        version.warn("Only accurate for version 1.0/1.1");
    }

    s.field("nMaxSlotIndex", buf[4]);
    s.field("bVoltageSupport", buf[5]).note(flag_note(
        buf[5].into(),
        &[(1, "5.0V"), (2, "3.0V"), (4, "1.8V")],
    ));

    let us = le32(&buf[6..]);
    let protocols = s
        .field("dwProtocols", us)
        .note(flag_note(us, &[(1, "T=0"), (2, "T=1")]));
    if us & !3 != 0 {
        protocols.warn("Invalid values detected");
    }

    s.field("dwDefaultClock", le32(&buf[10..]));
    s.field("dwMaximumClock", le32(&buf[14..]));
    s.field("bNumClockSupported", buf[18]);
    s.field("dwDataRate", format!("{} bps", le32(&buf[19..])));
    s.field("dwMaxDataRate", format!("{} bps", le32(&buf[23..])));
    s.field("bNumDataRatesSupported", buf[27]);
    s.field("dwMaxIFSD", le32(&buf[28..]));

    let us = le32(&buf[32..]);
    s.field("dwSynchProtocols", hex8(us))
        .note(flag_note(us, &[(1, "2-wire"), (2, "3-wire"), (4, "I2C")]));

    let us = le32(&buf[36..]);
    s.field("dwMechanical", hex8(us)).note(flag_note(
        us,
        &[(1, "accept"), (2, "eject"), (4, "capture"), (8, "lock")],
    ));

    let us = le32(&buf[40..]);
    let features = s.field("dwFeatures", hex8(us));
    flag_texts(
        features,
        us,
        &[
            (0x0002, "Auto configuration based on ATR"),
            (0x0004, "Auto activation on insert"),
            (0x0008, "Auto voltage selection"),
            (0x0010, "Auto clock change"),
            (0x0020, "Auto baud rate change"),
        ],
    );
    match us & (0x0040 | 0x0080) {
        0x0040 => {
            features.text("Auto parameter negotiation made by CCID");
        }
        0x0080 => {
            features.text("Auto PPS made by CCID");
        }
        0 => {}
        _ => {
            features.warn("Conflicting negotiation features");
        }
    }
    flag_texts(
        features,
        us,
        &[
            (0x0100, "CCID can set ICC in clock stop mode"),
            (0x0200, "NAD value other than 0x00 accepted (T=1)"),
            (0x0400, "Auto IFSD exchange (T=1)"),
        ],
    );
    match us & 0x0007_0000 {
        0 => {
            features.text("Character level exchange");
        }
        0x0001_0000 => {
            features.text("TPDU level exchange");
        }
        0x0002_0000 => {
            features.text("Short APDU level exchange");
        }
        0x0004_0000 => {
            features.text("Short and extended APDU level exchange");
        }
        _ => {
            features.warn("Conflicting exchange levels");
        }
    }
    if us & 0x0010_0000 != 0 {
        features.text("USB wakeup on ICC insertion and removal");
    }

    s.field("dwMaxCCIDMessageLength", le32(&buf[44..]));

    let echo = |b: u8| if b == 0xff { "echo" } else { "" };
    s.field("bClassGetResponse", hex2(buf[48]))
        .note(echo(buf[48]));
    s.field("bClassEnvelope", hex2(buf[49])).note(echo(buf[49]));

    if buf[50] == 0 && buf[51] == 0 {
        s.field("wLcdLayout", "none");
    } else {
        s.field("wLcdLayout", format!("{} cols {} lines", buf[50], buf[51]));
    }

    s.field("bPINSupport", buf[52]).note(flag_note(
        buf[52].into(),
        &[(1, "verification"), (2, "modification")],
    ));
    s.field("bMaxCCIDBusySlots", buf[53]);

    junk(buf, 54, s);
}

pub(super) fn dump_printer_device(
    dev: Option<&Handle>,
    interface: &InterfaceDescriptor,
    buf: &[u8],
    node: &mut Node,
) {
    if interface.b_interface_protocol != 0x04 {
        // Only IPP-over-USB.
        return;
    }

    let s = node.section("IPP Printer");
    if buf[0] < 4 {
        too_short(s);
        return;
    }
    s.field("bLength", buf[0]);
    s.field("bDescriptorType", buf[1]);
    s.field("bcdReleaseNumber", buf[2]);
    s.field("bcdNumDescriptors", buf[3]);

    let blen = buf[0] as usize;
    let mut n = 4;
    for _ in 0..buf[3] {
        if n + 2 > blen || n + 2 + buf[n + 1] as usize > blen {
            too_short(s);
            break;
        }
        match buf[n] {
            0x00 => {
                // Basic capabilities
                if n + 6 > blen {
                    too_short(s);
                    return;
                }
                let caps = le16(&buf[n + 2..]);
                let auth = match caps & 0x60 {
                    0x00 => "No-Auth",
                    0x20 => "Username-Auth",
                    0x40 => "Reserved-Auth",
                    _ => "Negotiable-Auth",
                };
                let flags = flag_note(
                    caps,
                    &[
                        (0x01, "Print"),
                        (0x02, "Scan"),
                        (0x04, "Fax"),
                        (0x08, "Other"),
                        (0x10, "HTTP-over-USB"),
                    ],
                );

                let c = s.section("Basic Capabilities");
                c.field("iIPPVersionsSupported", buf[n + 4]);
                c.field("iIPPPrinterUUID", buf[n + 5])
                    .note(get_dev_string(dev, buf[n + 5]));
                c.field("wBasicCapabilities", hex4(caps))
                    .note(join(&[&flags, auth]));
            }
            _ => {
                // Vendor Specific, Ignore for now.
                s.field("UnknownCapabilities", buf[n])
                    .note(format!("{} bytes", buf[n + 1]));
            }
        }
        n += 2 + buf[n + 1] as usize;
    }
}

/// Add a CDC functional descriptor section, or a warning if its length is
/// wrong for its subtype.  Returns the section to fill in if it's valid.
fn cdc_section<'a>(
    node: &'a mut Node,
    title: &str,
    valid: bool,
    buf: &[u8],
) -> Option<&'a mut Node> {
    let s = node.section(title);
    if valid {
        Some(s)
    } else {
        s.warn(format!(
            "Invalid descriptor: {}",
            hex_bytes(&buf[..buf[0] as usize])
        ));
        None
    }
}

pub(super) fn dump_comm_descriptor(dev: Option<&Handle>, buf: &[u8], node: &mut Node) {
    if buf[0] < 3 {
        too_short(node.section("CDC"));
        return;
    }
    let len = buf[0] as usize;
    match buf[2] {
        0x00 => {
            if let Some(s) = cdc_section(node, "CDC Header", len == 5, buf) {
                s.field("bcdCDC", bcd_at(&buf[3..]));
            }
        }
        0x01 => {
            // call management functional desc
            if let Some(s) = cdc_section(node, "CDC Call Management", len == 5, buf) {
                let caps = s.field("bmCapabilities", hex2(buf[3]));
                flag_texts(
                    caps,
                    buf[3].into(),
                    &[(0x01, "Call management"), (0x02, "Use DataInterface")],
                );
                s.field("bDataInterface", buf[4]);
            }
        }
        0x02 => {
            // acm functional desc
            if let Some(s) = cdc_section(node, "CDC ACM", len == 4, buf) {
                let caps = s.field("bmCapabilities", hex2(buf[3]));
                flag_texts(
                    caps,
                    buf[3].into(),
                    &[
                        (0x08, "Connection notifications"),
                        (0x04, "Sends break"),
                        (0x02, "Line coding and serial state"),
                        (0x01, "Get/set/clear comm features"),
                    ],
                );
            }
        }
        0x06 => {
            // union desc
            if let Some(s) = cdc_section(node, "CDC Union", len >= 5, buf) {
                s.field("bMasterInterface", buf[3]);
                for (i, b) in buf[4..len].iter().enumerate() {
                    s.field(format!("bSlaveInterface[{i}]"), b);
                }
            }
        }
        0x07 => {
            // country selection functional desc
            let valid = len >= 6 && len & 1 == 0;
            if let Some(s) = cdc_section(node, "CDC Country Selection", valid, buf) {
                s.field("iCountryCodeRelDate", buf[3])
                    .note(get_dev_string(dev, buf[3]));
                for (i, pair) in buf[4..len].chunks(2).enumerate() {
                    s.field(
                        format!("wCountryCode[{i}]"),
                        format!("0x{:02x}{:02x}", pair[0], pair[1]),
                    );
                }
            }
        }
        0x08 => {
            // telephone operational modes
            if let Some(s) = cdc_section(node, "CDC Telephone Operations", len == 4, buf) {
                let caps = s.field("bmCapabilities", hex2(buf[3]));
                flag_texts(
                    caps,
                    buf[3].into(),
                    &[
                        (0x04, "Computer centric mode"),
                        (0x02, "Standalone mode"),
                        (0x01, "Simple mode"),
                    ],
                );
            }
        }
        0x0a => {
            // network channel terminal
            if let Some(s) = cdc_section(node, "CDC Network Channel Terminal", len == 7, buf) {
                s.field("bEntityId", buf[3]);
                s.field("iName", buf[4]).note(get_dev_string(dev, buf[4]));
                s.field("bChannelIndex", buf[5]);
                s.field("bPhysicalInterface", buf[6]);
            }
        }
        0x0f => {
            // ethernet functional desc
            if let Some(s) = cdc_section(node, "CDC Ethernet", len == 13, buf) {
                s.field("iMacAddress", buf[3])
                    .note(get_dev_string(dev, buf[3]));
                // TODO: translate the 28 bits of bmEthernetStatistics (USB
                // CDC ECM Subclass, version 1.2, table 4) into something
                // "real".
                s.field("bmEthernetStatistics", hex8(le32(&buf[4..])));
                s.field("wMaxSegmentSize", le16(&buf[8..]));
                s.field("wNumberMCFilters", hex4(le16(&buf[10..])));
                s.field("bNumberPowerFilters", buf[12]);
            }
        }
        0x11 => {
            // WHCM functional desc
            if let Some(s) = cdc_section(node, "CDC WHCM", len == 5, buf) {
                s.field("bcdVersion", bcd_at(&buf[3..]));
            }
        }
        0x12 => {
            // MDLM functional desc
            if let Some(s) = cdc_section(node, "CDC MDLM", len == 21, buf) {
                s.field("bcdCDC", bcd_at(&buf[3..]));
                s.field("bGUID", get_guid(&buf[5..]));
            }
        }
        0x13 => {
            // MDLM detail desc
            if let Some(s) = cdc_section(node, "CDC MDLM Detail", len >= 5, buf) {
                s.field("bGuidDescriptorType", hex2(buf[3]));
                s.field("bDetailData", hex_bytes(&buf[4..len]));
            }
        }
        0x14 => {
            // device management functional desc
            if let Some(s) = cdc_section(node, "CDC Device Management", len == 7, buf) {
                s.field("bcdVersion", bcd_at(&buf[3..]));
                s.field("wMaxCommand", le16(&buf[5..]));
            }
        }
        0x15 => {
            // OBEX functional desc
            if let Some(s) = cdc_section(node, "CDC OBEX", len == 5, buf) {
                s.field("bcdVersion", bcd_at(&buf[3..]));
            }
        }
        0x16 => {
            // command set functional desc
            if let Some(s) = cdc_section(node, "CDC Command Set", len == 22, buf) {
                s.field("bcdVersion", bcd_at(&buf[3..]));
                s.field("iCommandSet", buf[5])
                    .note(get_dev_string(dev, buf[5]));
                s.field("bGUID", get_guid(&buf[6..]));
            }
        }
        0x1a => {
            // NCM functional desc
            if let Some(s) = cdc_section(node, "CDC NCM", len == 6, buf) {
                s.field("bcdNcmVersion", bcd_at(&buf[3..]));
                let caps = s.field("bmNetworkCapabilities", hex2(buf[5]));
                flag_texts(
                    caps,
                    buf[5].into(),
                    &[
                        (1 << 5, "8-byte NTB input size"),
                        (1 << 4, "CRC mode"),
                        (1 << 3, "Max datagram size"),
                        (1 << 2, "Encapsulated commands"),
                        (1 << 1, "Net address"),
                        (1 << 0, "Packet filter"),
                    ],
                );
            }
        }
        0x1b => {
            // MBIM functional desc
            if let Some(s) = cdc_section(node, "CDC MBIM", len == 12, buf) {
                s.field("bcdMBIMVersion", bcd_at(&buf[3..]));
                s.field("wMaxControlMessage", le16(&buf[5..]));
                s.field("bNumberFilters", buf[7]);
                s.field("bMaxFilterSize", buf[8]);
                s.field("wMaxSegmentSize", le16(&buf[9..]));
                let caps = s.field("bmNetworkCapabilities", hex2(buf[11]));
                flag_texts(
                    caps,
                    buf[11].into(),
                    &[(0x20, "8-byte NTB input size"), (0x08, "Max datagram size")],
                );
            }
        }
        0x1c => {
            // MBIM extended functional desc
            if let Some(s) = cdc_section(node, "CDC MBIM Extended", len == 8, buf) {
                s.field("bcdMBIMExtendedVersion", bcd_at(&buf[3..]));
                s.field("bMaxOutstandingCommandMessages", buf[5]);
                s.field("wMTU", le16(&buf[6..]));
            }
        }
        _ => {
            // There are about a dozen more descriptor types, if anyone has
            // a device with them in it, we'll add them here in the future
            // if really needed.
            unrecognized(buf, node.section("CDC"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Kind;
    use crate::usb::padded;

    fn texts(node: &Node) -> Vec<&str> {
        node.children
            .iter()
            .filter(|c| c.kind == Kind::Text)
            .map(|c| c.name.as_str())
            .collect()
    }

    #[test]
    fn cdc_union() {
        let mut node = Node::new("x");
        dump_comm_descriptor(None, &padded(&[6, 0x24, 0x06, 0, 1, 2]), &mut node);
        let union = node.get("CDC Union").unwrap();
        assert_eq!(union.get("bMasterInterface").unwrap().value, "0");
        assert_eq!(union.get("bSlaveInterface[0]").unwrap().value, "1");
        assert_eq!(union.get("bSlaveInterface[1]").unwrap().value, "2");
        assert!(!node.has_warnings());
    }

    #[test]
    fn cdc_acm() {
        let mut node = Node::new("x");
        dump_comm_descriptor(None, &padded(&[4, 0x24, 0x02, 0x06]), &mut node);
        let caps = node.get("CDC ACM").unwrap().get("bmCapabilities").unwrap();
        assert_eq!(caps.value, "0x06");
        assert_eq!(texts(caps), ["Sends break", "Line coding and serial state"]);
    }

    #[test]
    fn cdc_invalid_and_unknown() {
        let mut node = Node::new("x");
        dump_comm_descriptor(None, &padded(&[4, 0x24, 0x00, 0x10]), &mut node);
        dump_comm_descriptor(None, &padded(&[3, 0x24, 0x7f]), &mut node);
        let header = node.get("CDC Header").unwrap();
        assert_eq!(header.children.len(), 1);
        assert_eq!(header.children[0].kind, Kind::Warning);
        assert_eq!(header.children[0].name, "Invalid descriptor: 04 24 00 10");
        assert!(node.get("CDC").unwrap().has_warnings());
    }

    #[test]
    fn cdc_header_version() {
        let mut node = Node::new("x");
        dump_comm_descriptor(None, &padded(&[5, 0x24, 0x00, 0x10, 0x01]), &mut node);
        let header = node.get("CDC Header").unwrap();
        assert_eq!(header.get("bcdCDC").unwrap().value, "1.10");
    }

    #[test]
    fn dfu() {
        let mut node = Node::new("x");
        dump_dfu_interface(
            &padded(&[
                9,
                USB_DT_CS_DEVICE,
                0x0b,
                0xff,
                0x00,
                0x00,
                0x04,
                0x10,
                0x01,
            ]),
            &mut node,
        );
        let dfu = node.get("Device Firmware Upgrade").unwrap();
        assert!(!dfu.has_warnings());
        assert_eq!(
            texts(dfu.get("bmAttributes").unwrap()),
            [
                "Will Detach",
                "Manifestation Intolerant",
                "Upload Supported",
                "Download Supported"
            ]
        );
        assert_eq!(dfu.get("wDetachTimeout").unwrap().value, "255ms");
        assert_eq!(dfu.get("wTransferSize").unwrap().value, "1024 bytes");
        assert_eq!(dfu.get("bcdDFUVersion").unwrap().value, "1.10");
    }

    #[test]
    fn ccid_too_short() {
        let mut node = Node::new("x");
        dump_ccid_device(&padded(&[10, 0x21]), &mut node);
        assert!(node.get("ChipCard Interface").unwrap().has_warnings());
    }
}
