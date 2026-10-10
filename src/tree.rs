// SPDX-License-Identifier: GPL-2.0-or-later
//! `exausb -t`: the physical USB device hierarchy as a tree.
//!
//! Built from libusb's topology information (bus number and port path), so
//! it works the same on every platform.  Some libusb backends (e.g. macOS)
//! don't report root hubs; those busses show just their number.

use std::collections::BTreeMap;

use rusb::UsbContext;

use crate::model::{join, Node};
use crate::names::{get_vendor_product_with_fallback, names_class, names_protocol, names_subclass};
use crate::usb::{self, Context, Device, Speed};

/// Short speed notation, e.g. `480M`.
pub fn speed_short(speed: Speed) -> &'static str {
    match speed {
        Speed::Low => "1.5M",
        Speed::Full => "12M",
        Speed::High => "480M",
        Speed::Super => "5G",
        Speed::SuperPlus => "10G",
        Speed::SuperPlusX2 => "20G",
        Speed::Unknown => "",
    }
}

/// The interfaces of the device's active configuration.
fn interfaces(dev: &Device, node: &mut Node) {
    let Ok(config) = usb::active_config_descriptor(dev) else {
        return;
    };
    for alt in config
        .interfaces
        .iter()
        .filter_map(|i| i.altsettings.first())
    {
        let (class, sub, proto) = (
            alt.b_interface_class,
            alt.b_interface_sub_class,
            alt.b_interface_protocol,
        );
        node.section(format!("If {}", alt.b_interface_number))
            .note(join(&[
                names_class(class).unwrap_or(""),
                names_subclass(class, sub).unwrap_or(""),
                names_protocol(class, sub, proto).unwrap_or(""),
            ]));
    }
}

type Devices = BTreeMap<(u8, Vec<u8>), Node>;

/// The port path of a device's parent: the nearest ancestor that exists,
/// or the bus (empty path).
fn parent(devices: &Devices, bus: u8, ports: &[u8]) -> Vec<u8> {
    (1..ports.len())
        .rev()
        .map(|n| ports[..n].to_vec())
        .find(|p| devices.contains_key(&(bus, p.clone())))
        .unwrap_or_default()
}

/// The devices below port path `of`, each with its own children attached.
fn children(devices: &Devices, bus: u8, of: &[u8]) -> Vec<Node> {
    // BTreeMap order sorts siblings by port path.
    devices
        .iter()
        .filter(|((b, ports), _)| *b == bus && parent(devices, bus, ports) == of)
        .map(|((_, ports), node)| {
            let mut node = node.clone();
            node.children.extend(children(devices, bus, ports));
            node
        })
        .collect()
}

/// Attach each device to its parent hub or bus.
fn assemble(mut busses: BTreeMap<u8, Node>, devices: Devices) -> Vec<Node> {
    for bus in devices.keys().map(|(b, _)| *b) {
        busses
            .entry(bus)
            .or_insert_with(|| Node::new(format!("Bus {bus:03}")));
    }
    for (bus, node) in &mut busses {
        node.children.extend(children(&devices, *bus, &[]));
    }
    busses.into_values().collect()
}

/// The key identifying a device in the tree: bus and device number.
pub fn device_key(dev: &Device) -> u32 {
    (dev.bus_number() as u32) << 8 | dev.address() as u32
}

/// Build the device tree, one node per bus.  Device nodes carry their
/// [`device_key`]; `with_interfaces` adds each device's interfaces.
pub fn tree(ctx: &Context, with_interfaces: bool) -> rusb::Result<Vec<Node>> {
    let mut busses = BTreeMap::new();
    let mut devices = Devices::new();

    for dev in ctx.devices()?.iter() {
        let desc = usb::device_descriptor(&dev);
        let bus = dev.bus_number();
        let ports = dev.port_numbers().unwrap_or_default();
        let id = format!("{:04x}:{:04x}", desc.id_vendor, desc.id_product);
        let speed = speed_short(usb::device_speed(&dev));

        if ports.is_empty() {
            // A root hub.
            let mut node = Node::new(format!("Bus {bus:03}"));
            node.value(id)
                .note(join(&["root hub", speed]))
                .key(device_key(&dev));
            busses.insert(bus, node);
            continue;
        }

        let (vendor, product) = get_vendor_product_with_fallback(&dev, None);
        let mut node = Node::new(format!(
            "Port {}  {vendor} {product}",
            ports.last().unwrap()
        ));
        node.value(id)
            .note(join(&[speed, &format!("Device {:03}", dev.address())]))
            .key(device_key(&dev));
        if with_interfaces {
            interfaces(&dev, &mut node);
        }
        devices.insert((bus, ports), node);
    }

    Ok(assemble(busses, devices))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn titles(nodes: &[Node]) -> Vec<&str> {
        nodes.iter().map(|n| n.name.as_str()).collect()
    }

    #[test]
    fn assemble_tree() {
        let mut busses = BTreeMap::new();
        busses.insert(1, Node::new("Bus 001"));
        let mut devices = Devices::new();
        for (bus, ports) in [
            (1, vec![2, 4, 1]),
            (1, vec![2]),
            (1, vec![1]),
            (1, vec![2, 4]),
            (1, vec![2, 1]),
            (2, vec![3]),
            (2, vec![9, 1]), // parent missing: attached to the bus
        ] {
            let name = format!("{bus}-{ports:?}");
            devices.insert((bus, ports), Node::new(name));
        }

        let tree = assemble(busses, devices);
        assert_eq!(titles(&tree), ["Bus 001", "Bus 002"]);
        assert_eq!(titles(&tree[0].children), ["1-[1]", "1-[2]"]);
        let hub = &tree[0].children[1];
        assert_eq!(titles(&hub.children), ["1-[2, 1]", "1-[2, 4]"]);
        assert_eq!(titles(&hub.children[1].children), ["1-[2, 4, 1]"]);
        assert_eq!(titles(&tree[1].children), ["2-[3]", "2-[9, 1]"]);
    }
}
