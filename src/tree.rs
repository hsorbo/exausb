// SPDX-License-Identifier: GPL-2.0-or-later
//! `lsusb -t`: dump the physical USB device hierarchy as a tree.
//!
//! Port of usbutils' `lsusb-t.c` (Copyright 2011 Greg Kroah-Hartman).
//!
//! Upstream builds the tree from Linux sysfs, which also tells it which
//! kernel driver is bound to each interface.  This port does the same when
//! sysfs is available, and otherwise builds the tree from libusb's topology
//! information (bus number and port path).  Drivers aren't known then and
//! are shown as `[unknown]`.

use std::collections::BTreeMap;
use std::fs;

use rusb::UsbContext;

use crate::dump::verblevel;
use crate::names::{get_class_string, get_product_string, get_vendor_string};
use crate::sysfs::{self, sanitize, SYS_BUS_USB_DEVICES};
use crate::usb::{self, Context, Device, Speed};
use crate::usbmisc::get_dev_string;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TreeInterface {
    pub configuration: u32,
    pub ifnum: u32,
    pub class: u8,
    pub driver: String,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TreeDevice {
    /// sysfs-style name, e.g. "1-2.3".
    pub name: String,
    pub busnum: u32,
    pub portnum: u32,
    pub devnum: u32,
    pub id_vendor: u16,
    pub id_product: u16,
    pub maxchild: u32,
    pub speed: String,
    pub rx_lanes: u32,
    pub tx_lanes: u32,
    pub manufacturer: String,
    pub product: String,
    pub serial: String,
    pub interfaces: Vec<TreeInterface>,
    pub children: Vec<TreeDevice>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TreeBus {
    /// sysfs-style name, e.g. "usb1".
    pub name: String,
    pub busnum: u32,
    pub devnum: u32,
    pub device_class: u8,
    pub id_vendor: u16,
    pub id_product: u16,
    pub maxchild: u32,
    pub speed: String,
    pub rx_lanes: u32,
    pub tx_lanes: u32,
    pub driver: String,
    /// No root hub device was reported for this bus (libusb on some
    /// platforms); only the bus number is known.
    pub synthetic: bool,
    pub children: Vec<TreeDevice>,
}

// ---------------------------------------------------------------------------
// Printing

fn bdeviceclass_to_str(dc: u8) -> String {
    match dc {
        9 => "root_hub".to_string(),
        _ => format!("'bDeviceClass 0x{dc:02x} not yet handled'"),
    }
}

fn lanes_to_str(tx: u32, rx: u32) -> String {
    if tx == rx {
        if tx < 2 {
            String::new()
        } else {
            format!("/x{tx}")
        }
    } else {
        format!("/Tx{tx}+Rx{rx}")
    }
}

fn print_usbbusnode(b: &TreeBus) {
    let lanes = lanes_to_str(b.tx_lanes, b.rx_lanes);
    println!(
        "/:  Bus {:03}.Port {:03}: Dev {:03}, Class={}, Driver={}/{}p, {}M{}",
        b.busnum,
        1,
        b.devnum,
        bdeviceclass_to_str(b.device_class),
        b.driver,
        b.maxchild,
        b.speed,
        lanes
    );
    if b.synthetic {
        return;
    }
    if verblevel() >= 1 {
        let vendor = get_vendor_string(b.id_vendor);
        let product = get_product_string(b.id_vendor, b.id_product);
        println!(
            "    ID {:04x}:{:04x} {} {}",
            b.id_vendor, b.id_product, vendor, product
        );
    }
    if verblevel() >= 2 {
        println!(
            "    {}/{}  /dev/bus/usb/{:03}/{:03}",
            SYS_BUS_USB_DEVICES, b.name, b.busnum, b.devnum
        );
    }
}

fn print_usbdevice(d: &TreeDevice, i: Option<&TreeInterface>, indent: usize) {
    let lanes = lanes_to_str(d.tx_lanes, d.rx_lanes);

    match i {
        None => println!(
            "Port {:03}: Dev {:03}, {}M{}",
            d.portnum, d.devnum, d.speed, lanes
        ),
        Some(i) => {
            let subcls = get_class_string(i.class);
            if i.class == 9 {
                println!(
                    "Port {:03}: Dev {:03}, If {}, Class={}, Driver={}/{}p, {}M{}",
                    d.portnum, d.devnum, i.ifnum, subcls, i.driver, d.maxchild, d.speed, lanes
                );
            } else {
                println!(
                    "Port {:03}: Dev {:03}, If {}, Class={}, Driver={}, {}M{}",
                    d.portnum, d.devnum, i.ifnum, subcls, i.driver, d.speed, lanes
                );
            }
        }
    }
    if verblevel() >= 1 {
        print!(" {:>indent$}", "    ");
        let vendor = get_vendor_string(d.id_vendor);
        let product = get_product_string(d.id_vendor, d.id_product);
        println!(
            "ID {:04x}:{:04x} {} {}",
            d.id_vendor, d.id_product, vendor, product
        );
    }
    if verblevel() >= 2 {
        print!(" {:>indent$}", "    ");
        println!(
            "{}/{}  /dev/bus/usb/{:03}/{:03}",
            SYS_BUS_USB_DEVICES, d.name, d.busnum, d.devnum
        );
    }
    if verblevel() >= 3
        && !(d.manufacturer.is_empty() && d.product.is_empty() && d.serial.is_empty())
    {
        print!(" {:>indent$}", "    ");
        if !d.manufacturer.is_empty() {
            print!("Manufacturer={} ", d.manufacturer);
        }
        if !d.product.is_empty() {
            print!("Product={} ", d.product);
        }
        if !d.serial.is_empty() {
            print!("Serial={}", d.serial);
        }
        println!();
    }
}

fn print_tree_dev_interface(d: &TreeDevice, indent: usize) {
    let indent = indent + 3;
    let print_line = |i: Option<&TreeInterface>| {
        print!(" {:>indent$}", "|__ ");
        print_usbdevice(d, i, indent);
    };
    if d.interfaces.is_empty() {
        print_line(None);
    } else {
        for i in &d.interfaces {
            print_line(Some(i));
        }
    }
}

fn print_tree_dev_children(children: &[TreeDevice], indent: usize) {
    let indent = indent + 4;
    for d in children {
        print_tree_dev_interface(d, indent);
        print_tree_dev_children(&d.children, indent);
    }
}

pub fn print_tree(busses: &[TreeBus]) {
    for b in busses {
        print_usbbusnode(b);
        print_tree_dev_children(&b.children, 0);
    }
}

// ---------------------------------------------------------------------------
// Tree assembly (shared by both sources)

/// A device before it's attached to its parent.
struct FlatDevice {
    dev: TreeDevice,
    /// Port path below the root hub.
    ports: Vec<u32>,
}

/// Attach devices to their parents (by port path) and sort everything like
/// upstream: busses by number, siblings by port, interfaces by
/// (configuration, number).  Devices whose parent is missing are dropped,
/// as upstream does.
fn assemble(mut busses: Vec<TreeBus>, devices: Vec<FlatDevice>) -> Vec<TreeBus> {
    let mut by_path: BTreeMap<(u32, Vec<u32>), TreeDevice> = BTreeMap::new();
    for mut d in devices {
        d.dev.interfaces.sort_by_key(|i| (i.configuration, i.ifnum));
        by_path.insert((d.dev.busnum, d.ports), d.dev);
    }

    // Attach the deepest devices first so that subtrees are complete
    // before they're moved into their parent.
    let mut keys: Vec<_> = by_path.keys().cloned().collect();
    keys.sort_by_key(|(_, ports)| std::cmp::Reverse(ports.len()));
    for key in keys {
        let (bus, ports) = &key;
        if ports.len() <= 1 {
            continue;
        }
        let dev = by_path.remove(&key).unwrap();
        let parent_key = (*bus, ports[..ports.len() - 1].to_vec());
        if let Some(parent) = by_path.get_mut(&parent_key) {
            parent.children.push(dev);
        }
    }

    for ((bus, _), dev) in by_path {
        if let Some(b) = busses.iter_mut().find(|b| b.busnum == bus) {
            b.children.push(dev);
        }
    }

    fn sort_children(children: &mut [TreeDevice]) {
        children.sort_by_key(|d| d.portnum);
        for c in children {
            sort_children(&mut c.children);
        }
    }
    for b in &mut busses {
        sort_children(&mut b.children);
    }
    busses.sort_by_key(|b| b.busnum);
    busses
}

// ---------------------------------------------------------------------------
// sysfs source (Linux)

fn read_sysfs_string(root: &str, d_name: &str, file: &str) -> String {
    let path = format!("{root}/{d_name}/{file}");
    match fs::read(path) {
        Ok(bytes) => {
            let s = String::from_utf8_lossy(&bytes);
            // Upstream replaces control characters with spaces here.
            s.trim_end_matches('\n')
                .chars()
                .map(|c| {
                    if (c as u32) < 0x20 || c == '\x7f' {
                        ' '
                    } else {
                        c
                    }
                })
                .collect()
        }
        Err(_) => String::new(),
    }
}

fn read_sysfs_int(root: &str, d_name: &str, file: &str, radix: u32) -> u32 {
    let path = format!("{root}/{d_name}/{file}");
    match fs::read_to_string(&path) {
        Ok(s) => {
            let s = s.trim();
            let digits: String = s.chars().take_while(|c| c.is_digit(radix)).collect();
            u32::from_str_radix(&digits, radix).unwrap_or(0)
        }
        Err(e) => {
            eprintln!("{path}: {e}");
            0
        }
    }
}

fn read_driver(link: &str) -> Option<String> {
    let target = fs::read_link(link).ok()?;
    target.file_name().map(|n| n.to_string_lossy().into_owned())
}

/// Parse a leading decimal number; returns it and the rest of the string.
fn parse_num(s: &str) -> Option<(u32, &str)> {
    let end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    Some((s[..end].parse().ok()?, &s[end..]))
}

/// Build the tree from a sysfs `bus/usb/devices` directory.
fn tree_from_sysfs(root: &str) -> std::io::Result<Vec<TreeBus>> {
    let mut busses = Vec::new();
    let mut devices = Vec::new();
    let mut interfaces: Vec<(String, TreeInterface)> = Vec::new();

    for entry in fs::read_dir(root)? {
        let d_name = entry?.file_name().to_string_lossy().into_owned();
        let first = d_name.chars().next().unwrap_or(' ');

        if let Some(rest) = d_name
            .strip_prefix("usb")
            .filter(|r| r.starts_with(|c: char| c.is_ascii_digit()))
        {
            let speed = read_sysfs_string(root, &d_name, "speed");
            busses.push(TreeBus {
                busnum: parse_num(rest).map_or(0, |(n, _)| n),
                devnum: read_sysfs_int(root, &d_name, "devnum", 10),
                device_class: read_sysfs_int(root, &d_name, "bDeviceClass", 16) as u8,
                id_product: read_sysfs_int(root, &d_name, "idProduct", 16) as u16,
                id_vendor: read_sysfs_int(root, &d_name, "idVendor", 16) as u16,
                maxchild: read_sysfs_int(root, &d_name, "maxchild", 10),
                speed,
                rx_lanes: read_sysfs_int(root, &d_name, "rx_lanes", 10),
                tx_lanes: read_sysfs_int(root, &d_name, "tx_lanes", 10),
                driver: read_driver(&format!("{root}/{d_name}/../driver")).unwrap_or_default(),
                name: d_name,
                synthetic: false,
                children: Vec::new(),
            });
        } else if first.is_ascii_digit() {
            if let Some((dev_name, ifpart)) = d_name.split_once(':') {
                // Interface: "B-P.P:C.I"
                let Some((configuration, rest)) = parse_num(ifpart) else {
                    continue;
                };
                let Some((ifnum, _)) = rest.get(1..).and_then(parse_num) else {
                    continue;
                };
                let driver = read_driver(&format!("{root}/{d_name}/driver"))
                    .unwrap_or_else(|| "[none]".to_string());
                interfaces.push((
                    dev_name.to_string(),
                    TreeInterface {
                        configuration,
                        ifnum,
                        class: read_sysfs_int(root, &d_name, "bInterfaceClass", 16) as u8,
                        driver,
                    },
                ));
            } else {
                // Device: "B-P.P.P"
                let Some((busnum, mut rest)) = parse_num(&d_name) else {
                    continue;
                };
                let mut ports = Vec::new();
                while let Some((p, r)) = rest.get(1..).and_then(parse_num) {
                    ports.push(p);
                    rest = r;
                }
                devices.push(FlatDevice {
                    dev: TreeDevice {
                        busnum,
                        portnum: ports.last().copied().unwrap_or(0),
                        devnum: read_sysfs_int(root, &d_name, "devnum", 10),
                        id_product: read_sysfs_int(root, &d_name, "idProduct", 16) as u16,
                        id_vendor: read_sysfs_int(root, &d_name, "idVendor", 16) as u16,
                        maxchild: read_sysfs_int(root, &d_name, "maxchild", 10),
                        manufacturer: read_sysfs_string(root, &d_name, "manufacturer"),
                        product: read_sysfs_string(root, &d_name, "product"),
                        serial: read_sysfs_string(root, &d_name, "serial"),
                        speed: read_sysfs_string(root, &d_name, "speed"),
                        rx_lanes: read_sysfs_int(root, &d_name, "rx_lanes", 10),
                        tx_lanes: read_sysfs_int(root, &d_name, "tx_lanes", 10),
                        name: d_name,
                        ..Default::default()
                    },
                    ports,
                });
            }
        } else {
            eprintln!("ignoring '{d_name}'");
        }
    }

    // Root hub interfaces ("B-0:C.I") belong to the bus and aren't shown.
    for (dev_name, intf) in interfaces {
        if let Some(d) = devices.iter_mut().find(|d| d.dev.name == dev_name) {
            d.dev.interfaces.push(intf);
        }
    }

    Ok(assemble(busses, devices))
}

// ---------------------------------------------------------------------------
// libusb source (everything else)

fn speed_str(speed: Speed) -> &'static str {
    match speed {
        Speed::Low => "1.5",
        Speed::Full => "12",
        Speed::High => "480",
        Speed::Super => "5000",
        Speed::SuperPlus => "10000",
        Speed::SuperPlusX2 => "20000",
        Speed::Unknown => "unknown",
    }
}

/// Number of downstream ports of a hub, from its hub descriptor.
fn hub_port_count(handle: Option<&usb::Handle>, bcd_usb: u16) -> u32 {
    let Some(handle) = handle else { return 0 };
    let value: u16 = if bcd_usb >= 0x0300 { 0x2a } else { 0x29 };
    let mut buf = [0u8; 13];
    match usb::control_in(
        handle,
        usb::REQUEST_TYPE_CLASS | usb::RECIPIENT_DEVICE,
        usb::REQUEST_GET_DESCRIPTOR,
        value << 8,
        0,
        &mut buf,
    ) {
        Ok(n) if n >= 3 => buf[2] as u32,
        _ => 0,
    }
}

fn tree_from_libusb(ctx: &Context) -> rusb::Result<Vec<TreeBus>> {
    let mut busses: BTreeMap<u32, TreeBus> = BTreeMap::new();
    let mut devices = Vec::new();

    for dev in ctx.devices()?.iter() {
        let desc = usb::device_descriptor(&dev);
        let busnum = dev.bus_number() as u32;
        let ports: Vec<u32> = dev
            .port_numbers()
            .unwrap_or_default()
            .into_iter()
            .map(u32::from)
            .collect();
        let handle = if verblevel() >= 3 || desc.b_device_class == usb::CLASS_HUB {
            dev.open().ok()
        } else {
            None
        };
        let maxchild = if desc.b_device_class == usb::CLASS_HUB {
            hub_port_count(handle.as_ref(), desc.bcd_usb)
        } else {
            0
        };
        let speed = speed_str(usb::device_speed(&dev)).to_string();

        if ports.is_empty() {
            // A root hub (not every libusb backend reports these).
            busses.insert(
                busnum,
                TreeBus {
                    name: format!("usb{busnum}"),
                    busnum,
                    devnum: dev.address() as u32,
                    device_class: desc.b_device_class,
                    id_vendor: desc.id_vendor,
                    id_product: desc.id_product,
                    maxchild,
                    speed,
                    driver: "[unknown]".to_string(),
                    ..Default::default()
                },
            );
            continue;
        }

        let interfaces = active_interfaces(&dev);
        let string = |idx: u8| match get_dev_string(handle.as_ref(), idx) {
            s if s == "(error)" => String::new(),
            s => sanitize(&s),
        };
        devices.push(FlatDevice {
            dev: TreeDevice {
                name: sysfs::sysfs_name(busnum as u8, &dev.port_numbers().unwrap_or_default()),
                busnum,
                portnum: *ports.last().unwrap(),
                devnum: dev.address() as u32,
                id_vendor: desc.id_vendor,
                id_product: desc.id_product,
                maxchild,
                speed,
                manufacturer: string(desc.i_manufacturer),
                product: string(desc.i_product),
                serial: string(desc.i_serial_number),
                interfaces,
                ..Default::default()
            },
            ports,
        });
    }

    // Busses whose root hub libusb doesn't report (e.g. on macOS) still
    // get a node, so their devices are shown.
    for d in &devices {
        busses.entry(d.dev.busnum).or_insert_with(|| TreeBus {
            name: format!("usb{}", d.dev.busnum),
            busnum: d.dev.busnum,
            device_class: usb::CLASS_HUB,
            speed: "unknown".to_string(),
            driver: "[unknown]".to_string(),
            synthetic: true,
            ..Default::default()
        });
    }

    Ok(assemble(busses.into_values().collect(), devices))
}

fn active_interfaces(dev: &Device) -> Vec<TreeInterface> {
    let Ok(config) = usb::active_config_descriptor(dev) else {
        return Vec::new();
    };
    config
        .interfaces
        .iter()
        .filter_map(|intf| intf.altsettings.first())
        .map(|alt| TreeInterface {
            configuration: config.b_configuration_value as u32,
            ifnum: alt.b_interface_number as u32,
            class: alt.b_interface_class,
            driver: "[unknown]".to_string(),
        })
        .collect()
}

/// Run `lsusb -t`.  Returns the process exit status.
pub fn lsusb_t() -> i32 {
    if sysfs::available() {
        match tree_from_sysfs(SYS_BUS_USB_DEVICES) {
            Ok(busses) => {
                print_tree(&busses);
                0
            }
            Err(e) => {
                eprintln!("{SYS_BUS_USB_DEVICES}: {e}");
                1
            }
        }
    } else {
        let tree = Context::new().and_then(|ctx| tree_from_libusb(&ctx));
        match tree {
            Ok(busses) => {
                print_tree(&busses);
                0
            }
            Err(e) => {
                eprintln!("unable to initialize libusb: {e}");
                1
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dev(bus: u32, ports: &[u32], devnum: u32) -> FlatDevice {
        FlatDevice {
            dev: TreeDevice {
                busnum: bus,
                portnum: *ports.last().unwrap(),
                devnum,
                ..Default::default()
            },
            ports: ports.to_vec(),
        }
    }

    #[test]
    fn assemble_tree() {
        let busses = vec![
            TreeBus {
                busnum: 2,
                ..Default::default()
            },
            TreeBus {
                busnum: 1,
                ..Default::default()
            },
        ];
        let devices = vec![
            dev(1, &[2, 4, 1], 5),
            dev(1, &[2], 2),
            dev(1, &[1], 3),
            dev(1, &[2, 4], 4),
            dev(1, &[2, 1], 6),
            dev(2, &[3], 7),
            dev(2, &[9, 1], 8), // orphan: parent missing
        ];
        let tree = assemble(busses, devices);
        assert_eq!(tree.iter().map(|b| b.busnum).collect::<Vec<_>>(), [1, 2]);
        let b1 = &tree[0];
        assert_eq!(
            b1.children.iter().map(|d| d.devnum).collect::<Vec<_>>(),
            [3, 2]
        );
        let hub = &b1.children[1];
        assert_eq!(
            hub.children.iter().map(|d| d.devnum).collect::<Vec<_>>(),
            [6, 4]
        );
        assert_eq!(hub.children[1].children[0].devnum, 5);
        assert_eq!(tree[1].children.len(), 1);
    }

    /// Build a miniature sysfs `bus/usb/devices` tree, laid out like the
    /// real thing (root hubs are symlinks into the device hierarchy, drivers
    /// are symlinks).
    fn fake_sysfs() -> std::path::PathBuf {
        use std::os::unix::fs::symlink;

        let base = std::env::temp_dir().join(format!("exausb-sysfs-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let devices = base.join("bus/usb/devices");
        let hcd = base.join("devices/pci0");
        let drivers = base.join("drivers");
        for d in [
            &devices,
            &hcd,
            &drivers.join("xhci_hcd"),
            &drivers.join("hub"),
            &drivers.join("usbhid"),
        ] {
            fs::create_dir_all(d).unwrap();
        }
        symlink(drivers.join("xhci_hcd"), hcd.join("driver")).unwrap();

        let write = |dir: &std::path::Path, attrs: &[(&str, &str)]| {
            fs::create_dir_all(dir).unwrap();
            for (k, v) in attrs {
                fs::write(dir.join(k), format!("{v}\n")).unwrap();
            }
        };
        let dev_attrs = |devnum: &'static str,
                         maxchild: &'static str,
                         speed: &'static str,
                         lanes: &'static str| {
            [
                ("devnum", devnum),
                ("bDeviceClass", "09"),
                ("idVendor", "1d6b"),
                ("idProduct", "0003"),
                ("maxchild", maxchild),
                ("speed", speed),
                ("rx_lanes", lanes),
                ("tx_lanes", lanes),
                ("manufacturer", "Maker\tCo"),
                ("product", "Thing"),
                ("serial", "123"),
            ]
        };

        // Root hub: usb1 -> ../../../devices/pci0/usb1
        write(&hcd.join("usb1"), &dev_attrs("1", "4", "5000", "1"));
        symlink(hcd.join("usb1"), devices.join("usb1")).unwrap();
        write(&devices.join("1-0:1.0"), &[("bInterfaceClass", "09")]);

        write(&devices.join("1-3"), &dev_attrs("2", "4", "480", "1"));
        write(&devices.join("1-3:1.0"), &[("bInterfaceClass", "09")]);
        symlink(drivers.join("hub"), devices.join("1-3:1.0/driver")).unwrap();

        write(&devices.join("1-3.2"), &dev_attrs("5", "0", "12", "2"));
        write(&devices.join("1-3.2:1.1"), &[("bInterfaceClass", "03")]);
        write(&devices.join("1-3.2:1.0"), &[("bInterfaceClass", "03")]);
        symlink(drivers.join("usbhid"), devices.join("1-3.2:1.0/driver")).unwrap();

        write(&devices.join("1-1"), &dev_attrs("3", "0", "480", "1"));
        base
    }

    #[test]
    fn sysfs_tree() {
        let base = fake_sysfs();
        let root = base.join("bus/usb/devices");
        let tree = tree_from_sysfs(root.to_str().unwrap()).unwrap();
        let _ = fs::remove_dir_all(&base);

        assert_eq!(tree.len(), 1);
        let bus = &tree[0];
        assert_eq!((bus.busnum, bus.devnum, bus.maxchild), (1, 1, 4));
        assert_eq!(bus.driver, "xhci_hcd");
        assert_eq!(bus.speed, "5000");
        assert!(!bus.synthetic);

        // Sorted by port; root hub interfaces aren't attached anywhere.
        assert_eq!(
            bus.children.iter().map(|d| d.portnum).collect::<Vec<_>>(),
            [1, 3]
        );
        assert!(bus.children[0].interfaces.is_empty());
        let hub = &bus.children[1];
        assert_eq!(hub.name, "1-3");
        assert_eq!(hub.interfaces.len(), 1);
        assert_eq!(hub.interfaces[0].driver, "hub");

        let hid = &hub.children[0];
        assert_eq!((hid.devnum, hid.portnum, hid.rx_lanes), (5, 2, 2));
        assert_eq!(hid.manufacturer, "Maker Co"); // control chars become spaces
        let ifs: Vec<_> = hid
            .interfaces
            .iter()
            .map(|i| (i.ifnum, i.class, i.driver.as_str()))
            .collect();
        assert_eq!(ifs, [(0, 3, "usbhid"), (1, 3, "[none]")]);
    }

    #[test]
    fn lanes() {
        assert_eq!(lanes_to_str(1, 1), "");
        assert_eq!(lanes_to_str(2, 2), "/x2");
        assert_eq!(lanes_to_str(1, 2), "/Tx1+Rx2");
    }
}
