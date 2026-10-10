// SPDX-License-Identifier: GPL-2.0-or-later
//! Verbose (`-v`) descriptor decoding into the output model.
//!
//! Derived from the dumping code in usbutils' `lsusb.c`
//! (Copyright (C) 1999-2001, 2003 Thomas Sailer, Copyright (C) 2003-2005
//! David Brownell).
//!
//! Every dumper adds a section to the [`Node`] it's given.  Descriptor bytes
//! are passed as a zero-padded copy (see [`usb::padded`]), so dumpers can
//! index a few bytes past `bLength` without bounds checks; they still check
//! `bLength` before trusting any field.

mod audio;
mod bos;
mod class;
mod hid;
mod video;

use crate::model::{bcd, hex2, hex4, hex_bytes, join, Node};
use crate::names::{
    get_vendor_product_with_fallback, names_class, names_product, names_protocol, names_subclass,
    names_vendor,
};
use crate::sysfs;
use crate::usb::{
    self, control_in, ConfigDescriptor, Device, DeviceDescriptor, EndpointDescriptor, Handle,
    InterfaceDescriptor, Speed,
};
use crate::usbmisc::get_dev_string;

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
// Helpers shared by the dumpers

/// Format a 16-byte GUID (first fields little endian, RFC 4122 style).
pub(crate) fn get_guid(buf: &[u8]) -> String {
    format!(
        "{{{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}}}",
        buf[3], buf[2], buf[1], buf[0], buf[5], buf[4], buf[7], buf[6], buf[8], buf[9], buf[10],
        buf[11], buf[12], buf[13], buf[14], buf[15]
    )
}

/// Warn that a descriptor is shorter than its fields need.
pub(crate) fn too_short(node: &mut Node) {
    node.warn("Descriptor too short");
}

/// Warn about any bytes between `len` and `bLength`.
pub(crate) fn junk(buf: &[u8], len: usize, node: &mut Node) {
    if buf[0] as usize > len {
        node.warn(format!(
            "Junk at end of descriptor: {}",
            hex_bytes(&buf[len..buf[0] as usize])
        ));
    }
}

/// Add a warning showing an unrecognized descriptor's bytes.
pub(crate) fn unrecognized(buf: &[u8], node: &mut Node) {
    node.warn(format!(
        "Unrecognized descriptor: {}",
        hex_bytes(&buf[..buf[0] as usize])
    ));
}

/// "IN" or "OUT" for an endpoint address.
pub(crate) fn ep_dir(addr: u8) -> &'static str {
    if addr & 0x80 != 0 {
        "IN"
    } else {
        "OUT"
    }
}

/// Iterate over the descriptors packed into an "extra" buffer, calling `f`
/// with a padded copy of each.  Stops with a warning on a malformed length.
fn for_each_extra(extra: &[u8], node: &mut Node, mut f: impl FnMut(&[u8], &mut Node)) {
    let mut pos = 0;
    while extra.len() - pos >= 2 {
        let left = extra.len() - pos;
        let len = extra[pos] as usize;
        if len < 2 || len > left {
            node.warn(format!("Bad descriptor length {len} ({left} bytes left)"));
            break;
        }
        f(&usb::padded(&extra[pos..pos + len]), node);
        pos += len;
    }
}

fn class_note(class: u8, subclass: u8, protocol: u8) -> String {
    join(&[
        names_class(class).unwrap_or(""),
        names_subclass(class, subclass).unwrap_or(""),
        names_protocol(class, subclass, protocol).unwrap_or(""),
    ])
}

fn speed_name(speed: Speed) -> &'static str {
    match speed {
        Speed::Low => "Low Speed (1Mbps)",
        Speed::Full => "Full Speed (12Mbps)",
        Speed::High => "High Speed (480Mbps)",
        Speed::Super => "SuperSpeed (5Gbps)",
        Speed::SuperPlus => "SuperSpeed+ (10Gbps)",
        Speed::SuperPlusX2 => "SuperSpeed++ (20Gbps)",
        Speed::Unknown => "Unknown speed",
    }
}

// ---------------------------------------------------------------------------
// Standard descriptors

fn dump_device(dev: &Device, handle: Option<&Handle>, d: &DeviceDescriptor, node: &mut Node) {
    // Prefer sysfs for the strings (it doesn't need to open the device);
    // otherwise ask the device.
    let sysfs_name = sysfs::available()
        .then(|| sysfs::get_sysfs_name(dev))
        .flatten();
    let string = |prop: &str, idx: u8| match &sysfs_name {
        Some(name) => sysfs::read_sysfs_prop(name, prop),
        None => get_dev_string(handle, idx),
    };

    let s = node.section("Device Descriptor");
    s.field("bLength", d.b_length);
    s.field("bDescriptorType", d.b_descriptor_type);
    s.field("bcdUSB", bcd(d.bcd_usb));
    s.field("bDeviceClass", d.b_device_class)
        .note(names_class(d.b_device_class).unwrap_or(""));
    s.field("bDeviceSubClass", d.b_device_sub_class)
        .note(names_subclass(d.b_device_class, d.b_device_sub_class).unwrap_or(""));
    s.field("bDeviceProtocol", d.b_device_protocol).note(
        names_protocol(d.b_device_class, d.b_device_sub_class, d.b_device_protocol).unwrap_or(""),
    );
    s.field("bMaxPacketSize0", d.b_max_packet_size0);
    s.field("idVendor", hex4(d.id_vendor))
        .note(names_vendor(d.id_vendor).unwrap_or(""));
    s.field("idProduct", hex4(d.id_product))
        .note(names_product(d.id_vendor, d.id_product).unwrap_or(""));
    s.field("bcdDevice", bcd(d.bcd_device));
    s.field("iManufacturer", d.i_manufacturer)
        .note(string("manufacturer", d.i_manufacturer));
    s.field("iProduct", d.i_product)
        .note(string("product", d.i_product));
    s.field("iSerial", d.i_serial_number)
        .note(string("serial", d.i_serial_number));
    s.field("bNumConfigurations", d.b_num_configurations);
}

fn dump_security(buf: &[u8], node: &mut Node) {
    let s = node.section("Security Descriptor");
    if buf[0] < 5 {
        too_short(s);
        return;
    }
    s.field("bLength", buf[0]);
    s.field("bDescriptorType", buf[1]);
    s.field("wTotalLength", hex4(usb::le16(&buf[2..])));
    s.field("bNumEncryptionTypes", buf[4]);
}

fn dump_encryption_type(buf: &[u8], node: &mut Node) {
    let s = node.section("Encryption Type Descriptor");
    if buf[0] < 5 {
        too_short(s);
        return;
    }
    s.field("bLength", buf[0]);
    s.field("bDescriptorType", buf[1]);
    s.field("bEncryptionType", buf[2])
        .note(*ENCRYPTION_TYPE.get(buf[2] as usize).unwrap_or(&"RESERVED"));
    s.field("bEncryptionValue", buf[3]);
    s.field("bAuthKeyIndex", buf[4]);
}

fn dump_association(dev: Option<&Handle>, buf: &[u8], node: &mut Node) {
    let s = node.section("Interface Association");
    if buf[0] < 8 {
        too_short(s);
        return;
    }
    let func = get_dev_string(dev, buf[7]);
    s.note(join(&[&func, &class_note(buf[4], buf[5], buf[6])]));
    s.field("bLength", buf[0]);
    s.field("bDescriptorType", buf[1]);
    s.field("bFirstInterface", buf[2]);
    s.field("bInterfaceCount", buf[3]);
    s.field("bFunctionClass", buf[4])
        .note(names_class(buf[4]).unwrap_or(""));
    s.field("bFunctionSubClass", buf[5])
        .note(names_subclass(buf[4], buf[5]).unwrap_or(""));
    s.field("bFunctionProtocol", buf[6])
        .note(names_protocol(buf[4], buf[5], buf[6]).unwrap_or(""));
    s.field("iFunction", buf[7]).note(func);
}

fn dump_config(dev: Option<&Handle>, config: &ConfigDescriptor, speed: u16, node: &mut Node) {
    let cfg = get_dev_string(dev, config.i_configuration);
    let max_power = config.max_power as u32 * if speed >= 0x0300 { 8 } else { 2 };
    let powered = if config.bm_attributes & 0x40 != 0 {
        "Self Powered"
    } else {
        "Bus Powered"
    };

    let s = node
        .section(format!("Configuration {}", config.b_configuration_value))
        .note(join(&[&cfg, &format!("{max_power}mA"), powered]));
    s.field("bLength", config.b_length);
    s.field("bDescriptorType", config.b_descriptor_type);
    s.field("wTotalLength", hex4(config.w_total_length));
    s.field("bNumInterfaces", config.b_num_interfaces);
    s.field("bConfigurationValue", config.b_configuration_value);
    s.field("iConfiguration", config.i_configuration).note(cfg);
    let attr = s.field("bmAttributes", hex2(config.bm_attributes));
    if config.bm_attributes & 0x80 == 0 {
        attr.warn("Missing must-be-set bit");
    }
    attr.text(powered);
    if config.bm_attributes & 0x20 != 0 {
        attr.text("Remote Wakeup");
    }
    if config.bm_attributes & 0x10 != 0 {
        attr.text("Battery Powered");
    }
    s.field("MaxPower", format!("{max_power}mA"));

    for_each_extra(&config.extra, s, |buf, s| match buf[1] {
        USB_DT_OTG => {} // shown separately
        USB_DT_INTERFACE_ASSOCIATION => dump_association(dev, buf, s),
        USB_DT_SECURITY => dump_security(buf, s),
        USB_DT_ENCRYPTION_TYPE => dump_encryption_type(buf, s),
        _ => unrecognized(buf, s), // often a misplaced class descriptor
    });

    for alt in config.interfaces.iter().flat_map(|i| &i.altsettings) {
        dump_altsetting(dev, alt, s);
    }
}

/// Decode one class-specific (or otherwise unknown) descriptor found in an
/// interface's extra data.  Returns false if it wasn't recognized.
fn dump_altsetting_extra(
    dev: Option<&Handle>,
    interface: &InterfaceDescriptor,
    buf: &[u8],
    node: &mut Node,
) -> bool {
    let class = interface.b_interface_class;
    let subclass = interface.b_interface_sub_class;
    let protocol = interface.b_interface_protocol;

    match buf[1] {
        // The polite way to provide class specific descriptors: explicitly
        // tagged, using common class spec conventions.
        USB_DT_CS_DEVICE | USB_DT_CS_INTERFACE => match class {
            usb::CLASS_AUDIO => match subclass {
                1 => audio::dump_audiocontrol_interface(dev, buf, protocol, node),
                2 => audio::dump_audiostreaming_interface(dev, buf, protocol, node),
                3 => audio::dump_midistreaming_interface(dev, buf, node),
                _ => return false,
            },
            usb::CLASS_COMM => class::dump_comm_descriptor(dev, buf, node),
            USB_CLASS_VIDEO => match subclass {
                1 => video::dump_videocontrol_interface(dev, buf, protocol, node),
                2 => video::dump_videostreaming_interface(buf, node),
                _ => return false,
            },
            USB_CLASS_APPLICATION => match subclass {
                1 => class::dump_dfu_interface(buf, node),
                _ => return false,
            },
            usb::CLASS_HID => hid::dump_hid_device(dev, interface, buf, node),
            usb::CLASS_PRINTER => class::dump_printer_device(dev, interface, buf, node),
            USB_CLASS_CCID => class::dump_ccid_device(buf, node),
            _ => return false,
        },

        // The ugly way: implicitly tagged, each class could redefine the
        // type IDs.
        _ => match class {
            usb::CLASS_HID => hid::dump_hid_device(dev, interface, buf, node),
            USB_CLASS_CCID => class::dump_ccid_device(buf, node),
            0xe0 => match (subclass, protocol) {
                // wireless
                (1, 2) => class::dump_rc_interface(buf, node),
                (2, _) => class::dump_wire_adapter(buf, node),
                _ => return false,
            },
            usb::CLASS_AUDIO => match (buf[1], subclass) {
                // Misplaced descriptor
                (USB_DT_CS_ENDPOINT, 2) => {
                    audio::dump_audiostreaming_endpoint(dev, buf, protocol, node)
                }
                _ => return false,
            },
            // ... not everything is class-specific
            _ => match buf[1] {
                USB_DT_OTG => {} // shown separately
                USB_DT_INTERFACE_ASSOCIATION => dump_association(dev, buf, node),
                _ => return false,
            },
        },
    }
    true
}

fn dump_altsetting(dev: Option<&Handle>, interface: &InterfaceDescriptor, node: &mut Node) {
    let class = interface.b_interface_class;
    let subclass = interface.b_interface_sub_class;
    let protocol = interface.b_interface_protocol;
    let ifstr = get_dev_string(dev, interface.i_interface);

    let mut title = format!("Interface {}", interface.b_interface_number);
    if interface.b_alternate_setting != 0 {
        title += &format!(" alt {}", interface.b_alternate_setting);
    }
    let s = node
        .section(title)
        .note(join(&[&class_note(class, subclass, protocol), &ifstr]));
    s.field("bLength", interface.b_length);
    s.field("bDescriptorType", interface.b_descriptor_type);
    s.field("bInterfaceNumber", interface.b_interface_number);
    s.field("bAlternateSetting", interface.b_alternate_setting);
    s.field("bNumEndpoints", interface.b_num_endpoints);
    s.field("bInterfaceClass", class)
        .note(names_class(class).unwrap_or(""));
    s.field("bInterfaceSubClass", subclass)
        .note(names_subclass(class, subclass).unwrap_or(""));
    s.field("bInterfaceProtocol", protocol)
        .note(names_protocol(class, subclass, protocol).unwrap_or(""));
    s.field("iInterface", interface.i_interface).note(ifstr);

    for_each_extra(&interface.extra, s, |buf, s| {
        if !dump_altsetting_extra(dev, interface, buf, s) {
            unrecognized(buf, s); // often a misplaced class descriptor
        }
    });

    for endpoint in &interface.endpoints {
        dump_endpoint(dev, interface, endpoint, s);
    }
}

fn dump_pipe_desc(buf: &[u8], node: &mut Node) {
    if buf[0] == 4 && buf[1] == 0x24 {
        let pipe_name = match buf[2] {
            1 => "Command pipe",
            2 => "Status pipe",
            3 => "Data-in pipe",
            4 => "Data-out pipe",
            0xe0..=0xef => "Vendor specific",
            _ => "Reserved",
        };
        node.field("Pipe", hex2(buf[2])).note(pipe_name);
    } else {
        unrecognized(buf, node);
    }
}

fn dump_endpoint(
    dev: Option<&Handle>,
    interface: &InterfaceDescriptor,
    endpoint: &EndpointDescriptor,
    node: &mut Node,
) {
    const TYPEATTR: [&str; 4] = ["Control", "Isochronous", "Bulk", "Interrupt"];
    const SYNCATTR: [&str; 4] = ["None", "Asynchronous", "Adaptive", "Synchronous"];
    const USAGE: [&str; 4] = ["Data", "Feedback", "Implicit feedback Data", "(reserved)"];

    let addr = endpoint.b_endpoint_address;
    let attr = endpoint.bm_attributes as usize;
    let wmax = endpoint.w_max_packet_size as usize;
    let transfer = TYPEATTR[attr & 3];
    // Bits 12:11 are the number of additional transactions per microframe.
    let mult = (wmax >> 11) & 3;
    let packet = if mult == 0 {
        format!("{} bytes", wmax & 0x7ff)
    } else {
        format!("{}x {} bytes", mult + 1, wmax & 0x7ff)
    };

    let s = node
        .section(format!("Endpoint {} {}", addr & 0x0f, ep_dir(addr)))
        .note(join(&[transfer, &packet]));
    s.field("bLength", endpoint.b_length);
    s.field("bDescriptorType", endpoint.b_descriptor_type);
    s.field("bEndpointAddress", hex2(addr))
        .note(format!("EP {} {}", addr & 0x0f, ep_dir(addr)));
    s.field("bmAttributes", endpoint.bm_attributes).note(join(&[
        transfer,
        &format!("sync {}", SYNCATTR[(attr >> 2) & 3]),
        USAGE[(attr >> 4) & 3],
    ]));
    s.field("wMaxPacketSize", hex4(wmax as u16)).note(packet);
    s.field("bInterval", endpoint.b_interval);
    // Only for audio endpoints.
    if endpoint.b_length == 9 {
        s.field("bRefresh", endpoint.b_refresh);
        s.field("bSynchAddress", endpoint.b_synch_address);
    }

    let class = interface.b_interface_class;
    let subclass = interface.b_interface_sub_class;

    for_each_extra(&endpoint.extra, s, |buf, s| match buf[1] {
        USB_DT_CS_ENDPOINT => {
            if class == 1 && subclass == 2 {
                audio::dump_audiostreaming_endpoint(dev, buf, interface.b_interface_protocol, s);
            } else if class == 1 && subclass == 3 {
                audio::dump_midistreaming_endpoint(dev, buf, s);
            } else if class == 14 && subclass == 1 {
                video::dump_videocontrol_interrupt_endpoint(buf, s);
            } else {
                unrecognized(buf, s);
            }
        }
        // Misplaced descriptors
        USB_DT_CS_INTERFACE => match class {
            usb::CLASS_COMM | usb::CLASS_DATA => class::dump_comm_descriptor(dev, buf, s),
            usb::CLASS_MASS_STORAGE => dump_pipe_desc(buf, s),
            _ => unrecognized(buf, s),
        },
        USB_DT_CS_DEVICE => match class {
            USB_CLASS_CCID => class::dump_ccid_device(buf, s),
            _ => unrecognized(buf, s),
        },
        USB_DT_OTG => {} // shown separately
        USB_DT_INTERFACE_ASSOCIATION => dump_association(dev, buf, s),
        USB_DT_SS_ENDPOINT_COMP => {
            let c = s.section("SuperSpeed Endpoint Companion");
            if buf[0] < 6 {
                too_short(c);
                return;
            }
            c.field("bMaxBurst", buf[2]);
            // Bulk streams or isoc "Mult".
            if attr & 3 == 2 && buf[3] & 0x1f != 0 {
                c.field("MaxStreams", 1u32 << (buf[3] & 0x1f));
            }
            if attr & 3 == 1 && buf[3] & 0x3 != 0 {
                c.field("Mult", buf[3] & 0x3);
            }
            if attr & 3 == 1 || attr & 3 == 3 {
                c.field("wBytesPerInterval", usb::le16(&buf[4..]));
            }
        }
        _ => unrecognized(buf, s), // often a misplaced class descriptor
    });
}

// ---------------------------------------------------------------------------
// Hubs

pub(crate) fn dump_hub(p: &[u8], tt_type: u8, node: &mut Node) {
    let w_hub_char = (p[4] as u32) << 8 | p[3] as u32;

    let s = node.section("Hub Descriptor");
    s.field("bLength", p[0]);
    s.field("bDescriptorType", p[1]);
    s.field("nNbrPorts", p[2]);
    let c = s.field("wHubCharacteristic", hex4(w_hub_char as u16));
    c.text(match w_hub_char & 0x03 {
        0 => "Ganged power switching",
        1 => "Per-port power switching",
        _ => "No power switching (usb 1.0)",
    });
    if w_hub_char & 0x04 != 0 {
        c.text("Compound device");
    }
    c.text(match (w_hub_char >> 3) & 0x03 {
        0 => "Ganged overcurrent protection",
        1 => "Per-port overcurrent protection",
        _ => "No overcurrent protection",
    });
    // USB 3.0 hubs don't have TTs or port indicators.
    if (1..3).contains(&tt_type) {
        let l = (w_hub_char >> 5) & 0x03;
        c.text(format!("TT think time {} FS bits", (l + 1) * 8));
    }
    if tt_type != 3 && w_hub_char & (1 << 7) != 0 {
        c.text("Port indicators");
    }
    s.field("bPwrOn2PwrGood", format!("{}ms", p[5] as u32 * 2));

    // USB 3.0 hubs report current in units of aCurrentUnit, or 4 mA.
    let current = if tt_type == 3 {
        p[6] as u32 * 4
    } else {
        p[6] as u32
    };
    s.field("bHubContrCurrent", format!("{current}mA"));

    let offset = if tt_type == 3 {
        s.field("bHubDecLat", format!("0.{}µs", p[7]));
        s.field(
            "wHubDelay",
            format!("{}ns", (p[9] as u32) << 8 | p[8] as u32),
        );
        10
    } else {
        7
    };

    // This determines the variable number of bytes following.
    let l = ((p[2] as usize >> 3) + 1).min(HUB_STATUS_BYTELEN);
    s.field("DeviceRemovable", hex_bytes(&p[offset..offset + l]));
    if tt_type != 3 {
        s.field("PortPwrCtrlMask", hex_bytes(&p[offset + l..offset + 2 * l]));
    }
}

/// Names of the set bits in a port status (`flags` lists byte, mask, name).
fn port_flags(status: &[u8], flags: &[(usize, u8, &'static str)]) -> Vec<&'static str> {
    flags
        .iter()
        .filter(|(byte, mask, _)| status[*byte] & mask != 0)
        .map(|(_, _, name)| *name)
        .collect()
}

fn do_hub(fd: &Handle, tt_type: u8, speed: u16, has_ssp: bool, node: &mut Node) {
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
    // CAPS are used to highlight "transient" states.
    const USB2_FLAGS: &[(usize, u8, &str)] = &[
        (2, 0x10, "C_RESET"),
        (2, 0x08, "C_OC"),
        (2, 0x04, "C_SUSPEND"),
        (2, 0x02, "C_ENABLE"),
        (2, 0x01, "C_CONNECT"),
        (1, 0x10, "indicator"),
        (1, 0x08, "test"),
        (1, 0x04, "highspeed"),
        (1, 0x02, "lowspeed"),
        (1, 0x01, "power"),
        (0, 0x20, "L1"),
        (0, 0x10, "RESET"),
        (0, 0x08, "oc"),
        (0, 0x04, "suspend"),
        (0, 0x02, "enable"),
        (0, 0x01, "connect"),
    ];
    const USB3_CHANGE_FLAGS: &[(usize, u8, &str)] = &[
        (2, 0x80, "C_CONFIG_ERROR"),
        (2, 0x40, "C_LINK_STATE"),
        (2, 0x20, "C_BH_RESET"),
        (2, 0x10, "C_RESET"),
        (2, 0x08, "C_OC"),
        (2, 0x01, "C_CONNECT"),
    ];
    const USB3_STATE_FLAGS: &[(usize, u8, &str)] = &[
        (0, 0x10, "RESET"),
        (0, 0x08, "oc"),
        (0, 0x02, "enable"),
        (0, 0x01, "connect"),
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
            node.warn(format!("Can't get hub descriptor: {e}"));
            return;
        }
    };
    if ret < 9 {
        // at least one port's bitmasks
        node.warn(format!("Incomplete hub descriptor ({ret} bytes)"));
        return;
    }
    dump_hub(&buf, tt_type, node);

    let s = node.section("Hub Port Status");
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
            s.warn(format!("Can't read port {} status: {e}", i + 1));
            break;
        }

        let flags = if speed < 0x0300 {
            port_flags(&status, USB2_FLAGS)
        } else {
            let mut flags = port_flags(&status, USB3_CHANGE_FLAGS);
            flags.push(if status[1] & 0x1c == 0 {
                "5Gbps"
            } else {
                "Unknown Speed"
            });
            if status[1] & 0x02 != 0 {
                flags.push("power");
            }
            // Link state is bits 8:5.
            let link_state = (((status[0] & 0xe0) >> 5) + ((status[1] & 0x1) << 3)) as usize;
            if let Some(state) = LINK_STATE_DESCRIPTIONS.get(link_state) {
                flags.push(state);
            }
            flags.extend(port_flags(&status, USB3_STATE_FLAGS));
            flags
        };

        let port = s
            .field(
                format!("Port {}", i + 1),
                format!(
                    "{:02x}{:02x}.{:02x}{:02x}",
                    status[3], status[2], status[1], status[0]
                ),
            )
            .note(flags.join(" "));

        if is_ext_status && status[0] & 0x01 != 0 {
            port.text(format!(
                "Ext Status {:02x}{:02x}.{:02x}{:02x}",
                status[7], status[6], status[5], status[4]
            ));
            port.text(format!(
                "RX Speed Attribute ID {}, {} lanes",
                status[4] & 0x0f,
                (status[5] & 0x0f) + 1
            ));
            port.text(format!(
                "TX Speed Attribute ID {}, {} lanes",
                (status[4] >> 4) & 0x0f,
                ((status[5] >> 4) & 0x0f) + 1
            ));
        }
    }
}

// ---------------------------------------------------------------------------
// Other device-level descriptors

/// Fetch a standard device descriptor.  A stall just means the device
/// doesn't have one; other errors become a warning.
fn get_descriptor(fd: &Handle, ty: u8, buf: &mut [u8], what: &str, node: &mut Node) -> bool {
    let ret = control_in(
        fd,
        usb::REQUEST_TYPE_STANDARD | usb::RECIPIENT_DEVICE,
        usb::REQUEST_GET_DESCRIPTOR,
        (ty as u16) << 8,
        0,
        buf,
    );
    match ret {
        Ok(n) => n == buf.len() && buf[0] as usize == buf.len() && buf[1] == ty,
        Err(rusb::Error::Pipe) => false,
        Err(e) => {
            node.warn(format!("Can't get {what}: {e}"));
            false
        }
    }
}

fn do_dualspeed(fd: &Handle, node: &mut Node) {
    // All dual-speed devices have a qualifier.
    let mut buf = [0u8; 10];
    if !get_descriptor(
        fd,
        USB_DT_DEVICE_QUALIFIER,
        &mut buf,
        "device qualifier",
        node,
    ) {
        return;
    }

    let s = node
        .section("Device Qualifier")
        .note("for the other device speed");
    s.field("bLength", buf[0]);
    s.field("bDescriptorType", buf[1]);
    s.field("bcdUSB", bcd(usb::le16(&buf[2..]) as u16));
    s.field("bDeviceClass", buf[4])
        .note(names_class(buf[4]).unwrap_or(""));
    s.field("bDeviceSubClass", buf[5])
        .note(names_subclass(buf[4], buf[5]).unwrap_or(""));
    s.field("bDeviceProtocol", buf[6])
        .note(names_protocol(buf[4], buf[5], buf[6]).unwrap_or(""));
    s.field("bMaxPacketSize0", buf[7]);
    s.field("bNumConfigurations", buf[8]);
}

fn do_debug(fd: &Handle, node: &mut Node) {
    // Some high speed devices are also "USB2 debug devices", usable with
    // some EHCI implementations as a system debug channel.
    let mut buf = [0u8; 4];
    if !get_descriptor(fd, USB_DT_DEBUG, &mut buf, "debug descriptor", node) {
        return;
    }

    let s = node.section("Debug Descriptor");
    s.field("bLength", buf[0]);
    s.field("bDescriptorType", buf[1]);
    s.field("bDebugInEndpoint", hex2(buf[2]));
    s.field("bDebugOutEndpoint", hex2(buf[3]));
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

/// Show the OTG descriptor (each config of an OTG device has one).
fn do_otg(config: &ConfigDescriptor, node: &mut Node) -> bool {
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

    let s = node.section("OTG Descriptor");
    s.field("bLength", desc[0]);
    s.field("bDescriptorType", desc[1]);
    let a = s.field("bmAttributes", hex2(desc[2]));
    if desc[2] & 0x01 != 0 {
        a.text("SRP (Session Request Protocol)");
    }
    if desc[2] & 0x02 != 0 {
        a.text("HNP (Host Negotiation Protocol)");
    }
    true
}

fn dump_device_status(fd: &Handle, otg: bool, super_speed: bool, node: &mut Node) {
    let mut status = [0u8; 2];
    if let Err(e) = control_in(
        fd,
        usb::REQUEST_TYPE_STANDARD | usb::RECIPIENT_DEVICE,
        usb::REQUEST_GET_STATUS,
        0,
        0,
        &mut status,
    ) {
        node.warn(format!("Can't read device status: {e}"));
        return;
    }

    let s = node
        .section("Device Status")
        .value(hex4(usb::le16(&status) as u16));
    s.text(if status[0] & (1 << 0) != 0 {
        "Self Powered"
    } else {
        "Bus Powered"
    });
    if status[0] & (1 << 1) != 0 {
        s.text("Remote Wakeup Enabled");
    }
    if super_speed {
        if status[0] & (1 << 2) != 0 {
            s.text("U1 Enabled");
        }
        if status[0] & (1 << 3) != 0 {
            s.text("U2 Enabled");
        }
        if status[0] & (1 << 4) != 0 {
            s.text("Latency Tolerance Messaging (LTM) Enabled");
        }
    }
    // If both host and device support OTG.
    if otg {
        if status[0] & (1 << 3) != 0 {
            s.text("HNP Enabled");
        }
        if status[0] & (1 << 4) != 0 {
            s.text("HNP Capable");
        }
        if status[0] & (1 << 5) != 0 {
            s.text("ALT port is HNP Capable");
        }
    }
    // For high speed devices with debug descriptors.
    if status[0] & (1 << 6) != 0 {
        s.text("Debug Mode");
    }
}

/// Decode everything exausb knows about a device (`exausb -v`).
pub fn dumpdev(dev: &Device) -> Node {
    let desc = usb::device_descriptor(dev);
    let handle = dev.open().ok();
    let udev = handle.as_ref();
    let (vendor, product) = get_vendor_product_with_fallback(dev, udev);

    let mut node = Node::new(format!(
        "Bus {:03} Device {:03}  {vendor} {product}",
        dev.bus_number(),
        dev.address()
    ));
    node.value(format!("{:04x}:{:04x}", desc.id_vendor, desc.id_product))
        .note(speed_name(usb::device_speed(dev)));
    if udev.is_none() {
        node.warn("Couldn't open the device; some information is missing");
    }

    dump_device(dev, udev, &desc, &mut node);

    let mut otg = false;
    for i in 0..desc.b_num_configurations {
        match usb::config_descriptor(dev, i) {
            Ok(config) => {
                if i == 0 {
                    otg = do_otg(&config, &mut node);
                }
                dump_config(udev, &config, desc.bcd_usb, &mut node);
            }
            Err(e) => {
                node.warn(format!("Couldn't get configuration descriptor {i}: {e}"));
            }
        }
    }

    let Some(udev) = udev else {
        return node;
    };
    let mut has_ssp = false;
    if desc.bcd_usb >= 0x0201 {
        bos::dump_bos_descriptor(udev, &mut has_ssp, desc.bcd_usb >= 0x0210, &mut node);
    }
    if desc.b_device_class == usb::CLASS_HUB {
        do_hub(
            udev,
            desc.b_device_protocol,
            desc.bcd_usb,
            has_ssp,
            &mut node,
        );
    }
    if desc.bcd_usb == 0x0200 {
        do_dualspeed(udev, &mut node);
    }
    do_debug(udev, &mut node);
    dump_device_status(udev, otg, desc.bcd_usb >= 0x0300, &mut node);
    node
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

    #[test]
    fn hub_descriptor() {
        let mut node = Node::new("dev");
        let buf = usb::padded(&[9, 0x29, 4, 0x09, 0x00, 0x32, 0x64, 0x00, 0xff]);
        dump_hub(&buf, 1, &mut node);
        let hub = node.get("Hub Descriptor").unwrap();
        assert_eq!(hub.get("nNbrPorts").unwrap().value, "4");
        let chars = hub.get("wHubCharacteristic").unwrap();
        let texts: Vec<_> = chars.children.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(
            texts,
            [
                "Per-port power switching",
                "Per-port overcurrent protection",
                "TT think time 8 FS bits"
            ]
        );
        assert_eq!(hub.get("bPwrOn2PwrGood").unwrap().value, "100ms");
        assert_eq!(hub.get("PortPwrCtrlMask").unwrap().value, "ff");
    }

    /// Deterministic xorshift PRNG, so failures are reproducible.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        fn below(&mut self, n: u64) -> u8 {
            (self.next() % n) as u8
        }
        /// A byte biased towards small values, so counts and lengths often
        /// line up and the deeper decoding paths get exercised.
        fn byte(&mut self) -> u8 {
            match self.below(4) {
                0 | 1 => self.below(4),
                2 => self.below(16),
                _ => self.next() as u8,
            }
        }
        /// A run of descriptors with the given types, usually well formed.
        fn descriptors(&mut self, types: &[u8]) -> Vec<u8> {
            let mut out = Vec::new();
            for _ in 0..self.below(4) {
                let len = 2 + self.below(60);
                let mut d = vec![len, types[self.below(types.len() as u64) as usize]];
                d.extend((2..len).map(|_| self.byte()));
                out.extend(d);
            }
            if self.below(10) == 0 {
                out.push(self.byte()); // a truncated trailing descriptor
            }
            out
        }
    }

    /// Random descriptors for every class must decode without panicking.
    #[test]
    fn fuzz_no_panics() {
        const CLASSES: [(u8, u8, u8); 14] = [
            (1, 1, 0x00),
            (1, 1, 0x20),
            (1, 1, 0x30),
            (1, 2, 0x00),
            (1, 2, 0x20),
            (1, 3, 0),
            (0x0e, 1, 1),
            (0x0e, 2, 0),
            (2, 2, 0),
            (3, 1, 1),
            (7, 1, 4),
            (0x0b, 0, 0),
            (0xfe, 1, 1),
            (0xe0, 2, 2),
        ];
        const TYPES: [u8; 9] = [0x24, 0x25, 0x21, 0x30, 0x0b, 0x09, 0x0c, 0x0e, 0x41];

        let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
        for _ in 0..3000 {
            let interfaces = (0..1 + rng.below(3))
                .map(|n| {
                    let (class, sub, proto) = CLASSES[rng.below(CLASSES.len() as u64) as usize];
                    let endpoints: Vec<EndpointDescriptor> = (0..rng.below(3))
                        .map(|_| EndpointDescriptor {
                            b_length: if rng.below(2) == 0 { 7 } else { 9 },
                            b_descriptor_type: 5,
                            b_endpoint_address: rng.next() as u8,
                            bm_attributes: rng.next() as u8,
                            w_max_packet_size: rng.next() as u16,
                            b_interval: rng.byte(),
                            b_refresh: 0,
                            b_synch_address: 0,
                            extra: rng.descriptors(&TYPES),
                        })
                        .collect();
                    usb::Interface {
                        altsettings: vec![InterfaceDescriptor {
                            b_length: 9,
                            b_descriptor_type: 4,
                            b_interface_number: n,
                            b_alternate_setting: 0,
                            b_num_endpoints: endpoints.len() as u8,
                            b_interface_class: class,
                            b_interface_sub_class: sub,
                            b_interface_protocol: proto,
                            i_interface: 0,
                            endpoints,
                            extra: rng.descriptors(&TYPES),
                        }],
                    }
                })
                .collect::<Vec<_>>();
            let config = ConfigDescriptor {
                b_length: 9,
                b_descriptor_type: 2,
                w_total_length: 0,
                b_num_interfaces: interfaces.len() as u8,
                b_configuration_value: 1,
                i_configuration: 0,
                bm_attributes: rng.next() as u8,
                max_power: rng.next() as u8,
                interfaces,
                extra: rng.descriptors(&TYPES),
            };
            let mut node = Node::new("dev");
            dump_config(None, &config, 0x0200, &mut node);

            let caps = rng.descriptors(&[0x10]);
            bos::dump_device_capabilities(None, &caps, &mut false, rng.below(2) == 0, &mut node);

            let report: Vec<u8> = (0..rng.below(64)).map(|_| rng.byte()).collect();
            hid::dump_report_desc(&report, &mut node);

            let hub: Vec<u8> = (0..13).map(|_| rng.next() as u8).collect();
            dump_hub(&usb::padded(&hub), rng.below(4), &mut node);

            // Rendering must not panic either.
            crate::render::render(&node);
        }
    }
}
