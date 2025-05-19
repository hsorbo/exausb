// SPDX-License-Identifier: GPL-2.0-or-later
//! Binary Object Store (BOS) and device capability descriptor dumping.

use super::{dump_bytes, get_guid, USB_DT_BOS};
use crate::desc_defs::DESC_USB3_DC_CONFIGURATION_SUMMARY;
use crate::desc_dump::{desc_dump, DESC_BUF_LEN_FROM_BUF};
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

fn dump_usb2_device_capability_desc(buf: &[u8], lpm_required: bool) {
    const BESL_US: [u16; 16] = [
        125, 150, 200, 300, 400, 500, 1000, 2000, 3000, 4000, 5000, 6000, 7000, 8000, 9000, 10000,
    ];

    if buf[0] < 7 {
        eprintln!("  Bad USB 2.0 Extension Device Capability descriptor.");
        return;
    }
    let wide = le32(&buf[3..]);
    println!("  USB 2.0 Extension Device Capability:");
    println!("    bLength             {:5}", buf[0]);
    println!("    bDescriptorType     {:5}", buf[1]);
    println!("    bDevCapabilityType  {:5}", buf[2]);
    println!("    bmAttributes   0x{:08x}", wide);
    if (lpm_required || wide & 0x04 != 0) && wide & 0x02 == 0 {
        println!("      (Missing must-be-set LPM bit!)");
    } else if !lpm_required && wide & 0x02 == 0 {
        println!("      Link Power Management (LPM) not supported");
    } else if wide & 0x04 == 0 {
        println!("      HIRD Link Power Management (LPM) Supported");
    } else {
        println!("      BESL Link Power Management (LPM) Supported");
        if wide & 0x08 != 0 {
            let besl = ((wide & 0xf00) >> 8) as usize;
            println!("      Baseline BESL value  {:5} us ", BESL_US[besl]);
        }
        if wide & 0x10 != 0 {
            let besl = ((wide & 0xf000) >> 12) as usize;
            println!("      Deep BESL value      {:5} us ", BESL_US[besl]);
        }
    }
}

fn dump_ss_device_capability_desc(buf: &[u8]) {
    if buf[0] < 10 {
        eprintln!("  Bad SuperSpeed USB Device Capability descriptor.");
        return;
    }
    println!("  SuperSpeed USB Device Capability:");
    println!("    bLength             {:5}", buf[0]);
    println!("    bDescriptorType     {:5}", buf[1]);
    println!("    bDevCapabilityType  {:5}", buf[2]);
    println!("    bmAttributes         0x{:02x}", buf[3]);
    if buf[3] & 0x02 != 0 {
        println!("      Latency Tolerance Messages (LTM) Supported");
    }
    println!("    wSpeedsSupported   0x{:02x}{:02x}", buf[5], buf[4]);
    if buf[4] & (1 << 0) != 0 {
        println!("      Device can operate at Low Speed (1Mbps)");
    }
    if buf[4] & (1 << 1) != 0 {
        println!("      Device can operate at Full Speed (12Mbps)");
    }
    if buf[4] & (1 << 2) != 0 {
        println!("      Device can operate at High Speed (480Mbps)");
    }
    if buf[4] & (1 << 3) != 0 {
        println!("      Device can operate at SuperSpeed (5Gbps)");
    }

    println!("    bFunctionalitySupport {:3}", buf[6]);
    let speed = match buf[6] {
        0 => "Low Speed (1Mbps)",
        1 => "Full Speed (12Mbps)",
        2 => "High Speed (480Mbps)",
        3 => "SuperSpeed (5Gbps)",
        _ => "at an unknown speed!",
    };
    println!("      Lowest fully-functional device speed is {speed}");
    println!("    bU1DevExitLat        {:4} micro seconds", buf[7]);
    println!("    bU2DevExitLat    {:8} micro seconds", le16(&buf[8..]));
}

fn dump_ssp_device_capability_desc(buf: &[u8]) {
    const BITRATE_PREFIX: &[u8; 4] = b" KMG";

    if buf[0] < 12 {
        eprintln!("  Bad SuperSpeedPlus USB Device Capability descriptor.");
        return;
    }
    let attr_count = (buf[4] & 0x1f) as usize + 1;
    if (buf[0] as usize) < 12 + attr_count * 4 {
        eprintln!("  Bad SuperSpeedPlus USB Device Capability descriptor.");
        return;
    }

    let bm_attr = le32(&buf[4..]);
    println!("  SuperSpeedPlus USB Device Capability:");
    println!("    bLength             {:5}", buf[0]);
    println!("    bDescriptorType     {:5}", buf[1]);
    println!("    bDevCapabilityType  {:5}", buf[2]);
    println!("    bmAttributes         0x{:08x}", bm_attr);

    println!("      Sublink Speed Attribute count {}", attr_count);
    println!(
        "      Sublink Speed ID count {}",
        ((bm_attr >> 5) & 0xf) + 1
    );
    println!("    wFunctionalitySupport   0x{:02x}{:02x}", buf[9], buf[8]);
    println!("      Min functional Speed Attribute ID: {}", buf[8] & 0x0f);
    println!("      Min functional RX lanes: {}", buf[9] & 0x0f);
    println!("      Min functional TX lanes: {}", (buf[9] >> 4) & 0x0f);

    for i in (0..attr_count).take_while(|i| 12 + i * 4 + 4 <= buf[0] as usize) {
        let ss_attr = le32(&buf[12 + i * 4..]);
        println!("    bmSublinkSpeedAttr[{}]   0x{:08x}", i, ss_attr);
        println!(
            "      Speed Attribute ID: {} {}{}b/s {} {} SuperSpeed{}",
            ss_attr & 0x0f,
            ss_attr >> 16,
            BITRATE_PREFIX[((ss_attr >> 4) & 0x3) as usize] as char,
            if ss_attr & 0x40 != 0 {
                "Asymmetric"
            } else {
                "Symmetric"
            },
            if ss_attr & 0x80 != 0 { "TX" } else { "RX" },
            if ss_attr & 0x4000 != 0 { "Plus" } else { "" }
        );
    }
}

fn dump_container_id_device_capability_desc(buf: &[u8]) {
    if buf[0] < 20 {
        eprintln!("  Bad Container ID Device Capability descriptor.");
        return;
    }
    println!("  Container ID Device Capability:");
    println!("    bLength             {:5}", buf[0]);
    println!("    bDescriptorType     {:5}", buf[1]);
    println!("    bDevCapabilityType  {:5}", buf[2]);
    println!("    bReserved           {:5}", buf[3]);
    println!("    ContainerID             {}", get_guid(&buf[4..]));
}

fn get_webusb_url(fd: Option<&Handle>, vendor_req: u8, id: u8) -> String {
    let Some(fd) = fd else {
        return String::new();
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
                return String::new();
            }
        }
        _ => return String::new(),
    }

    let scheme = match url_buf[2] {
        0 => "http://",
        1 => "https://",
        255 => "",
        _ => {
            eprintln!("Bad URL scheme.");
            return String::new();
        }
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
    url
}

fn dump_platform_device_capability_desc(fd: Option<&Handle>, buf: &[u8]) {
    let desc_len = buf[0];
    if desc_len < 20 {
        eprintln!("  Bad Platform Device Capability descriptor.");
        return;
    }
    let cap_data_len = desc_len - 20;
    println!("  Platform Device Capability:");
    println!("    bLength             {:5}", buf[0]);
    println!("    bDescriptorType     {:5}", buf[1]);
    println!("    bDevCapabilityType  {:5}", buf[2]);
    println!("    bReserved           {:5}", buf[3]);
    let guid = get_guid(&buf[4..]);
    println!("    PlatformCapabilityUUID    {}", guid);

    if guid == WEBUSB_GUID && desc_len == 24 {
        // WebUSB platform descriptor.
        let url = get_webusb_url(fd, buf[22], buf[23]);
        println!("      WebUSB:");
        println!("        bcdVersion   {:2x}.{:02x}", buf[21], buf[20]);
        println!("        bVendorCode  {:5}", buf[22]);
        println!("        iLandingPage {:5} {}", buf[23], url);
        return;
    }

    for i in 0..cap_data_len as usize {
        println!("    CapabilityData[{}]    0x{:02x}", i, buf[20 + i]);
    }
}

fn dump_billboard_device_capability_desc(dev: Option<&Handle>, buf: &[u8]) {
    if buf[0] < 48 {
        eprintln!("  Bad Billboard Capability descriptor.");
        return;
    }
    if buf[4] > BILLBOARD_MAX_NUM_ALT_MODE {
        eprintln!("  Invalid value for bNumberOfAlternateModes.");
        return;
    }
    if (buf[0] as usize) < 44 + buf[4] as usize * 4 {
        eprintln!("  bLength does not match with bNumberOfAlternateModes.");
        return;
    }

    let url = get_dev_string(dev, buf[3]);
    let w_vconn_power = le16(&buf[6..]);
    let vconn = if w_vconn_power & (1 << 15) != 0 {
        "VCONN power not required"
    } else if w_vconn_power < 7 {
        VCONN_POWER[(w_vconn_power & 0x7) as usize]
    } else {
        "reserved"
    };
    println!("  Billboard Capability:");
    println!("    bLength                 {:5}", buf[0]);
    println!("    bDescriptorType         {:5}", buf[1]);
    println!("    bDevCapabilityType      {:5}", buf[2]);
    println!("    iAdditionalInfoURL      {:5} {}", buf[3], url);
    println!("    bNumberOfAlternateModes {:5}", buf[4]);
    println!("    bPreferredAlternateMode {:5}", buf[5]);
    println!("    VCONN Power             {:5} {}", w_vconn_power, vconn);

    let bm_configured = &buf[8..40];
    print!("    bmConfigured               ");
    dump_bytes(bm_configured, 32);

    println!(
        "    bcdVersion              {:2x}.{:02x}",
        if buf[41] == 0 { 1 } else { buf[41] },
        buf[40]
    );
    println!("    bAdditionalFailureInfo  {:5}", buf[42]);
    println!("    bReserved               {:5}", buf[43]);

    println!("    Alternate Modes supported by Device Container:");
    // Alternate mode 0 starts at index 44.
    let mut i = 44;
    for alt_mode in 0..buf[4] as usize {
        let svid = le16(&buf[i..]);
        let alt_mode_str = get_dev_string(dev, buf[i + 3]);
        let state = (bm_configured[alt_mode >> 2] >> ((alt_mode & 0x3) << 1)) & 0x3;
        println!(
            "    Alternate Mode {} : {}",
            alt_mode, ALT_MODE_STATE[state as usize]
        );
        println!(
            "      wSVID[{}]                    0x{:04X}",
            alt_mode, svid
        );
        println!("      bAlternateMode[{}]       {:5}", alt_mode, buf[i + 2]);
        println!(
            "      iAlternateModeString[{}] {:5} {}",
            alt_mode,
            buf[i + 3],
            alt_mode_str
        );
        i += 4;
    }
}

fn dump_billboard_alt_mode_capability_desc(buf: &[u8]) {
    if buf[0] != 8 {
        eprintln!("  Bad Billboard Alternate Mode Capability descriptor.");
        return;
    }
    println!("  Billboard Alternate Mode Capability:");
    println!("    bLength                 {:5}", buf[0]);
    println!("    bDescriptorType         {:5}", buf[1]);
    println!("    bDevCapabilityType      {:5}", buf[2]);
    println!("    bIndex                  {:5}", buf[3]);
    println!("    dwAlternateModeVdo          0x{:08X}", le32(&buf[4..]));
}

fn dump_fwstatus_capability_desc(buf: &[u8]) {
    if buf[0] != 8 {
        eprintln!("  Bad FWStatus Capability descriptor.");
        return;
    }
    let flags = le32(&buf[4..]);
    let yes_no = |bit: u32| if flags & bit != 0 { "Yes" } else { "No" };
    // The odd tabs are in upstream's format string too.
    println!("  FWStatus Capability:");
    println!("    bLength\t\t    {:5}", buf[0]);
    println!("    bDescriptorType\t    {:5}", buf[1]);
    println!("    bDevCapabilityType\t    {:5}", buf[2]);
    println!("    bcdDescriptorVersion   {:5}", buf[3]);
    println!("    bmAttributes\t    0x{:08X}", flags);
    println!("      Get FW Image Hash support: {}", yes_no(1 << 0));
    println!("      Disallow FW Update support: {}", yes_no(1 << 1));
}

pub(super) fn dump_bos_descriptor(fd: &Handle, has_ssp: &mut bool, lpm_required: bool) {
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
    println!("Binary Object Store Descriptor:");
    println!("  bLength             {:5}", bos_desc_static[0]);
    println!("  bDescriptorType     {:5}", bos_desc_static[1]);
    println!("  wTotalLength       0x{:04x}", bos_desc_size);
    println!("  bNumDeviceCaps      {:5}", bos_desc_static[4]);

    if bos_desc_size <= 5 {
        if bos_desc_static[4] > 0 {
            eprintln!("Couldn't get device capability descriptors");
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
        eprintln!("Couldn't get device capability descriptors");
        return;
    }

    dump_device_capabilities(Some(fd), &bos_desc[5..], has_ssp, lpm_required);
}

/// Dump the device capability descriptors that follow the BOS header.
pub(super) fn dump_device_capabilities(
    fd: Option<&Handle>,
    caps: &[u8],
    has_ssp: &mut bool,
    lpm_required: bool,
) {
    let mut pos = 0;
    while caps.len() - pos >= 3 {
        let size = caps.len() - pos;
        let len = caps[pos] as usize;
        if len < 3 || len > size {
            println!("  ** Bad device-capability bLength {} ({} left)", len, size);
            return;
        }
        let buf = usb::padded(&caps[pos..pos + len]);
        match buf[2] {
            USB_DC_WIRELESS_USB => {
                // It's dead!  Luckily no one has these devices so we can
                // ignore it.
            }
            USB_DC_20_EXTENSION => dump_usb2_device_capability_desc(&buf, lpm_required),
            USB_DC_SUPERSPEED => dump_ss_device_capability_desc(&buf),
            USB_DC_SUPERSPEEDPLUS => {
                dump_ssp_device_capability_desc(&buf);
                *has_ssp = true;
            }
            USB_DC_CONTAINER_ID => dump_container_id_device_capability_desc(&buf),
            USB_DC_PLATFORM => dump_platform_device_capability_desc(fd, &buf),
            USB_DC_BILLBOARD => dump_billboard_device_capability_desc(fd, &buf),
            USB_DC_BILLBOARD_ALT_MODE => dump_billboard_alt_mode_capability_desc(&buf),
            USB_DC_CONFIGURATION_SUMMARY => {
                println!("  Configuration Summary Device Capability:");
                desc_dump(
                    fd,
                    DESC_USB3_DC_CONFIGURATION_SUMMARY,
                    &buf,
                    DESC_BUF_LEN_FROM_BUF,
                    2,
                );
            }
            USB_DC_FWSTATUS_CAPABILITY => dump_fwstatus_capability_desc(&buf),
            _ => {
                print!("  ** UNRECOGNIZED: ");
                dump_bytes(&buf, len);
            }
        }
        pos += len;
    }
}
