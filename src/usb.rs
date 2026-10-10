// SPDX-License-Identifier: GPL-2.0-or-later
//! Thin layer over libusb.
//!
//! rusb's safe descriptor types decode (and partly hide) the raw descriptor
//! fields, but exausb wants to print them verbatim.  This module copies the
//! raw libusb descriptor structures into owned Rust values and provides a
//! few helpers for control transfers.

use std::slice;
use std::time::Duration;

use rusb::ffi;

pub type Context = rusb::Context;
pub type Device = rusb::Device<Context>;
pub type Handle = rusb::DeviceHandle<Context>;

/// Timeout used for control transfers, like `CTRL_TIMEOUT` in lsusb.c.
pub const CTRL_TIMEOUT: Duration = Duration::from_millis(5 * 1000);

// Request types / recipients / standard requests.
pub const ENDPOINT_IN: u8 = 0x80;
pub const REQUEST_TYPE_STANDARD: u8 = 0x00;
pub const REQUEST_TYPE_CLASS: u8 = 0x01 << 5;
pub const REQUEST_TYPE_VENDOR: u8 = 0x02 << 5;
pub const RECIPIENT_DEVICE: u8 = 0x00;
pub const RECIPIENT_INTERFACE: u8 = 0x01;
pub const RECIPIENT_OTHER: u8 = 0x03;
pub const REQUEST_GET_STATUS: u8 = 0x00;
pub const REQUEST_GET_DESCRIPTOR: u8 = 0x06;

// Standard descriptor types.
pub const DT_DEVICE: u8 = 0x01;
pub const DT_STRING: u8 = 0x03;
pub const DT_INTERFACE: u8 = 0x04;
pub const DT_ENDPOINT: u8 = 0x05;
pub const DT_HID: u8 = 0x21;
pub const DT_REPORT: u8 = 0x22;

// Class codes.
pub const CLASS_AUDIO: u8 = 0x01;
pub const CLASS_COMM: u8 = 0x02;
pub const CLASS_HID: u8 = 0x03;
pub const CLASS_PRINTER: u8 = 0x07;
pub const CLASS_MASS_STORAGE: u8 = 0x08;
pub const CLASS_HUB: u8 = 0x09;
pub const CLASS_DATA: u8 = 0x0a;

/// Negotiated device speed, as reported by `libusb_get_device_speed()`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Speed {
    Unknown,
    Low,
    Full,
    High,
    Super,
    SuperPlus,
    SuperPlusX2,
}

pub fn device_speed(dev: &Device) -> Speed {
    // libusb >= 1.0.27 knows about LIBUSB_SPEED_SUPER_PLUS_X2 (6), which
    // rusb's `Speed` enum does not.
    match unsafe { ffi::libusb_get_device_speed(dev.as_raw()) } {
        1 => Speed::Low,
        2 => Speed::Full,
        3 => Speed::High,
        4 => Speed::Super,
        5 => Speed::SuperPlus,
        6 => Speed::SuperPlusX2,
        _ => Speed::Unknown,
    }
}

/// Raw device descriptor.
#[derive(Clone, Debug)]
pub struct DeviceDescriptor {
    pub b_length: u8,
    pub b_descriptor_type: u8,
    pub bcd_usb: u16,
    pub b_device_class: u8,
    pub b_device_sub_class: u8,
    pub b_device_protocol: u8,
    pub b_max_packet_size0: u8,
    pub id_vendor: u16,
    pub id_product: u16,
    pub bcd_device: u16,
    pub i_manufacturer: u8,
    pub i_product: u8,
    pub i_serial_number: u8,
    pub b_num_configurations: u8,
}

pub fn device_descriptor(dev: &Device) -> DeviceDescriptor {
    let mut d = std::mem::MaybeUninit::<ffi::libusb_device_descriptor>::zeroed();
    // libusb_get_device_descriptor() cannot fail since libusb 1.0.16.
    let d = unsafe {
        ffi::libusb_get_device_descriptor(dev.as_raw(), d.as_mut_ptr());
        d.assume_init()
    };
    DeviceDescriptor {
        b_length: d.bLength,
        b_descriptor_type: d.bDescriptorType,
        bcd_usb: d.bcdUSB,
        b_device_class: d.bDeviceClass,
        b_device_sub_class: d.bDeviceSubClass,
        b_device_protocol: d.bDeviceProtocol,
        b_max_packet_size0: d.bMaxPacketSize0,
        id_vendor: d.idVendor,
        id_product: d.idProduct,
        bcd_device: d.bcdDevice,
        i_manufacturer: d.iManufacturer,
        i_product: d.iProduct,
        i_serial_number: d.iSerialNumber,
        b_num_configurations: d.bNumConfigurations,
    }
}

/// Raw configuration descriptor, including all interfaces and endpoints.
#[derive(Clone, Debug)]
pub struct ConfigDescriptor {
    pub b_length: u8,
    pub b_descriptor_type: u8,
    pub w_total_length: u16,
    pub b_num_interfaces: u8,
    pub b_configuration_value: u8,
    pub i_configuration: u8,
    pub bm_attributes: u8,
    pub max_power: u8,
    pub interfaces: Vec<Interface>,
    pub extra: Vec<u8>,
}

/// All alternate settings of one interface.
#[derive(Clone, Debug)]
pub struct Interface {
    pub altsettings: Vec<InterfaceDescriptor>,
}

#[derive(Clone, Debug)]
pub struct InterfaceDescriptor {
    pub b_length: u8,
    pub b_descriptor_type: u8,
    pub b_interface_number: u8,
    pub b_alternate_setting: u8,
    pub b_num_endpoints: u8,
    pub b_interface_class: u8,
    pub b_interface_sub_class: u8,
    pub b_interface_protocol: u8,
    pub i_interface: u8,
    pub endpoints: Vec<EndpointDescriptor>,
    pub extra: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct EndpointDescriptor {
    pub b_length: u8,
    pub b_descriptor_type: u8,
    pub b_endpoint_address: u8,
    pub bm_attributes: u8,
    pub w_max_packet_size: u16,
    pub b_interval: u8,
    pub b_refresh: u8,
    pub b_synch_address: u8,
    pub extra: Vec<u8>,
}

unsafe fn raw_slice<'a, T>(ptr: *const T, len: usize) -> &'a [T] {
    if ptr.is_null() || len == 0 {
        &[]
    } else {
        slice::from_raw_parts(ptr, len)
    }
}

unsafe fn extra_bytes(ptr: *const u8, len: i32) -> Vec<u8> {
    raw_slice(ptr, len.max(0) as usize).to_vec()
}

/// Read configuration descriptor `index` (as `libusb_get_config_descriptor()`).
pub fn config_descriptor(dev: &Device, index: u8) -> Result<ConfigDescriptor, rusb::Error> {
    let mut cfg: *const ffi::libusb_config_descriptor = std::ptr::null();
    let ret = unsafe { ffi::libusb_get_config_descriptor(dev.as_raw(), index, &mut cfg) };
    convert_config(ret, cfg)
}

/// Read the descriptor of the currently active configuration.
pub fn active_config_descriptor(dev: &Device) -> Result<ConfigDescriptor, rusb::Error> {
    let mut cfg: *const ffi::libusb_config_descriptor = std::ptr::null();
    let ret = unsafe { ffi::libusb_get_active_config_descriptor(dev.as_raw(), &mut cfg) };
    convert_config(ret, cfg)
}

/// Copy a libusb config descriptor into an owned value and free it.
fn convert_config(
    ret: i32,
    cfg: *const ffi::libusb_config_descriptor,
) -> Result<ConfigDescriptor, rusb::Error> {
    if ret != 0 || cfg.is_null() {
        return Err(error_from_code(ret));
    }

    let out = unsafe {
        let c = &*cfg;
        let interfaces = raw_slice(c.interface, c.bNumInterfaces as usize)
            .iter()
            .map(|intf| Interface {
                altsettings: raw_slice(intf.altsetting, intf.num_altsetting.max(0) as usize)
                    .iter()
                    .map(|alt| InterfaceDescriptor {
                        b_length: alt.bLength,
                        b_descriptor_type: alt.bDescriptorType,
                        b_interface_number: alt.bInterfaceNumber,
                        b_alternate_setting: alt.bAlternateSetting,
                        b_num_endpoints: alt.bNumEndpoints,
                        b_interface_class: alt.bInterfaceClass,
                        b_interface_sub_class: alt.bInterfaceSubClass,
                        b_interface_protocol: alt.bInterfaceProtocol,
                        i_interface: alt.iInterface,
                        endpoints: raw_slice(alt.endpoint, alt.bNumEndpoints as usize)
                            .iter()
                            .map(|ep| EndpointDescriptor {
                                b_length: ep.bLength,
                                b_descriptor_type: ep.bDescriptorType,
                                b_endpoint_address: ep.bEndpointAddress,
                                bm_attributes: ep.bmAttributes,
                                w_max_packet_size: ep.wMaxPacketSize,
                                b_interval: ep.bInterval,
                                b_refresh: ep.bRefresh,
                                b_synch_address: ep.bSynchAddress,
                                extra: extra_bytes(ep.extra, ep.extra_length),
                            })
                            .collect(),
                        extra: extra_bytes(alt.extra, alt.extra_length),
                    })
                    .collect(),
            })
            .collect();
        ConfigDescriptor {
            b_length: c.bLength,
            b_descriptor_type: c.bDescriptorType,
            w_total_length: c.wTotalLength,
            b_num_interfaces: c.bNumInterfaces,
            b_configuration_value: c.bConfigurationValue,
            i_configuration: c.iConfiguration,
            bm_attributes: c.bmAttributes,
            max_power: c.bMaxPower,
            interfaces,
            extra: extra_bytes(c.extra, c.extra_length),
        }
    };
    unsafe { ffi::libusb_free_config_descriptor(cfg) };
    Ok(out)
}

fn error_from_code(code: i32) -> rusb::Error {
    use rusb::Error::*;
    match code {
        -1 => Io,
        -2 => InvalidParam,
        -3 => Access,
        -4 => NoDevice,
        -5 => NotFound,
        -6 => Busy,
        -7 => Timeout,
        -8 => Overflow,
        -9 => Pipe,
        -10 => Interrupted,
        -11 => NoMem,
        -12 => NotSupported,
        _ => Other,
    }
}

/// The libusb name of an error, as `libusb_error_name()` would return it.
pub fn error_name(err: rusb::Error) -> &'static str {
    use rusb::Error::*;
    match err {
        Io => "LIBUSB_ERROR_IO",
        InvalidParam => "LIBUSB_ERROR_INVALID_PARAM",
        Access => "LIBUSB_ERROR_ACCESS",
        NoDevice => "LIBUSB_ERROR_NO_DEVICE",
        NotFound => "LIBUSB_ERROR_NOT_FOUND",
        Busy => "LIBUSB_ERROR_BUSY",
        Timeout => "LIBUSB_ERROR_TIMEOUT",
        Overflow => "LIBUSB_ERROR_OVERFLOW",
        Pipe => "LIBUSB_ERROR_PIPE",
        Interrupted => "LIBUSB_ERROR_INTERRUPTED",
        NoMem => "LIBUSB_ERROR_NO_MEM",
        NotSupported => "LIBUSB_ERROR_NOT_SUPPORTED",
        BadDescriptor | Other => "LIBUSB_ERROR_OTHER",
    }
}

/// An IN control transfer (`libusb_control_transfer()` with `CTRL_TIMEOUT`).
pub fn control_in(
    dev: &Handle,
    request_type: u8,
    request: u8,
    value: u16,
    index: u16,
    buf: &mut [u8],
) -> Result<usize, rusb::Error> {
    dev.read_control(
        request_type | ENDPOINT_IN,
        request,
        value,
        index,
        buf,
        CTRL_TIMEOUT,
    )
}

/// Read a little-endian u16 from the start of `buf`.
pub fn le16(buf: &[u8]) -> u32 {
    buf[0] as u32 | (buf[1] as u32) << 8
}

/// Read a little-endian u32 from the start of `buf`.
pub fn le32(buf: &[u8]) -> u32 {
    u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]])
}

/// Copy a descriptor into a zero-padded buffer.
///
/// The C code freely indexes descriptor bytes after checking only `bLength`,
/// and occasionally reads a byte or two past the end.  Dumping from a
/// zero-padded copy keeps those reads in bounds (and deterministic) while
/// preserving the original logic.
pub fn padded(desc: &[u8]) -> Vec<u8> {
    let mut v = vec![0u8; desc.len().max(desc.first().copied().unwrap_or(0) as usize) + 512];
    v[..desc.len()].copy_from_slice(desc);
    v
}
