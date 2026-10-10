// SPDX-License-Identifier: GPL-2.0-or-later
//! Verbose (`-v`) descriptor dumping.
//!
//! Derived from the dumping code in usbutils' `lsusb.c`
//! (Copyright (C) 1999-2001, 2003 Thomas Sailer, Copyright (C) 2003-2005
//! David Brownell).
//!
//! Descriptor dumpers take a zero-padded copy of the descriptor (see
//! [`usb::padded`]) so the byte indexing can follow the C code closely.

mod audio;
mod bos;
mod class;
mod hid;
mod video;

use std::sync::atomic::{AtomicU32, Ordering};

use crate::names::{
    get_class_string, get_protocol_string, get_subclass_string, get_vendor_product_with_fallback,
};
use crate::sysfs;
use crate::usb::{
    self, control_in, ConfigDescriptor, Device, DeviceDescriptor, EndpointDescriptor, Handle,
    Interface, InterfaceDescriptor, Speed,
};
use crate::usbmisc::get_dev_string;

static VERBLEVEL: AtomicU32 = AtomicU32::new(0);

pub fn set_verblevel(level: u32) {
    VERBLEVEL.store(level, Ordering::Relaxed);
}

pub fn verblevel() -> u32 {
    VERBLEVEL.load(Ordering::Relaxed)
}

// From USB 2.0 spec and updates.
const USB_DT_DEVICE_QUALIFIER: u8 = 0x06;
const USB_DT_OTG: u8 = 0x09;
const USB_DT_DEBUG: u8 = 0x0a;
const USB_DT_INTERFACE_ASSOCIATION: u8 = 0x0b;
const USB_DT_SECURITY: u8 = 0x0c;
const USB_DT_ENCRYPTION_TYPE: u8 = 0x0e;
const USB_DT_BOS: u8 = 0x0f;
const USB_DT_SS_ENDPOINT_COMP: u8 = 0x30;

// Conventional codes for class-specific descriptors (Common Class Spec 3.11).
const USB_DT_CS_DEVICE: u8 = usb::REQUEST_TYPE_CLASS | usb::DT_DEVICE;
const USB_DT_CS_INTERFACE: u8 = usb::REQUEST_TYPE_CLASS | usb::DT_INTERFACE;
const USB_DT_CS_ENDPOINT: u8 = usb::REQUEST_TYPE_CLASS | usb::DT_ENDPOINT;

const USB_CLASS_CCID: u8 = 0x0b;
const USB_CLASS_VIDEO: u8 = 0x0e;
const USB_CLASS_APPLICATION: u8 = 0xfe;

/// Max 3 bytes hub status = hub + 23 ports.
const HUB_STATUS_BYTELEN: usize = 3;

const ENCRYPTION_TYPE: [&str; 5] = ["INSECURE", "WIRED", "CCM_1", "RSA_1", "RESERVED"];

// ---------------------------------------------------------------------------

/// Print `" %02x"` for each of the first `len` bytes, then a newline.
pub(crate) fn dump_bytes(buf: &[u8], len: usize) {
    for b in &buf[..len.min(buf.len())] {
        print!(" {b:02x}");
    }
    println!();
}

/// Print any bytes between `len` and `bLength`.
pub(crate) fn dump_junk(buf: &[u8], indent: &str, len: usize) {
    if buf[0] as usize <= len {
        return;
    }
    print!("{indent}junk at descriptor end:");
    for b in &buf[len..buf[0] as usize] {
        print!(" {b:02x}");
    }
    println!();
}

pub(crate) fn check_descriptor_length(size: u32, min: u32) -> bool {
    if size < min {
        println!("      Warning: Descriptor too short, is {size}, must be at least {min}");
        return false;
    }
    true
}

/// Format a 16-byte GUID (first fields little endian, RFC 4122 style).
pub(crate) fn get_guid(buf: &[u8]) -> String {
    format!(
        "{{{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}}}",
        buf[3], buf[2], buf[1], buf[0], buf[5], buf[4], buf[7], buf[6], buf[8], buf[9], buf[10],
        buf[11], buf[12], buf[13], buf[14], buf[15]
    )
}

/// Iterate over the descriptors packed into an "extra" buffer.
///
/// Calls `f` with a padded copy of each descriptor.  Stops (after calling
/// `bad` with the offending `bLength` and remaining size) on a malformed
/// length.
fn for_each_extra(extra: &[u8], mut bad: impl FnMut(u8, usize), mut f: impl FnMut(&[u8])) {
    let mut pos = 0;
    while extra.len() - pos >= 2 {
        let size = extra.len() - pos;
        let len = extra[pos] as usize;
        if len < 2 || len > size {
            bad(extra[pos], size);
            break;
        }
        f(&usb::padded(&extra[pos..pos + len]));
        pos += len;
    }
}

// ---------------------------------------------------------------------------

fn dump_device(dev: &Device, handle: Option<&Handle>, descriptor: &DeviceDescriptor) {
    let (vendor, product) = get_vendor_product_with_fallback(dev, handle);
    let cls = get_class_string(descriptor.b_device_class);
    let subcls = get_subclass_string(descriptor.b_device_class, descriptor.b_device_sub_class);
    let proto = get_protocol_string(
        descriptor.b_device_class,
        descriptor.b_device_sub_class,
        descriptor.b_device_protocol,
    );

    // Upstream reads these from sysfs (to avoid waking up devices); where
    // there's no sysfs, ask the device instead.
    let (mfg, prod, serial) = if sysfs::available() {
        match sysfs::get_sysfs_name(dev) {
            Some(name) => (
                sysfs::read_sysfs_prop(&name, "manufacturer"),
                sysfs::read_sysfs_prop(&name, "product"),
                sysfs::read_sysfs_prop(&name, "serial"),
            ),
            None => Default::default(),
        }
    } else {
        (
            get_dev_string(handle, descriptor.i_manufacturer),
            get_dev_string(handle, descriptor.i_product),
            get_dev_string(handle, descriptor.i_serial_number),
        )
    };

    let negotiated_speed = match usb::device_speed(dev) {
        Speed::Low => "Low Speed (1Mbps)",
        Speed::Full => "Full Speed (12Mbps)",
        Speed::High => "High Speed (480Mbps)",
        Speed::Super => "SuperSpeed (5Gbps)",
        Speed::SuperPlus => "SuperSpeed+ (10Gbps)",
        Speed::SuperPlusX2 => "SuperSpeed++ (20Gbps)",
        Speed::Unknown => "Unknown",
    };
    println!("Negotiated speed: {negotiated_speed}");

    let d = descriptor;
    println!("Device Descriptor:");
    println!("  bLength             {:5}", d.b_length);
    println!("  bDescriptorType     {:5}", d.b_descriptor_type);
    println!(
        "  bcdUSB              {:2x}.{:02x}",
        d.bcd_usb >> 8,
        d.bcd_usb & 0xff
    );
    println!("  bDeviceClass        {:5} {}", d.b_device_class, cls);
    println!(
        "  bDeviceSubClass     {:5} {}",
        d.b_device_sub_class, subcls
    );
    println!("  bDeviceProtocol     {:5} {}", d.b_device_protocol, proto);
    println!("  bMaxPacketSize0     {:5}", d.b_max_packet_size0);
    println!("  idVendor           0x{:04x} {}", d.id_vendor, vendor);
    println!("  idProduct          0x{:04x} {}", d.id_product, product);
    println!(
        "  bcdDevice           {:2x}.{:02x}",
        d.bcd_device >> 8,
        d.bcd_device & 0xff
    );
    println!("  iManufacturer       {:5} {}", d.i_manufacturer, mfg);
    println!("  iProduct            {:5} {}", d.i_product, prod);
    println!("  iSerial             {:5} {}", d.i_serial_number, serial);
    println!("  bNumConfigurations  {:5}", d.b_num_configurations);
}

fn dump_security(buf: &[u8]) {
    if buf[0] < 5 {
        println!("    Warning: Security descriptor too short");
        return;
    }
    println!("    Security Descriptor:");
    println!("      bLength             {:5}", buf[0]);
    println!("      bDescriptorType     {:5}", buf[1]);
    println!("      wTotalLength       0x{:04x}", usb::le16(&buf[2..]));
    println!("      bNumEncryptionTypes {:5}", buf[4]);
}

fn dump_encryption_type(buf: &[u8]) {
    if buf[0] < 5 {
        println!("    Warning: Encryption Type descriptor too short");
        return;
    }
    let b_encryption_type = buf[2] as usize;
    println!("    Encryption Type Descriptor:");
    println!("      bLength             {:5}", buf[0]);
    println!("      bDescriptorType     {:5}", buf[1]);
    println!(
        "      bEncryptionType     {:5} {}",
        buf[2],
        ENCRYPTION_TYPE
            .get(b_encryption_type)
            .unwrap_or(&"RESERVED")
    );
    println!("      bEncryptionValue    {:5}", buf[3]);
    println!("      bAuthKeyIndex       {:5}", buf[4]);
}

fn dump_association(dev: Option<&Handle>, buf: &[u8]) {
    if buf[0] < 8 {
        println!("    Warning: Interface Association descriptor too short");
        return;
    }
    let cls = get_class_string(buf[4]);
    let subcls = get_subclass_string(buf[4], buf[5]);
    let proto = get_protocol_string(buf[4], buf[5], buf[6]);
    let func = get_dev_string(dev, buf[7]);

    println!("    Interface Association:");
    println!("      bLength             {:5}", buf[0]);
    println!("      bDescriptorType     {:5}", buf[1]);
    println!("      bFirstInterface     {:5}", buf[2]);
    println!("      bInterfaceCount     {:5}", buf[3]);
    println!("      bFunctionClass      {:5} {}", buf[4], cls);
    println!("      bFunctionSubClass   {:5} {}", buf[5], subcls);
    println!("      bFunctionProtocol   {:5} {}", buf[6], proto);
    println!("      iFunction           {:5} {}", buf[7], func);
}

fn dump_config(dev: Option<&Handle>, config: &ConfigDescriptor, speed: u16) {
    let cfg = get_dev_string(dev, config.i_configuration);

    println!("  Configuration Descriptor:");
    println!("    bLength             {:5}", config.b_length);
    println!("    bDescriptorType     {:5}", config.b_descriptor_type);
    println!("    wTotalLength       0x{:04x}", config.w_total_length);
    println!("    bNumInterfaces      {:5}", config.b_num_interfaces);
    println!("    bConfigurationValue {:5}", config.b_configuration_value);
    println!(
        "    iConfiguration      {:5} {}",
        config.i_configuration, cfg
    );
    println!("    bmAttributes         0x{:02x}", config.bm_attributes);

    if config.bm_attributes & 0x80 == 0 {
        println!("      (Missing must-be-set bit!)");
    }
    if config.bm_attributes & 0x40 != 0 {
        println!("      Self Powered");
    } else {
        println!("      (Bus Powered)");
    }
    if config.bm_attributes & 0x20 != 0 {
        println!("      Remote Wakeup");
    }
    if config.bm_attributes & 0x10 != 0 {
        println!("      Battery Powered");
    }
    println!(
        "    MaxPower            {:5}mA",
        config.max_power as u32 * if speed >= 0x0300 { 8 } else { 2 }
    );

    // Avoid re-ordering or hiding descriptors for display.
    for_each_extra(
        &config.extra,
        |len, left| println!("    ** Bad config-extra bLength {len} ({left} left)"),
        |buf| match buf[1] {
            USB_DT_OTG => {} // handled separately
            USB_DT_INTERFACE_ASSOCIATION => dump_association(dev, buf),
            USB_DT_SECURITY => dump_security(buf),
            USB_DT_ENCRYPTION_TYPE => dump_encryption_type(buf),
            _ => {
                // Often a misplaced class descriptor.
                print!("    ** UNRECOGNIZED: ");
                dump_bytes(buf, buf[0] as usize);
            }
        },
    );

    for interface in &config.interfaces {
        dump_interface(dev, interface);
    }
}

/// Dump one class-specific (or otherwise unknown) descriptor found in an
/// interface's extra data.  Returns false if it wasn't recognized.
fn dump_altsetting_extra(
    dev: Option<&Handle>,
    interface: &InterfaceDescriptor,
    buf: &[u8],
) -> bool {
    let class = interface.b_interface_class;
    let subclass = interface.b_interface_sub_class;
    let protocol = interface.b_interface_protocol;

    match buf[1] {
        // This is the polite way to provide class specific descriptors:
        // explicitly tagged, using common class spec conventions.
        USB_DT_CS_DEVICE | USB_DT_CS_INTERFACE => match class {
            usb::CLASS_AUDIO => match subclass {
                1 => audio::dump_audiocontrol_interface(dev, buf, protocol),
                2 => audio::dump_audiostreaming_interface(dev, buf, protocol),
                3 => audio::dump_midistreaming_interface(dev, buf),
                _ => return false,
            },
            usb::CLASS_COMM => class::dump_comm_descriptor(dev, buf, "      "),
            USB_CLASS_VIDEO => match subclass {
                1 => video::dump_videocontrol_interface(dev, buf, protocol),
                2 => video::dump_videostreaming_interface(buf),
                _ => return false,
            },
            USB_CLASS_APPLICATION => match subclass {
                1 => class::dump_dfu_interface(buf),
                _ => return false,
            },
            usb::CLASS_HID => hid::dump_hid_device(dev, interface, buf),
            usb::CLASS_PRINTER => class::dump_printer_device(dev, interface, buf),
            USB_CLASS_CCID => class::dump_ccid_device(buf),
            _ => return false,
        },

        // This is the ugly way: implicitly tagged, each class could
        // redefine the type IDs.
        _ => match class {
            usb::CLASS_HID => hid::dump_hid_device(dev, interface, buf),
            USB_CLASS_CCID => class::dump_ccid_device(buf),
            0xe0 => match (subclass, protocol) {
                // wireless
                (1, 2) => class::dump_rc_interface(buf),
                (2, _) => class::dump_wire_adapter(buf),
                _ => return false,
            },
            usb::CLASS_AUDIO => match (buf[1], subclass) {
                // MISPLACED DESCRIPTOR
                (USB_DT_CS_ENDPOINT, 2) => audio::dump_audiostreaming_endpoint(dev, buf, protocol),
                _ => return false,
            },
            // ... not everything is class-specific
            _ => match buf[1] {
                USB_DT_OTG => {} // handled separately
                USB_DT_INTERFACE_ASSOCIATION => dump_association(dev, buf),
                _ => return false,
            },
        },
    }
    true
}

fn dump_altsetting(dev: Option<&Handle>, interface: &InterfaceDescriptor) {
    let cls = get_class_string(interface.b_interface_class);
    let subcls = get_subclass_string(interface.b_interface_class, interface.b_interface_sub_class);
    let proto = get_protocol_string(
        interface.b_interface_class,
        interface.b_interface_sub_class,
        interface.b_interface_protocol,
    );
    let ifstr = get_dev_string(dev, interface.i_interface);

    println!("    Interface Descriptor:");
    println!("      bLength             {:5}", interface.b_length);
    println!(
        "      bDescriptorType     {:5}",
        interface.b_descriptor_type
    );
    println!(
        "      bInterfaceNumber    {:5}",
        interface.b_interface_number
    );
    println!(
        "      bAlternateSetting   {:5}",
        interface.b_alternate_setting
    );
    println!("      bNumEndpoints       {:5}", interface.b_num_endpoints);
    println!(
        "      bInterfaceClass     {:5} {}",
        interface.b_interface_class, cls
    );
    println!(
        "      bInterfaceSubClass  {:5} {}",
        interface.b_interface_sub_class, subcls
    );
    println!(
        "      bInterfaceProtocol  {:5} {}",
        interface.b_interface_protocol, proto
    );
    println!(
        "      iInterface          {:5} {}",
        interface.i_interface, ifstr
    );

    // Avoid re-ordering or hiding descriptors for display.
    for_each_extra(
        &interface.extra,
        |len, left| println!("      ** Bad interface-extra bLength {len} ({left} left)"),
        |buf| {
            if !dump_altsetting_extra(dev, interface, buf) {
                // Often a misplaced class descriptor.
                print!("      ** UNRECOGNIZED: ");
                dump_bytes(buf, buf[0] as usize);
            }
        },
    );

    for endpoint in &interface.endpoints {
        dump_endpoint(dev, interface, endpoint);
    }
}

fn dump_interface(dev: Option<&Handle>, interface: &Interface) {
    for alt in &interface.altsettings {
        dump_altsetting(dev, alt);
    }
}

fn dump_pipe_desc(buf: &[u8]) {
    if buf[0] == 4 && buf[1] == 0x24 {
        let pipe_name = match buf[2] {
            1 => "Command pipe",
            2 => "Status pipe",
            3 => "Data-in pipe",
            4 => "Data-out pipe",
            0xe0..=0xef => "Vendor specific",
            _ => "Reserved",
        };
        println!("        {} (0x{:02x})", pipe_name, buf[2]);
    } else {
        print!("        INTERFACE CLASS: ");
        dump_bytes(buf, buf[0] as usize);
    }
}

fn dump_endpoint(
    dev: Option<&Handle>,
    interface: &InterfaceDescriptor,
    endpoint: &EndpointDescriptor,
) {
    const TYPEATTR: [&str; 4] = ["Control", "Isochronous", "Bulk", "Interrupt"];
    const SYNCATTR: [&str; 4] = ["None", "Asynchronous", "Adaptive", "Synchronous"];
    const USAGE: [&str; 4] = ["Data", "Feedback", "Implicit feedback Data", "(reserved)"];
    const HB: [&str; 4] = ["1x", "2x", "3x", "(??)"];

    let wmax = endpoint.w_max_packet_size as usize;
    let attr = endpoint.bm_attributes as usize;

    println!("      Endpoint Descriptor:");
    println!("        bLength             {:5}", endpoint.b_length);
    println!(
        "        bDescriptorType     {:5}",
        endpoint.b_descriptor_type
    );
    println!(
        "        bEndpointAddress     0x{:02x}  EP {} {}",
        endpoint.b_endpoint_address,
        endpoint.b_endpoint_address & 0x0f,
        if endpoint.b_endpoint_address & 0x80 != 0 {
            "IN"
        } else {
            "OUT"
        }
    );
    println!("        bmAttributes        {:5}", endpoint.bm_attributes);
    println!("          Transfer Type            {}", TYPEATTR[attr & 3]);
    println!(
        "          Synch Type               {}",
        SYNCATTR[(attr >> 2) & 3]
    );
    println!(
        "          Usage Type               {}",
        USAGE[(attr >> 4) & 3]
    );
    println!(
        "        wMaxPacketSize     0x{:04x}  {} {} bytes",
        wmax,
        HB[(wmax >> 11) & 3],
        wmax & 0x7ff
    );
    println!("        bInterval           {:5}", endpoint.b_interval);
    // Only for audio endpoints.
    if endpoint.b_length == 9 {
        println!("        bRefresh            {:5}", endpoint.b_refresh);
        println!("        bSynchAddress       {:5}", endpoint.b_synch_address);
    }

    let class = interface.b_interface_class;
    let subclass = interface.b_interface_sub_class;

    // Avoid re-ordering or hiding descriptors for display.
    for_each_extra(
        &endpoint.extra,
        |len, left| println!("        ** Bad endpoint-extra bLength {len} ({left} left)"),
        |buf| match buf[1] {
            USB_DT_CS_ENDPOINT => {
                if class == 1 && subclass == 2 {
                    audio::dump_audiostreaming_endpoint(dev, buf, interface.b_interface_protocol);
                } else if class == 1 && subclass == 3 {
                    audio::dump_midistreaming_endpoint(dev, buf);
                } else if class == 14 && subclass == 1 {
                    video::dump_videocontrol_interrupt_endpoint(buf);
                }
            }
            USB_DT_CS_INTERFACE => {
                // MISPLACED DESCRIPTOR ... less indent
                match class {
                    usb::CLASS_COMM | usb::CLASS_DATA => {
                        class::dump_comm_descriptor(dev, buf, "      ")
                    }
                    usb::CLASS_MASS_STORAGE => dump_pipe_desc(buf),
                    _ => {
                        print!("        INTERFACE CLASS: ");
                        dump_bytes(buf, buf[0] as usize);
                    }
                }
            }
            USB_DT_CS_DEVICE => {
                // MISPLACED DESCRIPTOR ... less indent
                match class {
                    USB_CLASS_CCID => class::dump_ccid_device(buf),
                    _ => {
                        print!("        DEVICE CLASS: ");
                        dump_bytes(buf, buf[0] as usize);
                    }
                }
            }
            USB_DT_OTG => {} // handled separately
            USB_DT_INTERFACE_ASSOCIATION => dump_association(dev, buf),
            USB_DT_SS_ENDPOINT_COMP => {
                if buf[0] < 6 {
                    println!("        Warning: SuperSpeed Endpoint Companion descriptor too short");
                    return;
                }
                println!("        bMaxBurst {:15}", buf[2]);
                // Print bulk streams info or isoc "Mult".
                if attr & 3 == 2 && buf[3] & 0x1f != 0 {
                    println!("        MaxStreams {:14}", 1u32 << (buf[3] & 0x1f));
                }
                if attr & 3 == 1 && buf[3] & 0x3 != 0 {
                    println!("        Mult {:20}", buf[3] & 0x3);
                }
                if attr & 3 == 1 || attr & 3 == 3 {
                    println!("        wBytesPerInterval {:7}", usb::le16(&buf[4..]));
                }
            }
            _ => {
                // Often a misplaced class descriptor.
                print!("        ** UNRECOGNIZED: ");
                dump_bytes(buf, buf[0] as usize);
            }
        },
    );
}

// ---------------------------------------------------------------------------

pub(crate) fn dump_hub(prefix: &str, p: &[u8], tt_type: u8) {
    let w_hub_char = (p[4] as u32) << 8 | p[3] as u32;

    println!("{prefix}Hub Descriptor:");
    println!("{prefix}  bLength             {:3}", p[0]);
    println!("{prefix}  bDescriptorType     {:3}", p[1]);
    println!("{prefix}  nNbrPorts           {:3}", p[2]);
    println!("{prefix}  wHubCharacteristic 0x{:04x}", w_hub_char);
    match w_hub_char & 0x03 {
        0 => println!("{prefix}    Ganged power switching"),
        1 => println!("{prefix}    Per-port power switching"),
        _ => println!("{prefix}    No power switching (usb 1.0)"),
    }
    if w_hub_char & 0x04 != 0 {
        println!("{prefix}    Compound device");
    }
    match (w_hub_char >> 3) & 0x03 {
        0 => println!("{prefix}    Ganged overcurrent protection"),
        1 => println!("{prefix}    Per-port overcurrent protection"),
        _ => println!("{prefix}    No overcurrent protection"),
    }
    // USB 3.0 hubs don't have TTs.
    if (1..3).contains(&tt_type) {
        let l = (w_hub_char >> 5) & 0x03;
        println!("{prefix}    TT think time {} FS bits", (l + 1) * 8);
    }
    // USB 3.0 hubs don't have port indicators.  Sad face.
    if tt_type != 3 && w_hub_char & (1 << 7) != 0 {
        println!("{prefix}    Port indicators");
    }
    println!("{prefix}  bPwrOn2PwrGood      {:3} * 2 milli seconds", p[5]);

    // USB 3.0 hubs report current in units of aCurrentUnit, or 4 mA.
    if tt_type == 3 {
        println!(
            "{prefix}  bHubContrCurrent   {:4} milli Ampere",
            p[6] as u32 * 4
        );
    } else {
        println!("{prefix}  bHubContrCurrent    {:3} milli Ampere", p[6]);
    }

    let offset = if tt_type == 3 {
        println!("{prefix}  bHubDecLat          0.{:1} micro seconds", p[7]);
        println!(
            "{prefix}  wHubDelay          {:4} nano seconds",
            (p[9] as u32) << 8 | p[8] as u32
        );
        10
    } else {
        7
    };

    // This determines the variable number of bytes following.
    let l = ((p[2] as usize >> 3) + 1).min(HUB_STATUS_BYTELEN);
    print!("{prefix}  DeviceRemovable   ");
    for i in 0..l {
        print!(" 0x{:02x}", p[offset + i]);
    }

    if tt_type != 3 {
        print!("\n{prefix}  PortPwrCtrlMask   ");
        for j in 0..l {
            print!(" 0x{:02x}", p[offset + l + j]);
        }
    }
    println!();
}

fn errno() -> Option<i32> {
    std::io::Error::last_os_error().raw_os_error()
}

fn do_hub(fd: &Handle, tt_type: u8, speed: u16, has_ssp: bool) {
    const LINK_STATE_DESCRIPTIONS: [&str; 12] = [
        "U0",
        "U1",
        "U2",
        "suspend",
        "SS.disabled",
        "Rx.Detect",
        "SS.Inactive",
        "Polling",
        "Recovery",
        "Hot Reset",
        "Compliance",
        "Loopback",
    ];
    // base descriptor + 2 bitmasks
    let mut buf = [0u8; 7 + 2 * HUB_STATUS_BYTELEN];
    let is_ext_status = tt_type == 3 && speed >= 0x0310 && has_ssp;

    // USB 3.x hubs have a slightly different descriptor.
    let value: u16 = if speed >= 0x0300 { 0x2a } else { 0x29 };

    let ret = match control_in(
        fd,
        usb::REQUEST_TYPE_CLASS | usb::RECIPIENT_DEVICE,
        usb::REQUEST_GET_DESCRIPTOR,
        value << 8,
        0,
        &mut buf,
    ) {
        Ok(n) => n,
        Err(e) => {
            // Linux returns EHOSTUNREACH for suspended devices.
            let errno = errno();
            if errno != Some(libc::EHOSTUNREACH) {
                eprintln!("can't get hub descriptor, {} ({})", usb::error_name(e), e);
            }
            return;
        }
    };
    if ret < 9 {
        // at least one port's bitmasks
        eprintln!("incomplete hub descriptor, {ret} bytes");
        return;
    }
    dump_hub("", &buf, tt_type);

    println!(" Hub Port Status:");
    for i in 0..buf[2] as u16 {
        let mut status = [0u8; 8];

        // Request EXT_PORT_STATUS for USB 3.1 SuperSpeedPlus hubs,
        // PORT_STATUS otherwise.
        let len = if is_ext_status { 8 } else { 4 };
        if let Err(e) = control_in(
            fd,
            usb::REQUEST_TYPE_CLASS | usb::RECIPIENT_OTHER,
            usb::REQUEST_GET_STATUS,
            if is_ext_status { 2 } else { 0 },
            i + 1,
            &mut status[..len],
        ) {
            eprintln!(
                "cannot read port {} status, {} ({})",
                i + 1,
                e,
                usb::error_name(e)
            );
            break;
        }

        print!(
            "   Port {}: {:02x}{:02x}.{:02x}{:02x}",
            i + 1,
            status[3],
            status[2],
            status[1],
            status[0]
        );
        let flag = |byte: u8, mask: u8, s: &'static str| if byte & mask != 0 { s } else { "" };
        // CAPS are used to highlight "transient" states.
        if speed < 0x0300 {
            print!(
                "{}{}{}{}{}",
                flag(status[2], 0x10, " C_RESET"),
                flag(status[2], 0x08, " C_OC"),
                flag(status[2], 0x04, " C_SUSPEND"),
                flag(status[2], 0x02, " C_ENABLE"),
                flag(status[2], 0x01, " C_CONNECT"),
            );
            println!(
                "{}{}{}{}{}{}{}{}{}{}{}",
                flag(status[1], 0x10, " indicator"),
                flag(status[1], 0x08, " test"),
                flag(status[1], 0x04, " highspeed"),
                flag(status[1], 0x02, " lowspeed"),
                flag(status[1], 0x01, " power"),
                flag(status[0], 0x20, " L1"),
                flag(status[0], 0x10, " RESET"),
                flag(status[0], 0x08, " oc"),
                flag(status[0], 0x04, " suspend"),
                flag(status[0], 0x02, " enable"),
                flag(status[0], 0x01, " connect"),
            );
        } else {
            let link_state = (((status[0] & 0xe0) >> 5) + ((status[1] & 0x1) << 3)) as usize;
            print!(
                "{}{}{}{}{}{}",
                flag(status[2], 0x80, " C_CONFIG_ERROR"),
                flag(status[2], 0x40, " C_LINK_STATE"),
                flag(status[2], 0x20, " C_BH_RESET"),
                flag(status[2], 0x10, " C_RESET"),
                flag(status[2], 0x08, " C_OC"),
                flag(status[2], 0x01, " C_CONNECT"),
            );
            print!(
                "{}{}",
                if status[1] & 0x1c == 0 {
                    " 5Gbps"
                } else {
                    " Unknown Speed"
                },
                flag(status[1], 0x02, " power"),
            );
            // Link state is bits 8:5.
            if let Some(s) = LINK_STATE_DESCRIPTIONS.get(link_state) {
                print!(" {s}");
            }
            println!(
                "{}{}{}{}",
                flag(status[0], 0x10, " RESET"),
                flag(status[0], 0x08, " oc"),
                flag(status[0], 0x02, " enable"),
                flag(status[0], 0x01, " connect"),
            );
        }

        if is_ext_status && status[0] & 0x01 != 0 {
            println!(
                "     Ext Status: {:02x}{:02x}.{:02x}{:02x}",
                status[7], status[6], status[5], status[4]
            );
            println!(
                "       RX Speed Attribute ID: {} Lanes: {}",
                status[4] & 0x0f,
                (status[5] & 0x0f) + 1
            );
            println!(
                "       TX Speed Attribute ID: {} Lanes: {}",
                (status[4] >> 4) & 0x0f,
                ((status[5] >> 4) & 0x0f) + 1
            );
        }
    }
}

/// Report a failed standard GET_DESCRIPTOR, unless it's just a stall
/// (the device doesn't have that descriptor) or, at low verbosity, EAGAIN.
fn report_get_descriptor_error(what: &str, err: rusb::Error) {
    let errno = errno();
    if err == rusb::Error::Pipe || errno == Some(libc::EPIPE) {
        return;
    }
    if verblevel() > 1 || errno != Some(libc::EAGAIN) {
        eprintln!("{what}: {err}");
    }
}

fn do_dualspeed(fd: &Handle) {
    let mut buf = [0u8; 10];
    let ret = control_in(
        fd,
        usb::REQUEST_TYPE_STANDARD | usb::RECIPIENT_DEVICE,
        usb::REQUEST_GET_DESCRIPTOR,
        (USB_DT_DEVICE_QUALIFIER as u16) << 8,
        0,
        &mut buf,
    );

    // We don't need to complain to the user if the device is claimed and
    // we aren't allowed to access the device qualifier.
    if let Err(e) = ret {
        report_get_descriptor_error("can't get device qualifier", e);
    }

    // All dual-speed devices have a qualifier.
    if ret != Ok(buf.len()) || buf[0] as usize != buf.len() || buf[1] != USB_DT_DEVICE_QUALIFIER {
        return;
    }

    let cls = get_class_string(buf[4]);
    let subcls = get_subclass_string(buf[4], buf[5]);
    let proto = get_protocol_string(buf[4], buf[5], buf[6]);
    println!("Device Qualifier (for other device speed):");
    println!("  bLength             {:5}", buf[0]);
    println!("  bDescriptorType     {:5}", buf[1]);
    println!("  bcdUSB              {:2x}.{:02x}", buf[3], buf[2]);
    println!("  bDeviceClass        {:5} {}", buf[4], cls);
    println!("  bDeviceSubClass     {:5} {}", buf[5], subcls);
    println!("  bDeviceProtocol     {:5} {}", buf[6], proto);
    println!("  bMaxPacketSize0     {:5}", buf[7]);
    println!("  bNumConfigurations  {:5}", buf[8]);

    // TODO also show the OTHER_SPEED_CONFIG descriptors
}

fn do_debug(fd: &Handle) {
    let mut buf = [0u8; 4];
    let ret = control_in(
        fd,
        usb::REQUEST_TYPE_STANDARD | usb::RECIPIENT_DEVICE,
        usb::REQUEST_GET_DESCRIPTOR,
        (USB_DT_DEBUG as u16) << 8,
        0,
        &mut buf,
    );

    // We don't need to complain to the user if the device is claimed and
    // we aren't allowed to access the debug descriptor.
    if let Err(e) = ret {
        report_get_descriptor_error("can't get debug descriptor", e);
    }

    // Some high speed devices are also "USB2 debug devices", meaning you
    // can use them with some EHCI implementations as another kind of
    // system debug channel: like JTAG, RS232, or a console.
    if ret != Ok(buf.len()) || buf[0] as usize != buf.len() || buf[1] != USB_DT_DEBUG {
        return;
    }

    println!("Debug descriptor:");
    println!("  bLength              {:4}", buf[0]);
    println!("  bDescriptorType      {:4}", buf[1]);
    println!("  bDebugInEndpoint     0x{:02x}", buf[2]);
    println!("  bDebugOutEndpoint    0x{:02x}", buf[3]);
}

fn find_otg(buf: &[u8]) -> Option<&[u8]> {
    let mut pos = 0;
    while buf.len() - pos >= 3 {
        let b = &buf[pos..];
        if b[0] == 3 && b[1] == USB_DT_OTG {
            return Some(b);
        }
        if b[0] < 2 || b[0] as usize > b.len() {
            return None;
        }
        pos += b[0] as usize;
    }
    None
}

fn do_otg(config: &ConfigDescriptor) -> bool {
    // Each config of an otg device has an OTG descriptor.
    let desc = find_otg(&config.extra).or_else(|| {
        config
            .interfaces
            .iter()
            .flat_map(|intf| &intf.altsettings)
            .find_map(|alt| {
                find_otg(&alt.extra)
                    .or_else(|| alt.endpoints.iter().find_map(|ep| find_otg(&ep.extra)))
            })
    });
    let Some(desc) = desc else {
        return false;
    };

    println!("OTG Descriptor:");
    println!("  bLength               {:3}", desc[0]);
    println!("  bDescriptorType       {:3}", desc[1]);
    println!("  bmAttributes         0x{:02x}", desc[2]);
    if desc[2] & 0x01 != 0 {
        println!("    SRP (Session Request Protocol)");
    }
    if desc[2] & 0x02 != 0 {
        println!("    HNP (Host Negotiation Protocol)");
    }
    true
}

fn dump_device_status(fd: &Handle, otg: bool, super_speed: bool) {
    let mut status = [0u8; 2];
    if let Err(e) = control_in(
        fd,
        usb::REQUEST_TYPE_STANDARD | usb::RECIPIENT_DEVICE,
        usb::REQUEST_GET_STATUS,
        0,
        0,
        &mut status,
    ) {
        eprintln!("cannot read device status, {} ({})", e, usb::error_name(e));
        return;
    }

    println!("Device Status:     0x{:02x}{:02x}", status[1], status[0]);
    if status[0] & (1 << 0) != 0 {
        println!("  Self Powered");
    } else {
        println!("  (Bus Powered)");
    }
    if status[0] & (1 << 1) != 0 {
        println!("  Remote Wakeup Enabled");
    }
    if super_speed {
        if status[0] & (1 << 2) != 0 {
            println!("  U1 Enabled");
        }
        if status[0] & (1 << 3) != 0 {
            println!("  U2 Enabled");
        }
        if status[0] & (1 << 4) != 0 {
            println!("  Latency Tolerance Messaging (LTM) Enabled");
        }
    }
    // If both HOST and DEVICE support OTG.
    if otg {
        if status[0] & (1 << 3) != 0 {
            println!("  HNP Enabled");
        }
        if status[0] & (1 << 4) != 0 {
            println!("  HNP Capable");
        }
        if status[0] & (1 << 5) != 0 {
            println!("  ALT port is HNP Capable");
        }
    }
    // For high speed devices with debug descriptors.
    if status[0] & (1 << 6) != 0 {
        println!("  Debug Mode");
    }
}

/// Dump everything exausb knows about a device (`exausb -v`).
pub fn dumpdev(dev: &Device) {
    let handle = match dev.open() {
        Ok(h) => Some(h),
        Err(_) => {
            eprintln!("Couldn't open device, some information will be missing");
            None
        }
    };
    let udev = handle.as_ref();
    let mut otg = false;
    let mut has_ssp = false;

    let desc = usb::device_descriptor(dev);
    dump_device(dev, udev, &desc);
    if desc.b_num_configurations != 0 {
        match usb::config_descriptor(dev, 0) {
            Ok(config) => otg = do_otg(&config) || otg,
            Err(_) => {
                eprintln!(
                    "Couldn't get configuration descriptor 0, some information will be missing"
                )
            }
        }

        for i in 0..desc.b_num_configurations {
            match usb::config_descriptor(dev, i) {
                Ok(config) => dump_config(udev, &config, desc.bcd_usb),
                Err(_) => eprintln!(
                    "Couldn't get configuration descriptor {i}, some information will be missing"
                ),
            }
        }
    }
    let Some(udev) = udev else {
        return;
    };

    if desc.bcd_usb >= 0x0201 {
        bos::dump_bos_descriptor(udev, &mut has_ssp, desc.bcd_usb >= 0x0210);
    }
    if desc.b_device_class == usb::CLASS_HUB {
        do_hub(udev, desc.b_device_protocol, desc.bcd_usb, has_ssp);
    }
    if desc.bcd_usb == 0x0200 {
        do_dualspeed(udev);
    }
    do_debug(udev);
    dump_device_status(udev, otg, desc.bcd_usb >= 0x0300);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guid_format() {
        let buf = [
            0x38, 0xb6, 0x08, 0x34, 0xa9, 0x09, 0xa0, 0x47, 0x8b, 0xfd, 0xa0, 0x76, 0x88, 0x15,
            0xb6, 0x65,
        ];
        assert_eq!(get_guid(&buf), "{3408b638-09a9-47a0-8bfd-a0768815b665}");
    }

    #[test]
    fn otg_search() {
        assert_eq!(
            find_otg(&[4, 0x24, 0, 0, 3, USB_DT_OTG, 3]),
            Some(&[3, USB_DT_OTG, 3][..])
        );
        assert_eq!(find_otg(&[1, 0x24, 0, 3, USB_DT_OTG, 3]), None);
        assert_eq!(find_otg(&[]), None);
    }
}

/// Entry points for differential testing against upstream's C code with
/// synthetic descriptors (no device needed).  Not a stable API.
#[doc(hidden)]
pub mod harness {
    pub use crate::usb::{ConfigDescriptor, EndpointDescriptor, Interface, InterfaceDescriptor};

    /// `dump_config()` without a device handle.
    pub fn dump_config(config: &ConfigDescriptor, speed: u16) {
        super::dump_config(None, config, speed);
    }

    /// `dump_hub()` on a raw hub descriptor.
    pub fn dump_hub(prefix: &str, buf: &[u8], tt_type: u8) {
        super::dump_hub(prefix, &crate::usb::padded(buf), tt_type);
    }

    /// Dump BOS device capability descriptors (the bytes after the 5-byte
    /// BOS header).  Returns whether a SuperSpeedPlus capability was seen.
    pub fn dump_device_capabilities(caps: &[u8], lpm_required: bool) -> bool {
        let mut has_ssp = false;
        super::bos::dump_device_capabilities(None, caps, &mut has_ssp, lpm_required);
        has_ssp
    }

    /// `dump_report_desc()` on a raw HID report descriptor.
    pub fn dump_report_desc(buf: &[u8]) {
        super::hid::dump_report_desc(buf);
    }
}
