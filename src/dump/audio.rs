// SPDX-License-Identifier: GPL-2.0-or-later
//! Audio and MIDI class descriptor dumping.

use super::{dump_bytes, dump_junk, USB_DT_CS_ENDPOINT, USB_DT_CS_INTERFACE};
use crate::desc_defs::{self as defs, AudioDescs};
use crate::desc_dump::desc_dump;
use crate::usb::{le32, Handle};

const USB_AUDIO_CLASS_1: u8 = 0x00;
const USB_AUDIO_CLASS_2: u8 = 0x20;
/// USB DCD for Audio Devices Release 3.0: Section A.6, pp139
const USB_AUDIO_CLASS_3: u8 = 0x30;

fn dump_audio_subtype(
    dev: Option<&Handle>,
    name: &str,
    desc: &AudioDescs,
    buf: &[u8],
    protocol: u8,
    indent: u32,
) {
    const STRINGS: [&str; 3] = ["UAC1", "UAC2", "UAC3"];
    let idx = match protocol {
        USB_AUDIO_CLASS_2 => 1,
        USB_AUDIO_CLASS_3 => 2,
        _ => 0,
    };

    println!("({name})");

    let Some(d) = desc[idx] else {
        println!(
            "{:w$}Warning: {} descriptors are illegal for {}",
            "",
            name,
            STRINGS[idx],
            w = (indent * 2) as usize
        );
        return;
    };

    // Skip the first three bytes; those common fields have already been
    // dumped.
    desc_dump(dev, d, &buf[3..], buf[0] as u32 - 3, indent);
}

/// USB Audio Class AudioControl interface subtypes.
#[derive(PartialEq, Eq)]
enum UacSubtype {
    Header,
    InputTerminal,
    OutputTerminal,
    ExtendedTerminal,
    MixerUnit,
    SelectorUnit,
    FeatureUnit,
    EffectUnit,
    ProcessingUnit,
    ExtensionUnit,
    ClockSource,
    ClockSelector,
    ClockMultiplier,
    SampleRateConverter,
    Connectors,
    PowerDomain,
    Unknown,
}

/// UAC1, UAC2, and UAC3 define bDescriptorSubtype differently for the
/// AudioControl interface, so we need to do some ugly remapping:
///
/// ```text
/// val  | UAC1            | UAC2                  | UAC3
/// -----|-----------------|-----------------------|---------------------
/// 0x00 | AC UNDEFINED    | AC UNDEFINED          | AC UNDEFINED
/// 0x01 | HEADER          | HEADER                | HEADER
/// 0x02 | INPUT_TERMINAL  | INPUT_TERMINAL        | INPUT_TERMINAL
/// 0x03 | OUTPUT_TERMINAL | OUTPUT_TERMINAL       | OUTPUT_TERMINAL
/// 0x04 | MIXER_UNIT      | MIXER_UNIT            | EXTENDED_TERMINAL
/// 0x05 | SELECTOR_UNIT   | SELECTOR_UNIT         | MIXER_UNIT
/// 0x06 | FEATURE_UNIT    | FEATURE_UNIT          | SELECTOR_UNIT
/// 0x07 | PROCESSING_UNIT | EFFECT_UNIT           | FEATURE_UNIT
/// 0x08 | EXTENSION_UNIT  | PROCESSING_UNIT       | EFFECT_UNIT
/// 0x09 | -               | EXTENSION_UNIT        | PROCESSING_UNIT
/// 0x0a | -               | CLOCK_SOURCE          | EXTENSION_UNIT
/// 0x0b | -               | CLOCK_SELECTOR        | CLOCK_SOURCE
/// 0x0c | -               | CLOCK_MULTIPLIER      | CLOCK_SELECTOR
/// 0x0d | -               | SAMPLE_RATE_CONVERTER | CLOCK_MULTIPLIER
/// 0x0e | -               | -                     | SAMPLE_RATE_CONVERTER
/// 0x0f | -               | -                     | CONNECTORS
/// 0x10 | -               | -                     | POWER_DOMAIN
/// ```
///
/// Values not known to require mapping (or an unknown protocol) are
/// interpreted with the UAC3 numbering, like upstream.
fn get_uac_interface_subtype(c: u8, protocol: u8) -> UacSubtype {
    use UacSubtype::*;
    let mapped = match (protocol, c) {
        (USB_AUDIO_CLASS_1, 0x04) | (USB_AUDIO_CLASS_2, 0x04) => Some(MixerUnit),
        (USB_AUDIO_CLASS_1, 0x05) | (USB_AUDIO_CLASS_2, 0x05) => Some(SelectorUnit),
        (USB_AUDIO_CLASS_1, 0x06) | (USB_AUDIO_CLASS_2, 0x06) => Some(FeatureUnit),
        (USB_AUDIO_CLASS_1, 0x07) => Some(ProcessingUnit),
        (USB_AUDIO_CLASS_1, 0x08) => Some(ExtensionUnit),
        (USB_AUDIO_CLASS_2, 0x07) => Some(EffectUnit),
        (USB_AUDIO_CLASS_2, 0x08) => Some(ProcessingUnit),
        (USB_AUDIO_CLASS_2, 0x09) => Some(ExtensionUnit),
        (USB_AUDIO_CLASS_2, 0x0a) => Some(ClockSource),
        (USB_AUDIO_CLASS_2, 0x0b) => Some(ClockSelector),
        (USB_AUDIO_CLASS_2, 0x0c) => Some(ClockMultiplier),
        (USB_AUDIO_CLASS_2, 0x0d) => Some(SampleRateConverter),
        _ => None,
    };
    mapped.unwrap_or(match c {
        0x01 => Header,
        0x02 => InputTerminal,
        0x03 => OutputTerminal,
        0x04 => ExtendedTerminal,
        0x05 => MixerUnit,
        0x06 => SelectorUnit,
        0x07 => FeatureUnit,
        0x08 => EffectUnit,
        0x09 => ProcessingUnit,
        0x0a => ExtensionUnit,
        0x0b => ClockSource,
        0x0c => ClockSelector,
        0x0d => ClockMultiplier,
        0x0e => SampleRateConverter,
        0x0f => Connectors,
        0x10 => PowerDomain,
        _ => Unknown,
    })
}

pub(super) fn dump_audiocontrol_interface(dev: Option<&Handle>, buf: &[u8], protocol: u8) {
    use UacSubtype::*;

    if buf[1] != USB_DT_CS_INTERFACE {
        println!("      Warning: Invalid descriptor");
    }
    if buf[0] < 3 {
        println!("      Warning: Descriptor too short");
        return;
    }
    println!("      AudioControl Interface Descriptor:");
    println!("        bLength             {:5}", buf[0]);
    println!("        bDescriptorType     {:5}", buf[1]);
    print!("        bDescriptorSubtype  {:5} ", buf[2]);

    let (name, desc): (&str, &AudioDescs) = match get_uac_interface_subtype(buf[2], protocol) {
        Header => ("HEADER", &defs::DESC_AUDIO_AC_HEADER),
        InputTerminal => ("INPUT_TERMINAL", &defs::DESC_AUDIO_AC_INPUT_TERMINAL),
        OutputTerminal => ("OUTPUT_TERMINAL", &defs::DESC_AUDIO_AC_OUTPUT_TERMINAL),
        MixerUnit => ("MIXER_UNIT", &defs::DESC_AUDIO_AC_MIXER_UNIT),
        SelectorUnit => ("SELECTOR_UNIT", &defs::DESC_AUDIO_AC_SELECTOR_UNIT),
        FeatureUnit => ("FEATURE_UNIT", &defs::DESC_AUDIO_AC_FEATURE_UNIT),
        ProcessingUnit => ("PROCESSING_UNIT", &defs::DESC_AUDIO_AC_PROCESSING_UNIT),
        ExtensionUnit => ("EXTENSION_UNIT", &defs::DESC_AUDIO_AC_EXTENSION_UNIT),
        ClockSource => ("CLOCK_SOURCE", &defs::DESC_AUDIO_AC_CLOCK_SOURCE),
        ClockSelector => ("CLOCK_SELECTOR", &defs::DESC_AUDIO_AC_CLOCK_SELECTOR),
        ClockMultiplier => ("CLOCK_MULTIPLIER", &defs::DESC_AUDIO_AC_CLOCK_MULTIPLIER),
        // Upstream uses the clock multiplier definition here.
        SampleRateConverter => (
            "SAMPLING_RATE_CONVERTER",
            &defs::DESC_AUDIO_AC_CLOCK_MULTIPLIER,
        ),
        EffectUnit => ("EFFECT_UNIT", &defs::DESC_AUDIO_AC_EFFECT_UNIT),
        PowerDomain => ("POWER_DOMAIN", &defs::DESC_AUDIO_AC_POWER_DOMAIN),
        // Upstream has no case for these (UAC3 only) subtypes.
        ExtendedTerminal | Connectors | Unknown => {
            print!("(unknown)\n        Invalid desc subtype:");
            if buf[0] > 3 {
                dump_bytes(&buf[3..], buf[0] as usize - 3);
            } else {
                println!();
            }
            return;
        }
    };
    dump_audio_subtype(dev, name, desc, buf, protocol, 4);
}

/// Print a 3-byte little-endian sample frequency.
fn freq3(buf: &[u8], at: usize) -> u32 {
    buf[at] as u32 | (buf[at + 1] as u32) << 8 | (buf[at + 2] as u32) << 16
}

fn too_short() {
    println!("      Warning: Descriptor too short");
}

/// UAC1 FORMAT_TYPE_I and FORMAT_TYPE_III share a layout.
fn dump_uac1_format_type_i_iii(buf: &[u8]) {
    let j = if buf[7] != 0 {
        buf[7] as usize * 3 + 8
    } else {
        14
    };
    if (buf[0] as usize) < j {
        too_short();
        return;
    }
    println!("        bNrChannels         {:5}", buf[4]);
    println!("        bSubframeSize       {:5}", buf[5]);
    println!("        bBitResolution      {:5}", buf[6]);
    println!(
        "        bSamFreqType        {:5} {}",
        buf[7],
        if buf[7] != 0 {
            "Discrete"
        } else {
            "Continuous"
        }
    );
    if buf[7] == 0 {
        println!("        tLowerSamFreq     {:7}", freq3(buf, 8));
        println!("        tUpperSamFreq     {:7}", freq3(buf, 11));
    } else {
        for i in 0..buf[7] as usize {
            println!("        tSamFreq[{:2}]      {:7}", i, freq3(buf, 8 + 3 * i));
        }
    }
    dump_junk(buf, "        ", j);
}

fn print_drc(features: u8) {
    print!("          Internal Dynamic Range Control: ");
    match (features >> 4) & 3 {
        0 => println!("not supported"),
        1 => println!("supported but not scalable"),
        2 => println!("scalable, common boost and cut scaling value"),
        _ => println!("scalable, separate boost and cut scaling value"),
    }
}

pub(super) fn dump_audiostreaming_interface(dev: Option<&Handle>, buf: &[u8], protocol: u8) {
    const FMT_I_TAG: [&str; 6] = [
        "TYPE_I_UNDEFINED",
        "PCM",
        "PCM8",
        "IEEE_FLOAT",
        "ALAW",
        "MULAW",
    ];
    const FMT_II_TAG: [&str; 3] = ["TYPE_II_UNDEFINED", "MPEG", "AC-3"];
    const FMT_III_TAG: [&str; 7] = [
        "TYPE_III_UNDEFINED",
        "IEC1937_AC-3",
        "IEC1937_MPEG-1_Layer1",
        "IEC1937_MPEG-Layer2/3/NOEXT",
        "IEC1937_MPEG-2_EXT",
        "IEC1937_MPEG-2_Layer1_LS",
        "IEC1937_MPEG-2_Layer2/3_LS",
    ];

    if buf[1] != USB_DT_CS_INTERFACE {
        println!("      Warning: Invalid descriptor");
    }
    if buf[0] < 3 {
        too_short();
        return;
    }
    println!("      AudioStreaming Interface Descriptor:");
    println!("        bLength             {:5}", buf[0]);
    println!("        bDescriptorType     {:5}", buf[1]);
    print!("        bDescriptorSubtype  {:5} ", buf[2]);

    match buf[2] {
        0x01 => {
            // AS_GENERAL
            dump_audio_subtype(
                dev,
                "AS_GENERAL",
                &defs::DESC_AUDIO_AS_INTERFACE,
                buf,
                protocol,
                4,
            );
        }
        0x02 => {
            // FORMAT_TYPE
            println!("(FORMAT_TYPE)");
            if buf[0] < 4 {
                too_short();
                return;
            }
            match protocol {
                USB_AUDIO_CLASS_1 => {
                    if buf[0] < 8 {
                        too_short();
                        return;
                    }
                    print!("        bFormatType         {:5} ", buf[3]);
                    match buf[3] {
                        0x01 => {
                            println!("(FORMAT_TYPE_I)");
                            dump_uac1_format_type_i_iii(buf);
                        }
                        0x02 => {
                            println!("(FORMAT_TYPE_II)");
                            if buf[0] < 9 {
                                too_short();
                                return;
                            }
                            let j = if buf[8] != 0 {
                                buf[8] as usize * 3 + 9
                            } else {
                                15
                            };
                            if (buf[0] as usize) < j {
                                too_short();
                                return;
                            }
                            println!(
                                "        wMaxBitRate         {:5}",
                                buf[4] as u32 | (buf[5] as u32) << 8
                            );
                            println!(
                                "        wSamplesPerFrame    {:5}",
                                buf[6] as u32 | (buf[7] as u32) << 8
                            );
                            println!(
                                "        bSamFreqType        {:5} {}",
                                buf[8],
                                if buf[8] != 0 {
                                    "Discrete"
                                } else {
                                    "Continuous"
                                }
                            );
                            if buf[8] == 0 {
                                println!("        tLowerSamFreq     {:7}", freq3(buf, 9));
                                println!("        tUpperSamFreq     {:7}", freq3(buf, 12));
                            } else {
                                for i in 0..buf[8] as usize {
                                    println!(
                                        "        tSamFreq[{:2}]      {:7}",
                                        i,
                                        freq3(buf, 9 + 3 * i)
                                    );
                                }
                            }
                            dump_junk(buf, "        ", j);
                        }
                        0x03 => {
                            println!("(FORMAT_TYPE_III)");
                            dump_uac1_format_type_i_iii(buf);
                        }
                        _ => {
                            print!("(unknown)\n        Invalid desc format type:");
                            dump_bytes(&buf[4..], buf[0] as usize - 4);
                        }
                    }
                }
                USB_AUDIO_CLASS_2 => {
                    print!("        bFormatType         {:5} ", buf[3]);
                    match buf[3] {
                        0x01 | 0x03 => {
                            if buf[3] == 0x01 {
                                println!("(FORMAT_TYPE_I)");
                            } else {
                                println!("(FORMAT_TYPE_III)");
                            }
                            if buf[0] < 6 {
                                too_short();
                                return;
                            }
                            println!("        bSubslotSize        {:5}", buf[4]);
                            println!("        bBitResolution      {:5}", buf[5]);
                            dump_junk(buf, "        ", 6);
                        }
                        0x02 => {
                            println!("(FORMAT_TYPE_II)");
                            if buf[0] < 8 {
                                too_short();
                                return;
                            }
                            println!(
                                "        wMaxBitRate         {:5}",
                                buf[4] as u32 | (buf[5] as u32) << 8
                            );
                            println!(
                                "        wSlotsPerFrame      {:5}",
                                buf[6] as u32 | (buf[7] as u32) << 8
                            );
                            dump_junk(buf, "        ", 8);
                        }
                        0x04 => {
                            println!("(FORMAT_TYPE_IV)");
                            println!("        bFormatType         {:5}", buf[3]);
                            dump_junk(buf, "        ", 4);
                        }
                        _ => {
                            print!("(unknown)\n        Invalid desc format type:");
                            dump_bytes(&buf[4..], buf[0] as usize - 4);
                        }
                    }
                }
                _ => {}
            }
        }
        0x03 => {
            // FORMAT_SPECIFIC
            println!("(FORMAT_SPECIFIC)");
            if buf[0] < 5 {
                too_short();
                return;
            }
            let fmttag = buf[3] as u32 | (buf[4] as u32) << 8;
            let fmtptr = match fmttag {
                0..=5 => FMT_I_TAG[fmttag as usize],
                0x1000..=0x1002 => FMT_II_TAG[(fmttag & 0xfff) as usize],
                0x2000..=0x2006 => FMT_III_TAG[(fmttag & 0xfff) as usize],
                _ => "undefined",
            };
            println!("        wFormatTag          {:5} {}", fmttag, fmtptr);
            match fmttag {
                0x1001 => {
                    // MPEG
                    if buf[0] < 8 {
                        too_short();
                        return;
                    }
                    println!(
                        "        bmMPEGCapabilities 0x{:04x}",
                        buf[5] as u32 | (buf[6] as u32) << 8
                    );
                    const CAPS: [&str; 8] = [
                        "Layer I",
                        "Layer II",
                        "Layer III",
                        "MPEG-1 only",
                        "MPEG-1 dual-channel",
                        "MPEG-2 second stereo",
                        "MPEG-2 7.1 channel augmentation",
                        "Adaptive multi-channel prediction",
                    ];
                    for (bit, s) in CAPS.iter().enumerate() {
                        if buf[5] & (1 << bit) != 0 {
                            println!("          {s}");
                        }
                    }
                    print!("          MPEG-2 multilingual support: ");
                    match buf[6] & 3 {
                        0 => println!("Not supported"),
                        1 => println!("Supported at Fs"),
                        2 => println!("Reserved"),
                        _ => println!("Supported at Fs and 1/2Fs"),
                    }
                    println!("        bmMPEGFeatures       0x{:02x}", buf[7]);
                    print_drc(buf[7]);
                    dump_junk(buf, "        ", 8);
                }
                0x1002 => {
                    // AC-3
                    if buf[0] < 10 {
                        too_short();
                        return;
                    }
                    println!("        bmBSID         0x{:08x}", le32(&buf[5..]));
                    println!("        bmAC3Features        0x{:02x}", buf[9]);
                    const FEATURES: [&str; 4] =
                        ["RF mode", "Line mode", "Custom0 mode", "Custom1 mode"];
                    for (bit, s) in FEATURES.iter().enumerate() {
                        if buf[9] & (1 << bit) != 0 {
                            println!("          {s}");
                        }
                    }
                    print_drc(buf[9]);
                    dump_junk(buf, "        ", 8);
                }
                _ => {
                    print!("(unknown)\n        Invalid desc format type:");
                    dump_bytes(&buf[4..], buf[0] as usize - 4);
                }
            }
        }
        _ => {
            print!("        Invalid desc subtype:");
            if buf[0] > 3 {
                dump_bytes(&buf[3..], buf[0] as usize - 3);
            } else {
                println!();
            }
        }
    }
}

pub(super) fn dump_audiostreaming_endpoint(dev: Option<&Handle>, buf: &[u8], protocol: u8) {
    const SUBTYPE: [&str; 2] = ["invalid", "EP_GENERAL"];

    if buf[1] != USB_DT_CS_ENDPOINT {
        println!("      Warning: Invalid descriptor");
    }
    if buf[0] < 3 {
        too_short();
        return;
    }

    println!("        AudioStreaming Endpoint Descriptor:");
    println!("          bLength             {:5}", buf[0]);
    println!("          bDescriptorType     {:5}", buf[1]);
    print!("          bDescriptorSubtype  {:5} ", buf[2]);

    dump_audio_subtype(
        dev,
        SUBTYPE[(buf[2] == 1) as usize],
        &defs::DESC_AUDIO_AS_ISOCHRONOUS_AUDIO_DATA_ENDPOINT,
        buf,
        protocol,
        5,
    );
}

pub(super) fn dump_midistreaming_interface(dev: Option<&Handle>, buf: &[u8]) {
    if buf[1] != USB_DT_CS_INTERFACE {
        println!("      Warning: Invalid descriptor");
    }
    if buf[0] < 3 {
        too_short();
        return;
    }
    println!("      MIDIStreaming Interface Descriptor:");
    println!("        bLength             {:5}", buf[0]);
    println!("        bDescriptorType     {:5}", buf[1]);
    print!("        bDescriptorSubtype  {:5} ", buf[2]);

    let (name, desc) = match buf[2] {
        0x01 => ("HEADER", defs::DESC_MIDI_MS_HEADER),
        0x02 => ("MIDI_IN_JACK", defs::DESC_MIDI_MS_IN_JACK),
        0x03 => ("MIDI_OUT_JACK", defs::DESC_MIDI_MS_OUT_JACK),
        0x04 => ("ELEMENT", defs::DESC_MIDI_MS_ELEMENT),
        _ => {
            print!("(unknown)\n        Invalid desc subtype:");
            dump_bytes(&buf[3..], buf[0] as usize - 3);
            return;
        }
    };
    println!("({name})");
    desc_dump(dev, desc, &buf[3..], buf[0] as u32 - 3, 4);
}

pub(super) fn dump_midistreaming_endpoint(dev: Option<&Handle>, buf: &[u8]) {
    const SUBTYPES: [&str; 3] = ["invalid", "GENERAL", "GENERAL_2_0"];

    if buf[1] != USB_DT_CS_ENDPOINT {
        println!("      Warning: Invalid descriptor");
    }
    if buf[0] < 3 {
        too_short();
        return;
    }
    println!("        MIDIStreaming Endpoint Descriptor:");
    println!("          bLength             {:5}", buf[0]);
    println!("          bDescriptorType     {:5}", buf[1]);
    println!(
        "          bDescriptorSubtype  {:5} ({})",
        buf[2],
        SUBTYPES[if buf[2] < 3 { buf[2] as usize } else { 0 }]
    );

    desc_dump(
        dev,
        defs::DESC_MIDI_MS_ENDPOINT_GENERAL,
        &buf[3..],
        buf[0] as u32 - 3,
        5,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uac_subtype_mapping() {
        assert!(get_uac_interface_subtype(0x07, USB_AUDIO_CLASS_1) == UacSubtype::ProcessingUnit);
        assert!(get_uac_interface_subtype(0x07, USB_AUDIO_CLASS_2) == UacSubtype::EffectUnit);
        assert!(get_uac_interface_subtype(0x07, USB_AUDIO_CLASS_3) == UacSubtype::FeatureUnit);
        assert!(get_uac_interface_subtype(0x0a, USB_AUDIO_CLASS_2) == UacSubtype::ClockSource);
        assert!(get_uac_interface_subtype(0x0a, USB_AUDIO_CLASS_3) == UacSubtype::ExtensionUnit);
        // Unmapped UAC1 values fall back to the UAC3 numbering.
        assert!(get_uac_interface_subtype(0x0b, USB_AUDIO_CLASS_1) == UacSubtype::ClockSource);
        assert!(get_uac_interface_subtype(0x01, 0x42) == UacSubtype::Header);
        assert!(get_uac_interface_subtype(0x11, USB_AUDIO_CLASS_3) == UacSubtype::Unknown);
    }
}
