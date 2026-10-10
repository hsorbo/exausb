// SPDX-License-Identifier: GPL-2.0-or-later
//! Binary Object Store (BOS) and device capability descriptor dumping.

use super::{get_guid, too_short, unrecognized, USB_DT_BOS};
use crate::desc_defs::DESC_USB3_DC_CONFIGURATION_SUMMARY;
use crate::desc_dump::{desc_dump, DESC_BUF_LEN_FROM_BUF};
use crate::model::{bcd, hex2, hex4, hex8, hex_bytes, Node};
use crate::usb::{self, control_in, le16, le32, Handle};
use crate::usbmisc::get_dev_string;

// Device Capability Type Codes (Wireless USB spec and USB 3.0 bus spec).
const USB_DC_WIRELESS_USB: u8 = 0x01;
const USB_DC_20_EXTENSION: u8 = 0x02;
const USB_DC_SUPERSPEED: u8 = 0x03;
const USB_DC_CONTAINER_ID: u8 = 0x04;
const USB_DC_PLATFORM: u8 = 0x05;
const USB_DC_SUPERSPEEDPLUS: u8 = 0x0a;
const USB_DC_BILLBOARD: u8 = 0x0d;
const USB_DC_BILLBOARD_ALT_MODE: u8 = 0x0f;
const USB_DC_CONFIGURATION_SUMMARY: u8 = 0x10;
const USB_DC_FWSTATUS_CAPABILITY: u8 = 0x11;

const BILLBOARD_MAX_NUM_ALT_MODE: u8 = 0x34;

// From the WebUSB specification: https://wicg.github.io/webusb/
const WEBUSB_GUID: &str = "{3408b638-09a9-47a0-8bfd-a0768815b665}";
const WEBUSB_GET_URL: u16 = 0x02;
const USB_DT_WEBUSB_URL: u8 = 0x03;

const VCONN_POWER: [&str; 8] = ["1W", "1.5W", "2W", "3W", "4W", "5W", "6W", "reserved"];

const ALT_MODE_STATE: [&str; 4] = [
    "Unspecified Error",
    "Alternate Mode configuration not attempted",
    "Alternate Mode configuration attempted but unsuccessful",
    "Alternate Mode configuration successful",
];

/// Add the fields common to all device capability descriptors.
fn cap_header(buf: &[u8], s: &mut Node) {
    s.field("bLength", buf[0]);
    s.field("bDescriptorType", buf[1]);
    s.field("bDevCapabilityType", buf[2]);
}

fn dump_usb2_device_capability_desc(buf: &[u8], lpm_required: bool, node: &mut Node) {
    const BESL_US: [u16; 16] = [
        125, 150, 200, 300, 400, 500, 1000, 2000, 3000, 4000, 5000, 6000, 7000, 8000, 9000, 10000,
    ];

    let s = node.section("USB 2.0 Extension");
    if buf[0] < 7 {
        too_short(s);
        return;
    }
    cap_header(buf, s);
    let wide = le32(&buf[3..]);
    let attr = s.field("bmAttributes", hex8(wide));
    if (lpm_required || wide & 0x04 != 0) && wide & 0x02 == 0 {
        attr.warn("Missing must-be-set LPM bit");
    } else if !lpm_required && wide & 0x02 == 0 {
        attr.text("Link Power Management (LPM) not supported");
    } else if wide & 0x04 == 0 {
        attr.text("HIRD Link Power Management (LPM) Supported");
    } else {
        attr.text("BESL Link Power Management (LPM) Supported");
        if wide & 0x08 != 0 {
            let besl = ((wide & 0xf00) >> 8) as usize;
            attr.text(format!("Baseline BESL value {}µs", BESL_US[besl]));
        }
        if wide & 0x10 != 0 {
            let besl = ((wide & 0xf000) >> 12) as usize;
            attr.text(format!("Deep BESL value {}µs", BESL_US[besl]));
        }
    }
}

fn dump_ss_device_capability_desc(buf: &[u8], node: &mut Node) {
    const SPEEDS: [&str; 4] = [
        "Low Speed (1Mbps)",
        "Full Speed (12Mbps)",
        "High Speed (480Mbps)",
        "SuperSpeed (5Gbps)",
    ];

    let s = node.section("SuperSpeed USB");
    if buf[0] < 10 {
        too_short(s);
        return;
    }
    cap_header(buf, s);
    let attr = s.field("bmAttributes", hex2(buf[3]));
    if buf[3] & 0x02 != 0 {
        attr.text("Latency Tolerance Messages (LTM) Supported");
    }
    let speeds = s.field("wSpeedsSupported", hex4(le16(&buf[4..])));
    for (bit, speed) in SPEEDS.iter().enumerate() {
        if buf[4] & (1 << bit) != 0 {
            speeds.text(format!("Device can operate at {speed}"));
        }
    }
    let lowest = SPEEDS
        .get(buf[6] as usize)
        .copied()
        .unwrap_or("Unknown speed");
    s.field("bFunctionalitySupport", buf[6])
        .note(format!("Lowest fully-functional speed: {lowest}"));
    s.field("bU1DevExitLat", format!("{}µs", buf[7]));
    s.field("bU2DevExitLat", format!("{}µs", le16(&buf[8..])));
}

fn dump_ssp_device_capability_desc(buf: &[u8], node: &mut Node) {
    const BITRATE_PREFIX: [&str; 4] = ["", "K", "M", "G"];

    let s = node.section("SuperSpeedPlus USB");
    if buf[0] < 12 {
        too_short(s);
        return;
    }
    let attr_count = (buf[4] & 0x1f) as usize + 1;
    if (buf[0] as usize) < 12 + attr_count * 4 {
        too_short(s);
        return;
    }
    cap_header(buf, s);

    let bm_attr = le32(&buf[4..]);
    let attr = s.field("bmAttributes", hex8(bm_attr));
    attr.text(format!("Sublink Speed Attribute count {attr_count}"));
    attr.text(format!(
        "Sublink Speed ID count {}",
        ((bm_attr >> 5) & 0xf) + 1
    ));

    let func = s.field("wFunctionalitySupport", hex4(le16(&buf[8..])));
    func.text(format!(
        "Min functional Speed Attribute ID {}",
        buf[8] & 0x0f
    ));
    func.text(format!("Min functional RX lanes {}", buf[9] & 0x0f));
    func.text(format!("Min functional TX lanes {}", (buf[9] >> 4) & 0x0f));

    for i in (0..attr_count).take_while(|i| 12 + i * 4 + 4 <= buf[0] as usize) {
        let ss_attr = le32(&buf[12 + i * 4..]);
        s.field(format!("bmSublinkSpeedAttr[{i}]"), hex8(ss_attr))
            .note(format!(
                "Speed Attribute ID {}: {}{}b/s {} {} SuperSpeed{}",
                ss_attr & 0x0f,
                ss_attr >> 16,
                BITRATE_PREFIX[((ss_attr >> 4) & 0x3) as usize],
                if ss_attr & 0x40 != 0 {
                    "Asymmetric"
                } else {
                    "Symmetric"
                },
                if ss_attr & 0x80 != 0 { "TX" } else { "RX" },
                if ss_attr & 0x4000 != 0 { "Plus" } else { "" }
            ));
    }
}

fn dump_container_id_device_capability_desc(buf: &[u8], node: &mut Node) {
    let s = node.section("Container ID");
    if buf[0] < 20 {
        too_short(s);
        return;
    }
    cap_header(buf, s);
    s.field("bReserved", buf[3]);
    s.field("ContainerID", get_guid(&buf[4..]));
}

/// Fetch a WebUSB landing page URL.  Failing to read it isn't worth a
/// warning (an empty string is returned), but a malformed one is.
fn get_webusb_url(fd: Option<&Handle>, vendor_req: u8, id: u8) -> Result<String, &'static str> {
    let Some(fd) = fd else {
        return Ok(String::new());
    };
    let mut url_buf = [0u8; 255];
    let ret = control_in(
        fd,
        usb::RECIPIENT_DEVICE | usb::REQUEST_TYPE_VENDOR,
        vendor_req,
        id as u16,
        WEBUSB_GET_URL,
        &mut url_buf,
    );
    match ret {
        Ok(n) if n > 0 => {
            if url_buf[0] <= 3 || url_buf[1] != USB_DT_WEBUSB_URL || n != url_buf[0] as usize {
                return Ok(String::new());
            }
        }
        _ => return Ok(String::new()),
    }

    let scheme = match url_buf[2] {
        0 => "http://",
        1 => "https://",
        255 => "",
        _ => return Err("Bad URL scheme"),
    };
    let mut url = scheme.to_string();
    // Crude UTF-8 to ASCII conversion.
    for &c in &url_buf[3..url_buf[0] as usize] {
        if c < 0x80 {
            url.push(if c < 0x20 || c == 0x7f {
                '?'
            } else {
                c as char
            });
        }
    }
    Ok(url)
}

fn dump_platform_device_capability_desc(fd: Option<&Handle>, buf: &[u8], node: &mut Node) {
    let s = node.section("Platform");
    let desc_len = buf[0];
    if desc_len < 20 {
        too_short(s);
        return;
    }
    cap_header(buf, s);
    s.field("bReserved", buf[3]);
    let guid = get_guid(&buf[4..]);
    s.field("PlatformCapabilityUUID", &guid);

    if guid == WEBUSB_GUID && desc_len == 24 {
        // WebUSB platform descriptor.
        let w = s.section("WebUSB");
        w.field("bcdVersion", bcd(le16(&buf[20..]) as u16));
        w.field("bVendorCode", buf[22]);
        let page = w.field("iLandingPage", buf[23]);
        match get_webusb_url(fd, buf[22], buf[23]) {
            Ok(url) => {
                page.note(url);
            }
            Err(e) => {
                page.warn(e);
            }
        }
        return;
    }

    for i in 0..(desc_len - 20) as usize {
        s.field(format!("CapabilityData[{i}]"), hex2(buf[20 + i]));
    }
}

fn dump_billboard_device_capability_desc(dev: Option<&Handle>, buf: &[u8], node: &mut Node) {
    let s = node.section("Billboard");
    if buf[0] < 48 {
        too_short(s);
        return;
    }
    if buf[4] > BILLBOARD_MAX_NUM_ALT_MODE {
        s.warn(format!("Invalid bNumberOfAlternateModes {}", buf[4]));
        return;
    }
    if (buf[0] as usize) < 44 + buf[4] as usize * 4 {
        s.warn("bLength does not match bNumberOfAlternateModes");
        return;
    }

    let w_vconn_power = le16(&buf[6..]);
    let vconn = if w_vconn_power & (1 << 15) != 0 {
        "VCONN power not required"
    } else if w_vconn_power < 7 {
        VCONN_POWER[(w_vconn_power & 0x7) as usize]
    } else {
        "reserved"
    };
    cap_header(buf, s);
    s.field("iAdditionalInfoURL", buf[3])
        .note(get_dev_string(dev, buf[3]));
    s.field("bNumberOfAlternateModes", buf[4]);
    s.field("bPreferredAlternateMode", buf[5]);
    s.field("VCONNPower", w_vconn_power).note(vconn);

    let bm_configured = &buf[8..40];
    s.field("bmConfigured", hex_bytes(bm_configured));
    s.field(
        "bcdVersion",
        format!(
            "{:x}.{:02x}",
            if buf[41] == 0 { 1 } else { buf[41] },
            buf[40]
        ),
    );
    s.field("bAdditionalFailureInfo", buf[42]);
    s.field("bReserved", buf[43]);

    // Alternate mode 0 starts at index 44.
    for alt_mode in 0..buf[4] as usize {
        let i = 44 + alt_mode * 4;
        let state = (bm_configured[alt_mode >> 2] >> ((alt_mode & 0x3) << 1)) & 0x3;
        let m = s
            .section(format!("Alternate Mode {alt_mode}"))
            .note(ALT_MODE_STATE[state as usize]);
        m.field("wSVID", hex4(le16(&buf[i..])));
        m.field("bAlternateMode", buf[i + 2]);
        m.field("iAlternateModeString", buf[i + 3])
            .note(get_dev_string(dev, buf[i + 3]));
    }
}

fn dump_billboard_alt_mode_capability_desc(buf: &[u8], node: &mut Node) {
    let s = node.section("Billboard Alternate Mode");
    if buf[0] != 8 {
        s.warn(format!("Invalid descriptor length {}", buf[0]));
        return;
    }
    cap_header(buf, s);
    s.field("bIndex", buf[3]);
    s.field("dwAlternateModeVdo", hex8(le32(&buf[4..])));
}

fn dump_fwstatus_capability_desc(buf: &[u8], node: &mut Node) {
    let s = node.section("FWStatus");
    if buf[0] != 8 {
        s.warn(format!("Invalid descriptor length {}", buf[0]));
        return;
    }
    let flags = le32(&buf[4..]);
    let yes_no = |bit: u32| if flags & bit != 0 { "Yes" } else { "No" };
    cap_header(buf, s);
    s.field("bcdDescriptorVersion", buf[3]);
    let attr = s.field("bmAttributes", hex8(flags));
    attr.text(format!("Get FW Image Hash support: {}", yes_no(1 << 0)));
    attr.text(format!("Disallow FW Update support: {}", yes_no(1 << 1)));
}

pub(super) fn dump_bos_descriptor(
    fd: &Handle,
    has_ssp: &mut bool,
    lpm_required: bool,
    node: &mut Node,
) {
    // Total length of BOS descriptors varies.  Read first static 5 bytes
    // which include the total length before allocating and reading the
    // full BOS.
    let mut bos_desc_static = [0u8; 5];

    match control_in(
        fd,
        usb::RECIPIENT_DEVICE,
        usb::REQUEST_GET_DESCRIPTOR,
        (USB_DT_BOS as u16) << 8,
        0,
        &mut bos_desc_static,
    ) {
        Ok(n) if n > 0 => {}
        _ => return,
    }
    if bos_desc_static[0] != 5 || bos_desc_static[1] != USB_DT_BOS {
        return;
    }

    let bos_desc_size = le16(&bos_desc_static[2..]) as usize;
    let s = node.section("Binary Object Store");
    s.field("bLength", bos_desc_static[0]);
    s.field("bDescriptorType", bos_desc_static[1]);
    s.field("wTotalLength", hex4(bos_desc_size as u16));
    s.field("bNumDeviceCaps", bos_desc_static[4]);

    if bos_desc_size <= 5 {
        if bos_desc_static[4] > 0 {
            s.warn("Couldn't get device capability descriptors");
        }
        return;
    }

    let mut bos_desc = vec![0u8; bos_desc_size];
    if control_in(
        fd,
        usb::RECIPIENT_DEVICE,
        usb::REQUEST_GET_DESCRIPTOR,
        (USB_DT_BOS as u16) << 8,
        0,
        &mut bos_desc,
    )
    .is_err()
    {
        s.warn("Couldn't get device capability descriptors");
        return;
    }

    dump_device_capabilities(Some(fd), &bos_desc[5..], has_ssp, lpm_required, s);
}

/// Dump the device capability descriptors that follow the BOS header.
pub(super) fn dump_device_capabilities(
    fd: Option<&Handle>,
    caps: &[u8],
    has_ssp: &mut bool,
    lpm_required: bool,
    node: &mut Node,
) {
    let mut pos = 0;
    while caps.len() - pos >= 3 {
        let size = caps.len() - pos;
        let len = caps[pos] as usize;
        if len < 3 || len > size {
            node.warn(format!(
                "Bad device-capability bLength {len} ({size} bytes left)"
            ));
            return;
        }
        let buf = usb::padded(&caps[pos..pos + len]);
        match buf[2] {
            USB_DC_WIRELESS_USB => {
                // It's dead!  Luckily no one has these devices so we can
                // ignore it.
            }
            USB_DC_20_EXTENSION => dump_usb2_device_capability_desc(&buf, lpm_required, node),
            USB_DC_SUPERSPEED => dump_ss_device_capability_desc(&buf, node),
            USB_DC_SUPERSPEEDPLUS => {
                dump_ssp_device_capability_desc(&buf, node);
                *has_ssp = true;
            }
            USB_DC_CONTAINER_ID => dump_container_id_device_capability_desc(&buf, node),
            USB_DC_PLATFORM => dump_platform_device_capability_desc(fd, &buf, node),
            USB_DC_BILLBOARD => dump_billboard_device_capability_desc(fd, &buf, node),
            USB_DC_BILLBOARD_ALT_MODE => dump_billboard_alt_mode_capability_desc(&buf, node),
            USB_DC_CONFIGURATION_SUMMARY => {
                let s = node.section("Configuration Summary");
                desc_dump(
                    fd,
                    DESC_USB3_DC_CONFIGURATION_SUMMARY,
                    &buf,
                    DESC_BUF_LEN_FROM_BUF,
                    s,
                );
            }
            USB_DC_FWSTATUS_CAPABILITY => dump_fwstatus_capability_desc(&buf, node),
            _ => {
                let s = node.section("Device Capability").value(hex2(buf[2]));
                unrecognized(&buf, s);
            }
        }
        pos += len;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Kind;

    fn texts(node: &Node) -> Vec<&str> {
        node.children
            .iter()
            .filter(|c| c.kind == Kind::Text)
            .map(|c| c.name.as_str())
            .collect()
    }

    #[test]
    fn capabilities() {
        let mut caps = vec![];
        // USB 2.0 Extension: BESL LPM, baseline BESL 0x2 (200us), deep
        // BESL 0x6 (1000us).
        caps.extend([7, 0x10, USB_DC_20_EXTENSION, 0x1e, 0x62, 0, 0]);
        // Container ID.
        caps.extend([20, 0x10, USB_DC_CONTAINER_ID, 0]);
        caps.extend([
            0x38, 0xb6, 0x08, 0x34, 0xa9, 0x09, 0xa0, 0x47, 0x8b, 0xfd, 0xa0, 0x76, 0x88, 0x15,
            0xb6, 0x65,
        ]);
        // SuperSpeedPlus with two sublink speed attributes (10Gb/s RX/TX).
        caps.extend([20, 0x10, USB_DC_SUPERSPEEDPLUS, 0, 0x01, 0, 0, 0]);
        caps.extend([
            0x00, 0x11, 0, 0, 0x30, 0x40, 0x0a, 0x00, 0xb0, 0x40, 0x0a, 0x00,
        ]);

        let mut node = Node::new("x");
        let mut has_ssp = false;
        dump_device_capabilities(None, &caps, &mut has_ssp, true, &mut node);
        assert!(has_ssp);
        assert!(!node.has_warnings());

        let usb2 = node.get("USB 2.0 Extension").unwrap();
        let attr = usb2.get("bmAttributes").unwrap();
        assert_eq!(attr.value, "0x0000621e");
        assert_eq!(
            texts(attr),
            [
                "BESL Link Power Management (LPM) Supported",
                "Baseline BESL value 200µs",
                "Deep BESL value 1000µs"
            ]
        );

        let id = node.get("Container ID").unwrap();
        assert_eq!(
            id.get("ContainerID").unwrap().value,
            "{3408b638-09a9-47a0-8bfd-a0768815b665}"
        );

        let ssp = node.get("SuperSpeedPlus USB").unwrap();
        assert_eq!(
            texts(ssp.get("bmAttributes").unwrap()),
            [
                "Sublink Speed Attribute count 2",
                "Sublink Speed ID count 1"
            ]
        );
        assert_eq!(
            ssp.get("bmSublinkSpeedAttr[1]").unwrap().note,
            "Speed Attribute ID 0: 10Gb/s Symmetric TX SuperSpeedPlus"
        );
    }

    #[test]
    fn missing_lpm_bit_and_bad_length() {
        let caps = [7, 0x10, USB_DC_20_EXTENSION, 0x04, 0, 0, 0, 9, 0x10, 3];
        let mut node = Node::new("x");
        let mut has_ssp = false;
        dump_device_capabilities(None, &caps, &mut has_ssp, false, &mut node);
        assert!(!has_ssp);
        let attr = node
            .get("USB 2.0 Extension")
            .unwrap()
            .get("bmAttributes")
            .unwrap();
        assert_eq!(attr.children[0].kind, Kind::Warning);
        assert_eq!(
            node.children.last().unwrap().name,
            "Bad device-capability bLength 9 (3 bytes left)"
        );
    }
}
