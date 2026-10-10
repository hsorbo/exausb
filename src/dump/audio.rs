// SPDX-License-Identifier: GPL-2.0-or-later
//! Audio and MIDI class descriptor dumping.

use super::{junk, too_short, USB_DT_CS_ENDPOINT, USB_DT_CS_INTERFACE};
use crate::desc_defs::{self as defs, AudioDescs};
use crate::desc_dump::desc_dump;
use crate::model::{hex2, hex4, hex8, hex_bytes, Node};
use crate::usb::{le16, le32, Handle};

const USB_AUDIO_CLASS_1: u8 = 0x00;
const USB_AUDIO_CLASS_2: u8 = 0x20;
/// USB DCD for Audio Devices Release 3.0: Section A.6, pp139
const USB_AUDIO_CLASS_3: u8 = 0x30;

/// Add a section for a class-specific descriptor with its common header
/// fields.  Returns `None` (after a warning) if it's too short for them.
fn cs_section<'a>(node: &'a mut Node, title: &str, ty: u8, buf: &[u8]) -> Option<&'a mut Node> {
    let s = node.section(title);
    if buf[1] != ty {
        s.warn("Invalid descriptor type");
    }
    if buf[0] < 3 {
        too_short(s);
        return None;
    }
    s.field("bLength", buf[0]);
    s.field("bDescriptorType", buf[1]);
    Some(s)
}

/// Add `bDescriptorSubtype`, naming the subtype on the field and section.
fn subtype(buf: &[u8], name: &str, s: &mut Node) {
    s.note(name);
    s.field("bDescriptorSubtype", buf[2]).note(name);
}

/// Add `bFormatType`, naming the format type on the field and section.
fn format_type(buf: &[u8], name: &str, s: &mut Node) {
    s.note(name);
    s.field("bFormatType", buf[3]).note(name);
}

/// Warn about an invalid `what`, showing the undecoded bytes.
fn invalid(what: &str, bytes: &[u8], s: &mut Node) {
    if bytes.is_empty() {
        s.warn(format!("Invalid {what}"));
    } else {
        s.warn(format!("Invalid {what}: {}", hex_bytes(bytes)));
    }
}

fn dump_audio_subtype(
    dev: Option<&Handle>,
    name: &str,
    desc: &AudioDescs,
    buf: &[u8],
    protocol: u8,
    s: &mut Node,
) {
    const STRINGS: [&str; 3] = ["UAC1", "UAC2", "UAC3"];
    let idx = match protocol {
        USB_AUDIO_CLASS_2 => 1,
        USB_AUDIO_CLASS_3 => 2,
        _ => 0,
    };

    subtype(buf, name, s);

    let Some(d) = desc[idx] else {
        s.warn(format!(
            "{name} descriptors are illegal for {}",
            STRINGS[idx]
        ));
        return;
    };

    // Skip the first three bytes; those common fields have already been
    // dumped.
    desc_dump(dev, d, &buf[3..], buf[0] as u32 - 3, s);
}

/// USB Audio Class AudioControl interface subtypes.
#[derive(Debug, PartialEq, Eq)]
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

pub(super) fn dump_audiocontrol_interface(
    dev: Option<&Handle>,
    buf: &[u8],
    protocol: u8,
    node: &mut Node,
) {
    use UacSubtype::*;

    let Some(s) = cs_section(node, "AudioControl Interface", USB_DT_CS_INTERFACE, buf) else {
        return;
    };

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
            subtype(buf, "unknown", s);
            invalid("descriptor subtype", &buf[3..buf[0] as usize], s);
            return;
        }
    };
    dump_audio_subtype(dev, name, desc, buf, protocol, s);
}

/// A 3-byte little-endian sample frequency.
fn freq3(buf: &[u8], at: usize) -> u32 {
    buf[at] as u32 | (buf[at + 1] as u32) << 8 | (buf[at + 2] as u32) << 16
}

/// `bSamFreqType` at `at`, followed by the sample frequencies.
fn dump_sam_freqs(buf: &[u8], at: usize, s: &mut Node) {
    let n = buf[at];
    s.field("bSamFreqType", n)
        .note(if n != 0 { "Discrete" } else { "Continuous" });
    if n == 0 {
        s.field("tLowerSamFreq", format!("{}Hz", freq3(buf, at + 1)));
        s.field("tUpperSamFreq", format!("{}Hz", freq3(buf, at + 4)));
    } else {
        for i in 0..n as usize {
            s.field(
                format!("tSamFreq[{i}]"),
                format!("{}Hz", freq3(buf, at + 1 + 3 * i)),
            );
        }
    }
}

/// UAC1 FORMAT_TYPE_I and FORMAT_TYPE_III share a layout.
fn dump_uac1_format_type_i_iii(buf: &[u8], s: &mut Node) {
    let j = if buf[7] != 0 {
        buf[7] as usize * 3 + 8
    } else {
        14
    };
    if (buf[0] as usize) < j {
        too_short(s);
        return;
    }
    s.field("bNrChannels", buf[4]);
    s.field("bSubframeSize", buf[5]);
    s.field("bBitResolution", buf[6]);
    dump_sam_freqs(buf, 7, s);
    junk(buf, j, s);
}

fn dump_uac1_format_type(buf: &[u8], s: &mut Node) {
    match buf[3] {
        0x01 => {
            format_type(buf, "FORMAT_TYPE_I", s);
            dump_uac1_format_type_i_iii(buf, s);
        }
        0x02 => {
            format_type(buf, "FORMAT_TYPE_II", s);
            if buf[0] < 9 {
                too_short(s);
                return;
            }
            let j = if buf[8] != 0 {
                buf[8] as usize * 3 + 9
            } else {
                15
            };
            if (buf[0] as usize) < j {
                too_short(s);
                return;
            }
            s.field("wMaxBitRate", le16(&buf[4..]));
            s.field("wSamplesPerFrame", le16(&buf[6..]));
            dump_sam_freqs(buf, 8, s);
            junk(buf, j, s);
        }
        0x03 => {
            format_type(buf, "FORMAT_TYPE_III", s);
            dump_uac1_format_type_i_iii(buf, s);
        }
        _ => {
            format_type(buf, "unknown", s);
            invalid("format type", &buf[4..buf[0] as usize], s);
        }
    }
}

fn dump_uac2_format_type(buf: &[u8], s: &mut Node) {
    match buf[3] {
        0x01 | 0x03 => {
            let name = if buf[3] == 0x01 {
                "FORMAT_TYPE_I"
            } else {
                "FORMAT_TYPE_III"
            };
            format_type(buf, name, s);
            if buf[0] < 6 {
                too_short(s);
                return;
            }
            s.field("bSubslotSize", buf[4]);
            s.field("bBitResolution", buf[5]);
            junk(buf, 6, s);
        }
        0x02 => {
            format_type(buf, "FORMAT_TYPE_II", s);
            if buf[0] < 8 {
                too_short(s);
                return;
            }
            s.field("wMaxBitRate", le16(&buf[4..]));
            s.field("wSlotsPerFrame", le16(&buf[6..]));
            junk(buf, 8, s);
        }
        0x04 => {
            format_type(buf, "FORMAT_TYPE_IV", s);
            junk(buf, 4, s);
        }
        _ => {
            format_type(buf, "unknown", s);
            invalid("format type", &buf[4..buf[0] as usize], s);
        }
    }
}

/// The "Internal Dynamic Range Control" support in bmMPEGFeatures and
/// bmAC3Features.
fn drc(features: u8) -> String {
    let support = match (features >> 4) & 3 {
        0 => "not supported",
        1 => "supported but not scalable",
        2 => "scalable, common boost and cut scaling value",
        _ => "scalable, separate boost and cut scaling value",
    };
    format!("Internal Dynamic Range Control: {support}")
}

fn dump_format_specific(buf: &[u8], s: &mut Node) {
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

    subtype(buf, "FORMAT_SPECIFIC", s);
    if buf[0] < 5 {
        too_short(s);
        return;
    }
    let fmttag = le16(&buf[3..]);
    let fmtptr = match fmttag {
        0..=5 => FMT_I_TAG[fmttag as usize],
        0x1000..=0x1002 => FMT_II_TAG[(fmttag & 0xfff) as usize],
        0x2000..=0x2006 => FMT_III_TAG[(fmttag & 0xfff) as usize],
        _ => "undefined",
    };
    s.field("wFormatTag", fmttag).note(fmtptr);
    match fmttag {
        0x1001 => {
            // MPEG
            if buf[0] < 8 {
                too_short(s);
                return;
            }
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
            let caps = s.field("bmMPEGCapabilities", hex4(le16(&buf[5..])));
            for (bit, name) in CAPS.iter().enumerate() {
                if buf[5] & (1 << bit) != 0 {
                    caps.text(*name);
                }
            }
            caps.text(format!(
                "MPEG-2 multilingual support: {}",
                match buf[6] & 3 {
                    0 => "Not supported",
                    1 => "Supported at Fs",
                    2 => "Reserved",
                    _ => "Supported at Fs and 1/2Fs",
                }
            ));
            s.field("bmMPEGFeatures", hex2(buf[7])).text(drc(buf[7]));
            junk(buf, 8, s);
        }
        0x1002 => {
            // AC-3
            if buf[0] < 10 {
                too_short(s);
                return;
            }
            const FEATURES: [&str; 4] = ["RF mode", "Line mode", "Custom0 mode", "Custom1 mode"];
            s.field("bmBSID", hex8(le32(&buf[5..])));
            let features = s.field("bmAC3Features", hex2(buf[9]));
            for (bit, name) in FEATURES.iter().enumerate() {
                if buf[9] & (1 << bit) != 0 {
                    features.text(*name);
                }
            }
            features.text(drc(buf[9]));
            junk(buf, 10, s);
        }
        _ => invalid("format type", &buf[4..buf[0] as usize], s),
    }
}

pub(super) fn dump_audiostreaming_interface(
    dev: Option<&Handle>,
    buf: &[u8],
    protocol: u8,
    node: &mut Node,
) {
    let Some(s) = cs_section(node, "AudioStreaming Interface", USB_DT_CS_INTERFACE, buf) else {
        return;
    };

    match buf[2] {
        0x01 => {
            // AS_GENERAL
            dump_audio_subtype(
                dev,
                "AS_GENERAL",
                &defs::DESC_AUDIO_AS_INTERFACE,
                buf,
                protocol,
                s,
            );
        }
        0x02 => {
            // FORMAT_TYPE
            subtype(buf, "FORMAT_TYPE", s);
            if buf[0] < 4 {
                too_short(s);
                return;
            }
            match protocol {
                USB_AUDIO_CLASS_1 => {
                    if buf[0] < 8 {
                        too_short(s);
                        return;
                    }
                    dump_uac1_format_type(buf, s);
                }
                USB_AUDIO_CLASS_2 => dump_uac2_format_type(buf, s),
                _ => {}
            }
        }
        0x03 => dump_format_specific(buf, s),
        _ => {
            s.field("bDescriptorSubtype", buf[2]);
            invalid("descriptor subtype", &buf[3..buf[0] as usize], s);
        }
    }
}

pub(super) fn dump_audiostreaming_endpoint(
    dev: Option<&Handle>,
    buf: &[u8],
    protocol: u8,
    node: &mut Node,
) {
    const SUBTYPE: [&str; 2] = ["invalid", "EP_GENERAL"];

    let Some(s) = cs_section(node, "AudioStreaming Endpoint", USB_DT_CS_ENDPOINT, buf) else {
        return;
    };

    dump_audio_subtype(
        dev,
        SUBTYPE[(buf[2] == 1) as usize],
        &defs::DESC_AUDIO_AS_ISOCHRONOUS_AUDIO_DATA_ENDPOINT,
        buf,
        protocol,
        s,
    );
}

pub(super) fn dump_midistreaming_interface(dev: Option<&Handle>, buf: &[u8], node: &mut Node) {
    let Some(s) = cs_section(node, "MIDIStreaming Interface", USB_DT_CS_INTERFACE, buf) else {
        return;
    };

    let (name, desc) = match buf[2] {
        0x01 => ("HEADER", defs::DESC_MIDI_MS_HEADER),
        0x02 => ("MIDI_IN_JACK", defs::DESC_MIDI_MS_IN_JACK),
        0x03 => ("MIDI_OUT_JACK", defs::DESC_MIDI_MS_OUT_JACK),
        0x04 => ("ELEMENT", defs::DESC_MIDI_MS_ELEMENT),
        _ => {
            subtype(buf, "unknown", s);
            invalid("descriptor subtype", &buf[3..buf[0] as usize], s);
            return;
        }
    };
    subtype(buf, name, s);
    desc_dump(dev, desc, &buf[3..], buf[0] as u32 - 3, s);
}

pub(super) fn dump_midistreaming_endpoint(dev: Option<&Handle>, buf: &[u8], node: &mut Node) {
    const SUBTYPES: [&str; 3] = ["invalid", "GENERAL", "GENERAL_2_0"];

    let Some(s) = cs_section(node, "MIDIStreaming Endpoint", USB_DT_CS_ENDPOINT, buf) else {
        return;
    };

    subtype(
        buf,
        SUBTYPES[if buf[2] < 3 { buf[2] as usize } else { 0 }],
        s,
    );
    desc_dump(
        dev,
        defs::DESC_MIDI_MS_ENDPOINT_GENERAL,
        &buf[3..],
        buf[0] as u32 - 3,
        s,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usb;

    #[test]
    fn uac_subtype_mapping() {
        use UacSubtype::*;
        let cases = [
            (0x07, USB_AUDIO_CLASS_1, ProcessingUnit),
            (0x07, USB_AUDIO_CLASS_2, EffectUnit),
            (0x07, USB_AUDIO_CLASS_3, FeatureUnit),
            (0x0a, USB_AUDIO_CLASS_2, ClockSource),
            (0x0a, USB_AUDIO_CLASS_3, ExtensionUnit),
            // Unmapped UAC1 values fall back to the UAC3 numbering.
            (0x0b, USB_AUDIO_CLASS_1, ClockSource),
            (0x01, 0x42, Header),
            (0x11, USB_AUDIO_CLASS_3, Unknown),
        ];
        for (c, protocol, expected) in cases {
            assert_eq!(get_uac_interface_subtype(c, protocol), expected);
        }
    }

    #[test]
    fn uac1_format_type_i() {
        // 2 channels, 2-byte subframes, 16 bits, discrete 44.1 and 48 kHz.
        let buf = usb::padded(&[
            14, 0x24, 0x02, 0x01, 2, 2, 16, 2, 0x44, 0xac, 0x00, 0x80, 0xbb, 0x00,
        ]);
        let mut node = Node::new("dev");
        dump_audiostreaming_interface(None, &buf, USB_AUDIO_CLASS_1, &mut node);
        let s = node.get("AudioStreaming Interface").unwrap();
        assert_eq!(s.note, "FORMAT_TYPE_I");
        assert_eq!(s.get("bDescriptorSubtype").unwrap().note, "FORMAT_TYPE");
        assert_eq!(s.get("bNrChannels").unwrap().value, "2");
        assert_eq!(s.get("bSamFreqType").unwrap().note, "Discrete");
        assert_eq!(s.get("tSamFreq[0]").unwrap().value, "44100Hz");
        assert_eq!(s.get("tSamFreq[1]").unwrap().value, "48000Hz");
        assert!(!s.has_warnings());
    }

    #[test]
    fn short_descriptor() {
        let buf = usb::padded(&[2, 0x24]);
        let mut node = Node::new("dev");
        dump_midistreaming_interface(None, &buf, &mut node);
        let s = node.get("MIDIStreaming Interface").unwrap();
        assert!(s.has_warnings());
        assert!(s.get("bLength").is_none());
    }
}
