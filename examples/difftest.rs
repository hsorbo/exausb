// SPDX-License-Identifier: GPL-2.0-or-later
//! Differential-testing driver: dumps synthetic descriptors read from stdin
//! so the output can be compared with upstream's C implementation (see
//! `tests/difftest/`).
//!
//! Input format, one record per line, numbers in hex, `-` for no bytes:
//!
//! ```text
//! CONFIG <bcdUSB> <bmAttributes> <MaxPower> <iConfiguration> <extra>
//! IFACE <class> <subclass> <protocol> <ifnum> <alt> <extra>
//! EP <bLength> <bEndpointAddress> <bmAttributes> <wMaxPacketSize> <bInterval> <extra>
//! END
//! HUB <tt_type> <bytes>
//! REPORT <bytes>
//! BOS <lpm_required> <bytes>
//! ```

use std::io::{self, BufRead};

use exausb::harness::{self, ConfigDescriptor, EndpointDescriptor, Interface, InterfaceDescriptor};

fn num(s: &str) -> u32 {
    u32::from_str_radix(s, 16).expect("bad number")
}

fn bytes(s: &str) -> Vec<u8> {
    if s == "-" {
        return Vec::new();
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("bad hex"))
        .collect()
}

fn main() {
    let mut config: Option<(u16, ConfigDescriptor)> = None;

    for (n, line) in io::stdin().lock().lines().enumerate() {
        let line = line.unwrap();
        let f: Vec<&str> = line.split_whitespace().collect();
        let Some(&kind) = f.first() else { continue };
        if kind != "IFACE" && kind != "EP" && kind != "END" {
            println!("#### record {n}");
        }
        match kind {
            "CONFIG" => {
                config = Some((
                    num(f[1]) as u16,
                    ConfigDescriptor {
                        b_length: 9,
                        b_descriptor_type: 2,
                        w_total_length: 0,
                        b_num_interfaces: 0,
                        b_configuration_value: 1,
                        i_configuration: num(f[4]) as u8,
                        bm_attributes: num(f[2]) as u8,
                        max_power: num(f[3]) as u8,
                        interfaces: Vec::new(),
                        extra: bytes(f[5]),
                    },
                ));
            }
            "IFACE" => {
                let (_, c) = config.as_mut().unwrap();
                c.b_num_interfaces += 1;
                c.interfaces.push(Interface {
                    altsettings: vec![InterfaceDescriptor {
                        b_length: 9,
                        b_descriptor_type: 4,
                        b_interface_number: num(f[4]) as u8,
                        b_alternate_setting: num(f[5]) as u8,
                        b_num_endpoints: 0,
                        b_interface_class: num(f[1]) as u8,
                        b_interface_sub_class: num(f[2]) as u8,
                        b_interface_protocol: num(f[3]) as u8,
                        i_interface: 0,
                        endpoints: Vec::new(),
                        extra: bytes(f[6]),
                    }],
                });
            }
            "EP" => {
                let (_, c) = config.as_mut().unwrap();
                let alt = &mut c.interfaces.last_mut().unwrap().altsettings[0];
                alt.b_num_endpoints += 1;
                alt.endpoints.push(EndpointDescriptor {
                    b_length: num(f[1]) as u8,
                    b_descriptor_type: 5,
                    b_endpoint_address: num(f[2]) as u8,
                    bm_attributes: num(f[3]) as u8,
                    w_max_packet_size: num(f[4]) as u16,
                    b_interval: num(f[5]) as u8,
                    b_refresh: 0,
                    b_synch_address: 0,
                    extra: bytes(f[6]),
                });
            }
            "END" => {
                let (speed, c) = config.take().unwrap();
                harness::dump_config(&c, speed);
            }
            "HUB" => harness::dump_hub("", &bytes(f[2]), num(f[1]) as u8),
            "REPORT" => harness::dump_report_desc(&bytes(f[1])),
            "BOS" => {
                harness::dump_device_capabilities(&bytes(f[2]), num(f[1]) != 0);
            }
            other => panic!("unknown record {other}"),
        }
    }
}
