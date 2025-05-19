// SPDX-License-Identifier: GPL-2.0-or-later
//! Communications (CDC), smart card (CCID), printer, DFU and wireless class
//! descriptor dumping.

use super::{dump_bytes, get_guid, USB_DT_CS_DEVICE};
use crate::usb::{le16, le32, Handle, InterfaceDescriptor};
use crate::usbmisc::get_dev_string;

pub(super) fn dump_wire_adapter(buf: &[u8]) {
    if buf[0] < 14 {
        println!("      Warning: Wire Adapter descriptor too short");
        return;
    }
    // The odd tabs are in upstream's format string too.
    println!("      Wire Adapter Class Descriptor:");
    println!("        bLength             {:5}", buf[0]);
    println!("        bDescriptorType     {:5}", buf[1]);
    println!("        bcdWAVersion        {:2x}.{:02x}", buf[3], buf[2]);
    println!("\t bNumPorts\t     {:5}", buf[4]);
    println!("\t bmAttributes\t     {:5}", buf[5]);
    println!("\t wNumRPRipes\t     {:5}", le16(&buf[6..]));
    println!("\t wRPipeMaxBlock\t     {:5}", le16(&buf[8..]));
    println!("\t bRPipeBlockSize     {:5}", buf[10]);
    println!("\t bPwrOn2PwrGood\t     {:5}", buf[11]);
    println!("\t bNumMMCIEs\t     {:5}", buf[12]);
    println!("\t DeviceRemovable     {:5}", buf[13]);
}

pub(super) fn dump_rc_interface(buf: &[u8]) {
    if buf[0] < 4 {
        println!("      Warning: Radio Control Interface descriptor too short");
        return;
    }
    println!("      Radio Control Interface Class Descriptor:");
    println!("        bLength             {:5}", buf[0]);
    println!("        bDescriptorType     {:5}", buf[1]);
    println!("        bcdRCIVersion       {:2x}.{:02x}", buf[3], buf[2]);
}

pub(super) fn dump_dfu_interface(buf: &[u8]) {
    if buf[1] != USB_DT_CS_DEVICE {
        println!("      Warning: Invalid descriptor");
    }
    if buf[0] < 7 {
        println!("      Warning: Descriptor too short");
        return;
    }
    println!("      Device Firmware Upgrade Interface Descriptor:");
    println!("        bLength                         {:5}", buf[0]);
    println!("        bDescriptorType                 {:5}", buf[1]);
    println!("        bmAttributes                    {:5}", buf[2]);
    if buf[2] & 0xf0 != 0 {
        println!("          (unknown attributes!)");
    }
    println!(
        "          Will {}Detach",
        if buf[2] & 0x08 != 0 { "" } else { "Not " }
    );
    println!(
        "          Manifestation {}",
        if buf[2] & 0x04 != 0 {
            "Tolerant"
        } else {
            "Intolerant"
        }
    );
    println!(
        "          Upload {}",
        if buf[2] & 0x02 != 0 {
            "Supported"
        } else {
            "Unsupported"
        }
    );
    println!(
        "          Download {}",
        if buf[2] & 0x01 != 0 {
            "Supported"
        } else {
            "Unsupported"
        }
    );
    println!(
        "        wDetachTimeout                  {:5} milliseconds",
        le16(&buf[3..])
    );
    println!(
        "        wTransferSize                   {:5} bytes",
        le16(&buf[5..])
    );

    // DFU 1.0 defines no version code, DFU 1.1 does.
    if buf[0] < 9 {
        return;
    }
    println!(
        "        bcdDFUVersion                   {:x}.{:02x}",
        buf[8], buf[7]
    );
}

pub(super) fn dump_ccid_device(buf: &[u8]) {
    if buf[0] < 54 {
        println!("      Warning: Descriptor too short");
        return;
    }
    println!("      ChipCard Interface Descriptor:");
    println!("        bLength             {:5}", buf[0]);
    println!("        bDescriptorType     {:5}", buf[1]);
    print!("        bcdCCID             {:2x}.{:02x}", buf[3], buf[2]);
    if buf[3] != 1 || (buf[2] != 0 && buf[2] != 0x10) {
        print!("  (Warning: Only accurate for version 1.0/1.1)");
    }
    println!();

    println!("        nMaxSlotIndex       {:5}", buf[4]);
    println!(
        "        bVoltageSupport     {:5}  {}{}{}",
        buf[5],
        if buf[5] & 1 != 0 { "5.0V " } else { "" },
        if buf[5] & 2 != 0 { "3.0V " } else { "" },
        if buf[5] & 4 != 0 { "1.8V " } else { "" }
    );

    let us = le32(&buf[6..]);
    print!("        dwProtocols         {:5} ", us);
    if us & 1 != 0 {
        print!(" T=0");
    }
    if us & 2 != 0 {
        print!(" T=1");
    }
    if us & !3 != 0 {
        print!(" (Invalid values detected)");
    }
    println!();

    println!("        dwDefaultClock      {:5}", le32(&buf[10..]));
    println!("        dwMaximumClock      {:5}", le32(&buf[14..]));
    println!("        bNumClockSupported  {:5}", buf[18]);
    println!("        dwDataRate        {:7} bps", le32(&buf[19..]));
    println!("        dwMaxDataRate     {:7} bps", le32(&buf[23..]));
    println!("        bNumDataRatesSupp.  {:5}", buf[27]);

    println!("        dwMaxIFSD           {:5}", le32(&buf[28..]));

    let us = le32(&buf[32..]);
    print!("        dwSyncProtocols  {:08X} ", us);
    if us & 1 != 0 {
        print!(" 2-wire");
    }
    if us & 2 != 0 {
        print!(" 3-wire");
    }
    if us & 4 != 0 {
        print!(" I2C");
    }
    println!();

    let us = le32(&buf[36..]);
    print!("        dwMechanical     {:08X} ", us);
    if us & 1 != 0 {
        print!(" accept");
    }
    if us & 2 != 0 {
        print!(" eject");
    }
    if us & 4 != 0 {
        print!(" capture");
    }
    if us & 8 != 0 {
        print!(" lock");
    }
    println!();

    let us = le32(&buf[40..]);
    println!("        dwFeatures       {:08X}", us);
    if us & 0x0002 != 0 {
        println!("          Auto configuration based on ATR");
    }
    if us & 0x0004 != 0 {
        println!("          Auto activation on insert");
    }
    if us & 0x0008 != 0 {
        println!("          Auto voltage selection");
    }
    if us & 0x0010 != 0 {
        println!("          Auto clock change");
    }
    if us & 0x0020 != 0 {
        println!("          Auto baud rate change");
    }
    match us & (0x0040 | 0x0080) {
        0x0040 => println!("          Auto parameter negotiation made by CCID"),
        0x0080 => println!("          Auto PPS made by CCID"),
        0 => {}
        _ => println!("        WARNING: conflicting negotiation features"),
    }

    if us & 0x0100 != 0 {
        println!("          CCID can set ICC in clock stop mode");
    }
    if us & 0x0200 != 0 {
        println!("          NAD value other than 0x00 accepted (T=1)");
    }
    if us & 0x0400 != 0 {
        println!("          Auto IFSD exchange (T=1)");
    }

    match us & 0x0007_0000 {
        0 => println!("          Character level exchange"),
        0x0001_0000 => println!("          TPDU level exchange"),
        0x0002_0000 => println!("          Short APDU level exchange"),
        0x0004_0000 => println!("          Short and extended APDU level exchange"),
        _ => println!("        WARNING: conflicting exchange levels"),
    }

    if us & 0x0010_0000 != 0 {
        println!("          USB wakeup on ICC insertion and removal");
    }

    println!("        dwMaxCCIDMsgLen     {:5}", le32(&buf[44..]));

    print!("        bClassGetResponse    ");
    if buf[48] == 0xff {
        println!("echo");
    } else {
        println!("  {:02X}", buf[48]);
    }

    print!("        bClassEnvelope       ");
    if buf[49] == 0xff {
        println!("echo");
    } else {
        println!("  {:02X}", buf[49]);
    }

    print!("        wlcdLayout           ");
    if buf[50] == 0 && buf[51] == 0 {
        println!("none");
    } else {
        println!("{} cols {} lines", buf[50], buf[51]);
    }

    print!("        bPINSupport         {:5} ", buf[52]);
    if buf[52] & 1 != 0 {
        print!(" verification");
    }
    if buf[52] & 2 != 0 {
        print!(" modification");
    }
    println!();

    println!("        bMaxCCIDBusySlots   {:5}", buf[53]);

    if buf[0] > 54 {
        print!("        junk             ");
        dump_bytes(&buf[54..], buf[0] as usize - 54);
    }
}

pub(super) fn dump_printer_device(
    dev: Option<&Handle>,
    interface: &InterfaceDescriptor,
    buf: &[u8],
) {
    if interface.b_interface_protocol != 0x04 {
        // Only IPP-over-USB.
        return;
    }

    if buf[0] < 4 {
        println!("        Warning: IPP Printer Descriptor too short");
        return;
    }

    println!("        IPP Printer Descriptor:");
    println!("          bLength             {:5}", buf[0]);
    println!("          bDescriptorType     {:5}", buf[1]);
    println!("          bcdReleaseNumber    {:5}", buf[2]);
    println!("          bcdNumDescriptors   {:5}", buf[3]);

    let blen = buf[0] as usize;
    let mut n = 4;
    for _ in 0..buf[3] {
        if n + 2 > blen || n + 2 + buf[n + 1] as usize > blen {
            println!("            Warning: Descriptor too short");
            break;
        }
        match buf[n] {
            0x00 => {
                // Basic capabilities
                if n + 6 > blen {
                    println!("          Warning: Descriptor too short");
                    return;
                }
                let caps = le16(&buf[n + 2..]);
                let uuid = get_dev_string(dev, buf[n + 5]);

                println!("            iIPPVersionsSupported {:5}", buf[n + 4]);
                println!(
                    "            iIPPPrinterUUID       {:5} {}",
                    buf[n + 5],
                    uuid
                );
                print!("            wBasicCapabilities   0x{:04x} ", caps);
                if caps & 0x01 != 0 {
                    print!(" Print");
                }
                if caps & 0x02 != 0 {
                    print!(" Scan");
                }
                if caps & 0x04 != 0 {
                    print!(" Fax");
                }
                if caps & 0x08 != 0 {
                    print!(" Other");
                }
                if caps & 0x10 != 0 {
                    print!(" HTTP-over-USB");
                }
                match caps & 0x60 {
                    0x00 => print!(" No-Auth"),
                    0x20 => print!(" Username-Auth"),
                    0x40 => print!(" Reserved-Auth"),
                    _ => print!(" Negotiable-Auth"),
                }
                println!();
            }
            _ => {
                // Vendor Specific, Ignore for now.
                println!(
                    "            UnknownCapabilities   {:5} {:5}",
                    buf[n],
                    buf[n + 1]
                );
            }
        }
        n += 2 + buf[n + 1] as usize;
    }
}

/// `str` if the index is set and the string is non-empty, else "(??)".
fn str_or_unknown(idx: u8, s: &str) -> &str {
    if idx != 0 && !s.is_empty() {
        s
    } else {
        "(??)"
    }
}

pub(super) fn dump_comm_descriptor(dev: Option<&Handle>, buf: &[u8], indent: &str) {
    if buf[0] < 3 {
        println!("{indent}Warning: Descriptor too short");
        return;
    }
    let len = buf[0];
    let ty = match buf[2] {
        0x00 => {
            if len != 5 {
                "Header"
            } else {
                println!("{indent}CDC Header:");
                println!("{indent}  bcdCDC               {:x}.{:02x}", buf[4], buf[3]);
                return;
            }
        }
        0x01 => {
            // call management functional desc
            if len != 5 {
                "Call Management"
            } else {
                println!("{indent}CDC Call Management:");
                println!("{indent}  bmCapabilities       0x{:02x}", buf[3]);
                if buf[3] & 0x01 != 0 {
                    println!("{indent}    call management");
                }
                if buf[3] & 0x02 != 0 {
                    println!("{indent}    use DataInterface");
                }
                println!("{indent}  bDataInterface          {}", buf[4]);
                return;
            }
        }
        0x02 => {
            // acm functional desc
            if len != 4 {
                "ACM"
            } else {
                println!("{indent}CDC ACM:");
                println!("{indent}  bmCapabilities       0x{:02x}", buf[3]);
                if buf[3] & 0x08 != 0 {
                    println!("{indent}    connection notifications");
                }
                if buf[3] & 0x04 != 0 {
                    println!("{indent}    sends break");
                }
                if buf[3] & 0x02 != 0 {
                    println!("{indent}    line coding and serial state");
                }
                if buf[3] & 0x01 != 0 {
                    println!("{indent}    get/set/clear comm features");
                }
                return;
            }
        }
        0x06 => {
            // union desc
            if len < 5 {
                "Union"
            } else {
                println!("{indent}CDC Union:");
                println!("{indent}  bMasterInterface        {}", buf[3]);
                print!("{indent}  bSlaveInterface         ");
                for b in &buf[4..len as usize] {
                    print!("{b} ");
                }
                println!();
                return;
            }
        }
        0x07 => {
            // country selection functional desc
            if len < 6 || len & 1 != 0 {
                "Country Selection"
            } else {
                let s = get_dev_string(dev, buf[3]);
                println!("{indent}Country Selection:");
                println!(
                    "{indent}  iCountryCodeRelDate     {:4} {}",
                    buf[3],
                    str_or_unknown(buf[3], &s)
                );
                for pair in buf[4..len as usize].chunks(2) {
                    println!(
                        "{indent}  wCountryCode          0x{:02x}{:02x}",
                        pair[0], pair[1]
                    );
                }
                return;
            }
        }
        0x08 => {
            // telephone operational modes
            if len != 4 {
                "Telephone Operations"
            } else {
                println!("{indent}CDC Telephone operations:");
                println!("{indent}  bmCapabilities       0x{:02x}", buf[3]);
                if buf[3] & 0x04 != 0 {
                    println!("{indent}    computer centric mode");
                }
                if buf[3] & 0x02 != 0 {
                    println!("{indent}    standalone mode");
                }
                if buf[3] & 0x01 != 0 {
                    println!("{indent}    simple mode");
                }
                return;
            }
        }
        0x0a => {
            // network channel terminal
            if len != 7 {
                "Network Channel Terminal"
            } else {
                let s = get_dev_string(dev, buf[4]);
                println!("{indent}Network Channel Terminal:");
                println!("{indent}  bEntityId               {:3}", buf[3]);
                println!("{indent}  iName                   {:3} {}", buf[4], s);
                println!("{indent}  bChannelIndex           {:3}", buf[5]);
                println!("{indent}  bPhysicalInterface      {:3}", buf[6]);
                return;
            }
        }
        0x0f => {
            // ethernet functional desc
            if len != 13 {
                "Ethernet"
            } else {
                let s = get_dev_string(dev, buf[3]);
                let tmp = le32(&buf[4..]);
                println!("{indent}CDC Ethernet:");
                println!(
                    "{indent}  iMacAddress             {:10} {}",
                    buf[3],
                    str_or_unknown(buf[3], &s)
                );
                println!("{indent}  bmEthernetStatistics    0x{:08x}", tmp);
                // TODO: translate the 28 bits of bmEthernetStatistics (USB
                // CDC ECM Subclass, version 1.2, table 4) into something
                // "real".
                println!("{indent}  wMaxSegmentSize         {:10}", le16(&buf[8..]));
                println!(
                    "{indent}  wNumberMCFilters            0x{:04x}",
                    le16(&buf[10..])
                );
                println!("{indent}  bNumberPowerFilters     {:10}", buf[12]);
                return;
            }
        }
        0x11 => {
            // WHCM functional desc
            if len != 5 {
                "WHCM version"
            } else {
                println!("{indent}CDC WHCM:");
                println!("{indent}  bcdVersion           {:x}.{:02x}", buf[4], buf[3]);
                return;
            }
        }
        0x12 => {
            // MDLM functional desc
            if len != 21 {
                "MDLM"
            } else {
                println!("{indent}CDC MDLM:");
                println!("{indent}  bcdCDC               {:x}.{:02x}", buf[4], buf[3]);
                println!("{indent}  bGUID               {}", get_guid(&buf[5..]));
                return;
            }
        }
        0x13 => {
            // MDLM detail desc
            if len < 5 {
                "MDLM detail"
            } else {
                println!("{indent}CDC MDLM detail:");
                println!("{indent}  bGuidDescriptorType  {:02x}", buf[3]);
                print!("{indent}  bDetailData         ");
                dump_bytes(&buf[4..], len as usize - 4);
                return;
            }
        }
        0x14 => {
            // device management functional desc
            if len != 7 {
                "Device Management"
            } else {
                println!("{indent}CDC Device Management:");
                println!("{indent}  bcdVersion           {:x}.{:02x}", buf[4], buf[3]);
                println!("{indent}  wMaxCommand          {}", le16(&buf[5..]));
                return;
            }
        }
        0x15 => {
            // OBEX functional desc
            if len != 5 {
                "OBEX"
            } else {
                println!("{indent}CDC OBEX:");
                println!("{indent}  bcdVersion           {:x}.{:02x}", buf[4], buf[3]);
                return;
            }
        }
        0x16 => {
            // command set functional desc
            if len != 22 {
                "Command Set"
            } else {
                let s = get_dev_string(dev, buf[5]);
                println!("{indent}CDC Command Set:");
                println!("{indent}  bcdVersion           {:x}.{:02x}", buf[4], buf[3]);
                println!(
                    "{indent}  iCommandSet          {:4} {}",
                    buf[5],
                    str_or_unknown(buf[5], &s)
                );
                println!("{indent}  bGUID                {}", get_guid(&buf[6..]));
                return;
            }
        }
        0x1a => {
            // NCM functional desc
            if len != 6 {
                "NCM"
            } else {
                println!("{indent}CDC NCM:");
                println!("{indent}  bcdNcmVersion        {:x}.{:02x}", buf[4], buf[3]);
                println!("{indent}  bmNetworkCapabilities 0x{:02x}", buf[5]);
                const CAPS: [(u8, &str); 6] = [
                    (1 << 5, "8-byte ntb input size"),
                    (1 << 4, "crc mode"),
                    (1 << 3, "max datagram size"),
                    (1 << 2, "encapsulated commands"),
                    (1 << 1, "net address"),
                    (1 << 0, "packet filter"),
                ];
                for (bit, s) in CAPS {
                    if buf[5] & bit != 0 {
                        println!("{indent}    {s}");
                    }
                }
                return;
            }
        }
        0x1b => {
            // MBIM functional desc
            if len != 12 {
                "MBIM"
            } else {
                println!("{indent}CDC MBIM:");
                println!("{indent}  bcdMBIMVersion       {:x}.{:02x}", buf[4], buf[3]);
                println!("{indent}  wMaxControlMessage   {}", le16(&buf[5..]));
                println!("{indent}  bNumberFilters       {}", buf[7]);
                println!("{indent}  bMaxFilterSize       {}", buf[8]);
                println!("{indent}  wMaxSegmentSize      {}", le16(&buf[9..]));
                println!("{indent}  bmNetworkCapabilities 0x{:02x}", buf[11]);
                if buf[11] & 0x20 != 0 {
                    println!("{indent}    8-byte ntb input size");
                }
                if buf[11] & 0x08 != 0 {
                    println!("{indent}    max datagram size");
                }
                return;
            }
        }
        0x1c => {
            // MBIM extended functional desc
            if len != 8 {
                "MBIM Extended"
            } else {
                println!("{indent}CDC MBIM Extended:");
                println!(
                    "{indent}  bcdMBIMExtendedVersion          {:2x}.{:02x}",
                    buf[4], buf[3]
                );
                println!("{indent}  bMaxOutstandingCommandMessages    {:3}", buf[5]);
                println!(
                    "{indent}  wMTU                            {:5}",
                    le16(&buf[6..])
                );
                return;
            }
        }
        _ => {
            // There are about a dozen more descriptor types, if anyone has
            // a device with them in it, we'll add them here in the future
            // if really needed.
            print!("{indent}UNRECOGNIZED CDC: ");
            dump_bytes(buf, len as usize);
            return;
        }
    };

    print!("{indent}INVALID CDC ({ty}): ");
    dump_bytes(buf, len as usize);
}
