// SPDX-License-Identifier: GPL-2.0-or-later
//! USB descriptor definitions.
//!
//! Derived from usbutils' `desc-defs.c` / `desc-defs.h`
//! (Copyright (C) 2017-2018 Michael Drake <michael.drake@codethink.co.uk>).
//!
//! Descriptors are described as slices of [`Desc`] field definitions, which
//! are interpreted by [`crate::desc_dump::desc_dump`].

use crate::model::Node;

/// Callback for [`DescType::Snowflake`] fields: annotates the field node
/// (its value is already set) with a note and/or child lines.
pub type SnowflakeFn = fn(value: u64, field: &mut Node);

/// Descriptor field value type.
#[derive(Clone, Copy)]
pub enum DescType {
    /// Plain numerical value; no annotation.
    Constant,
    /// Plain numerical value; no annotation.
    Number,
    /// Number with a postfix string.
    NumberPostfix(&'static str),
    /// Plain hex rendered value; no annotation.
    Bitmap,
    /// Binary coded decimal.
    Bcd,
    /// UAC1 style bmControl field (one bit per string).  An empty string
    /// means "no name for this bit".
    BmControl1(&'static [&'static str]),
    /// UAC2/UAC3 style bmControl field (two bits per string).  An empty
    /// string means "no name for this control".
    BmControl2(&'static [&'static str]),
    /// String descriptor index.
    StrDescIndex,
    /// UAC3 style class-specific string request.
    CsStrDescId,
    /// Audio terminal string.
    TerminalStr,
    /// Bitfield with a string per bit; `None` entries are skipped.  The slice
    /// length is the number of bits that have meaning.
    BitmapStrings(&'static [Option<&'static str>]),
    /// Enum-style value to string.
    NumberStrings(&'static [&'static str]),
    /// The value is processed by another descriptor definition, selected by
    /// the value of the field named `type_field`.
    Extension {
        type_field: &'static str,
        d: &'static [DescExt],
    },
    /// Value with a custom annotation callback function.
    Snowflake(SnowflakeFn),
}

/// Extension descriptor definition, selected when the `type_field` value
/// equals `ty`.
pub struct DescExt {
    pub desc: &'static [Desc],
    pub ty: u32,
}

/// Array-specific parts of a field definition.
#[derive(Clone, Copy)]
pub struct DescArray {
    /// True if entry is an array.
    pub array: bool,
    /// True if array length is specified in bits.
    pub bits: bool,
    /// Name of field specifying the array entry count.
    pub length_field1: Option<&'static str>,
    /// Name of field specifying multiplier for array entry count.
    pub length_field2: Option<&'static str>,
}

impl DescArray {
    pub const NONE: DescArray = DescArray {
        array: false,
        bits: false,
        length_field1: None,
        length_field2: None,
    };
}

/// Descriptor field definition.
pub struct Desc {
    /// Field's name.
    pub field: &'static str,
    /// Byte size of field, if `size_field` is `None`.
    pub size: u32,
    /// Name of field specifying field size.
    pub size_field: Option<&'static str>,
    /// Field's value type.
    pub ty: DescType,
    /// Array information.
    pub array: DescArray,
}

impl Desc {
    /// A fixed-size field.
    pub const fn new(field: &'static str, size: u32, ty: DescType) -> Desc {
        Desc {
            field,
            size,
            size_field: None,
            ty,
            array: DescArray::NONE,
        }
    }

    /// A field whose byte size is given by the value of another field.
    pub const fn sized_by(field: &'static str, size_field: &'static str, ty: DescType) -> Desc {
        Desc {
            field,
            size: 0,
            size_field: Some(size_field),
            ty,
            array: DescArray::NONE,
        }
    }

    /// Make this an array whose length is inferred from the descriptor length.
    pub const fn array(mut self) -> Desc {
        self.array = DescArray {
            array: true,
            bits: false,
            length_field1: None,
            length_field2: None,
        };
        self
    }

    /// Make this an array whose entry count is given by another field.
    pub const fn array_len(mut self, length_field1: &'static str) -> Desc {
        self.array = DescArray {
            array: true,
            bits: false,
            length_field1: Some(length_field1),
            length_field2: None,
        };
        self
    }

    /// Make this an array whose entry count is the product of two fields.
    #[allow(dead_code)]
    pub const fn array_len2(
        mut self,
        length_field1: &'static str,
        length_field2: &'static str,
    ) -> Desc {
        self.array = DescArray {
            array: true,
            bits: false,
            length_field1: Some(length_field1),
            length_field2: Some(length_field2),
        };
        self
    }

    /// Make this an array whose length in *bits* is the product of two fields.
    pub const fn array_bits(
        mut self,
        length_field1: &'static str,
        length_field2: &'static str,
    ) -> Desc {
        self.array = DescArray {
            array: true,
            bits: true,
            length_field1: Some(length_field1),
            length_field2: Some(length_field2),
        };
        self
    }
}

/// Descriptor definitions for the three Audio Device Class protocols
/// (UAC1, UAC2, UAC3).  `None` means the descriptor is illegal for that
/// protocol version.
pub type AudioDescs = [Option<&'static [Desc]>; 3];

// ---------------------------------------------------------------------------
// Descriptor tables (translated from desc-defs.c) follow.
// ---------------------------------------------------------------------------

/// Undefined descriptor
///
/// Ensures remaining data is dumped as garbage at end of descriptor.
pub static DESC_UNDEFINED: &[Desc] = &[];

/// USB Audio Device Class 1 Channel Names.  (Note: Order matters.)
///
/// Used as `.bitmap_strings = { .strings = uac1_channel_names, .count = 12 }`.
static UAC1_CHANNEL_NAMES: &[Option<&str>] = &[
    Some("Left Front (L)"),
    Some("Right Front (R)"),
    Some("Center Front (C)"),
    Some("Low Frequency Enhancement (LFE)"),
    Some("Left Surround (LS)"),
    Some("Right Surround (RS)"),
    Some("Left of Center (LC)"),
    Some("Right of Center (RC)"),
    Some("Surround (S)"),
    Some("Side Left (SL)"),
    Some("Side Right (SR)"),
    Some("Top (T)"),
];

/// USB Audio Device Class 2 Channel Names.  (Note: Order matters.)
///
/// Used as `.bitmap_strings = { .strings = uac2_channel_names, .count = 26 }`.
/// NOTE: The C array has 27 entries but every user passes `.count = 26`, so
/// the last entry ("Back Right of Center (BRC)") is never rendered and is
/// omitted here.
static UAC2_CHANNEL_NAMES: &[Option<&str>] = &[
    Some("Front Left (FL)"),
    Some("Front Right (FR)"),
    Some("Front Center (FC)"),
    Some("Low Frequency Effects (LFE)"),
    Some("Back Left (BL)"),
    Some("Back Right (BR)"),
    Some("Front Left of Center (FLC)"),
    Some("Front Right of Center (FRC)"),
    Some("Back Center (BC)"),
    Some("Side Left (SL)"),
    Some("Side Right (SR)"),
    Some("Top Center (TC)"),
    Some("Top Front Left (TFL)"),
    Some("Top Front Center (TFC)"),
    Some("Top Front Right (TFR)"),
    Some("Top Back Left (TBL)"),
    Some("Top Back Center (TBC)"),
    Some("Top Back Right (TBR)"),
    Some("Top Front Left of Center (TFLC)"),
    Some("Top Front Right of Center (TFRC)"),
    Some("Left Low Frequency Effects (LLFE)"),
    Some("Right Low Frequency Effects (RLFE)"),
    Some("Top Side Left (TSL)"),
    Some("Top Side Right (TSR)"),
    Some("Bottom Center (BC)"),
    Some("Back Left of Center (BLC)"),
    // "Back Right of Center (BRC)" -- index 26, beyond .count = 26
];

/// Audio Control Interface Header bmControls; Human readable bit meanings.
static UAC2_INTERFACE_HEADER_BMCONTROLS: &[&str] = &["Latency control"];

/// UAC1: 4.3.2 Class-Specific AC Interface Descriptor; Table 4-2.
static DESC_AUDIO_1_AC_HEADER: &[Desc] = &[
    Desc::new("bcdADC", 2, DescType::Bcd),
    Desc::new("wTotalLength", 2, DescType::Constant),
    Desc::new("bInCollection", 1, DescType::Constant),
    Desc::new("baInterfaceNr", 1, DescType::Number).array(),
];

/// UAC2: 4.7.2 Class-Specific AC Interface Descriptor; Table 4-5.
static DESC_AUDIO_2_AC_HEADER: &[Desc] = &[
    Desc::new("bcdADC", 2, DescType::Bcd),
    Desc::new("bCategory", 1, DescType::Constant),
    Desc::new("wTotalLength", 2, DescType::Number),
    Desc::new(
        "bmControls",
        1,
        DescType::BmControl2(UAC2_INTERFACE_HEADER_BMCONTROLS),
    ),
];

/// UAC3: 4.5.2 Class-Specific AC Interface Descriptor; Table 4-15.
static DESC_AUDIO_3_AC_HEADER: &[Desc] = &[
    Desc::new("bCategory", 1, DescType::Constant),
    Desc::new("wTotalLength", 2, DescType::Number),
    Desc::new(
        "bmControls",
        4,
        DescType::BmControl2(UAC2_INTERFACE_HEADER_BMCONTROLS),
    ),
];

/// AudioControl Header descriptor definitions for the three Audio Device Class protocols
pub static DESC_AUDIO_AC_HEADER: AudioDescs = [
    Some(DESC_AUDIO_1_AC_HEADER),
    Some(DESC_AUDIO_2_AC_HEADER),
    Some(DESC_AUDIO_3_AC_HEADER),
];

/// UAC2: 4.7.2.10 Effect Unit Descriptor; Table 4-15.
static DESC_AUDIO_2_AC_EFFECT_UNIT: &[Desc] = &[
    Desc::new("bUnitID", 1, DescType::Number),
    Desc::new("wEffectType", 2, DescType::Constant),
    Desc::new("bSourceID", 1, DescType::Constant),
    Desc::new("bmaControls", 4, DescType::Bitmap).array(),
    Desc::new("iEffects", 1, DescType::StrDescIndex),
];

/// UAC3: 4.5.2.9 Effect Unit Descriptor; Table 4-33.
static DESC_AUDIO_3_AC_EFFECT_UNIT: &[Desc] = &[
    Desc::new("bUnitID", 1, DescType::Number),
    Desc::new("wEffectType", 2, DescType::Constant),
    Desc::new("bSourceID", 1, DescType::Constant),
    Desc::new("bmaControls", 4, DescType::Bitmap).array(),
    Desc::new("wEffectsDescrStr", 2, DescType::CsStrDescId),
];

/// Effect Unit descriptor definitions for the three Audio Device Class protocols
pub static DESC_AUDIO_AC_EFFECT_UNIT: AudioDescs = [
    None, // UAC1 not supported
    Some(DESC_AUDIO_2_AC_EFFECT_UNIT),
    Some(DESC_AUDIO_3_AC_EFFECT_UNIT),
];

/// UAC2 Input Terminal bmControls; Human readable bit meanings.
static UAC2_INPUT_TERM_BMCONTROLS: &[&str] = &[
    "Copy Protect",
    "Connector",
    "Overload",
    "Cluster",
    "Underflow",
    "Overflow",
];

/// UAC3 Input Terminal bmControls; Human readable bit meanings.
static UAC3_INPUT_TERM_BMCONTROLS: &[&str] = &[
    "Insertion",
    "Overload",
    "Underflow",
    "Overflow",
    "Underflow",
];

/// UAC1: 4.3.2.1 Input Terminal Descriptor; Table 4-3.
static DESC_AUDIO_1_AC_INPUT_TERMINAL: &[Desc] = &[
    Desc::new("bTerminalID", 1, DescType::Number),
    Desc::new("wTerminalType", 2, DescType::TerminalStr),
    Desc::new("bAssocTerminal", 1, DescType::Constant),
    Desc::new("bNrChannels", 1, DescType::Number),
    Desc::new(
        "wChannelConfig",
        2,
        DescType::BitmapStrings(UAC1_CHANNEL_NAMES),
    ),
    Desc::new("iChannelNames", 1, DescType::StrDescIndex),
    Desc::new("iTerminal", 1, DescType::StrDescIndex),
];

/// UAC2: 4.7.2.4 Input Terminal Descriptor; Table 4-9.
static DESC_AUDIO_2_AC_INPUT_TERMINAL: &[Desc] = &[
    Desc::new("bTerminalID", 1, DescType::Number),
    Desc::new("wTerminalType", 2, DescType::TerminalStr),
    Desc::new("bAssocTerminal", 1, DescType::Constant),
    Desc::new("bCSourceID", 1, DescType::Constant),
    Desc::new("bNrChannels", 1, DescType::Number),
    Desc::new(
        "bmChannelConfig",
        4,
        DescType::BitmapStrings(UAC2_CHANNEL_NAMES),
    ),
    Desc::new("iChannelNames", 1, DescType::StrDescIndex),
    Desc::new(
        "bmControls",
        2,
        DescType::BmControl2(UAC2_INPUT_TERM_BMCONTROLS),
    ),
    Desc::new("iTerminal", 1, DescType::StrDescIndex),
];

/// UAC3: 4.5.2.1 Input Terminal Descriptor; Table 4-16.
static DESC_AUDIO_3_AC_INPUT_TERMINAL: &[Desc] = &[
    Desc::new("bTerminalID", 1, DescType::Number),
    Desc::new("wTerminalType", 2, DescType::TerminalStr),
    Desc::new("bAssocTerminal", 1, DescType::Number),
    Desc::new("bCSourceID", 1, DescType::Number),
    Desc::new(
        "bmControls",
        4,
        DescType::BmControl2(UAC3_INPUT_TERM_BMCONTROLS),
    ),
    Desc::new("wClusterDescrID", 2, DescType::Number),
    Desc::new("wExTerminalDescrID", 2, DescType::Number),
    Desc::new("wConnectorsDescrID", 2, DescType::Number),
    Desc::new("wTerminalDescrStr", 2, DescType::CsStrDescId),
];

/// Input Terminal descriptor definitions for the three Audio Device Class protocols
pub static DESC_AUDIO_AC_INPUT_TERMINAL: AudioDescs = [
    Some(DESC_AUDIO_1_AC_INPUT_TERMINAL),
    Some(DESC_AUDIO_2_AC_INPUT_TERMINAL),
    Some(DESC_AUDIO_3_AC_INPUT_TERMINAL),
];

/// UAC2 Output Terminal bmControls; Human readable bit meanings.
static UAC2_OUTPUT_TERM_BMCONTROLS: &[&str] = &[
    "Copy Protect",
    "Connector",
    "Overload",
    "Underflow",
    "Overflow",
];

/// UAC3 Output Terminal bmControls; Human readable bit meanings.
static UAC3_OUTPUT_TERM_BMCONTROLS: &[&str] = &["Insertion", "Overload", "Underflow", "Overflow"];

/// UAC1: 4.3.2.2 Output Terminal Descriptor; Table 4-4.
static DESC_AUDIO_1_AC_OUTPUT_TERMINAL: &[Desc] = &[
    Desc::new("bTerminalID", 1, DescType::Number),
    Desc::new("wTerminalType", 2, DescType::TerminalStr),
    Desc::new("bAssocTerminal", 1, DescType::Number),
    Desc::new("bSourceID", 1, DescType::Number),
    Desc::new("iTerminal", 1, DescType::StrDescIndex),
];

/// UAC2: 4.7.2.5 Output Terminal Descriptor; Table 4-10.
static DESC_AUDIO_2_AC_OUTPUT_TERMINAL: &[Desc] = &[
    Desc::new("bTerminalID", 1, DescType::Number),
    Desc::new("wTerminalType", 2, DescType::TerminalStr),
    Desc::new("bAssocTerminal", 1, DescType::Number),
    Desc::new("bSourceID", 1, DescType::Number),
    Desc::new("bCSourceID", 1, DescType::Number),
    Desc::new(
        "bmControls",
        2,
        DescType::BmControl2(UAC2_OUTPUT_TERM_BMCONTROLS),
    ),
    Desc::new("iTerminal", 1, DescType::StrDescIndex),
];

/// UAC3: 4.5.2.2 Output Terminal Descriptor; Table 4-17.
static DESC_AUDIO_3_AC_OUTPUT_TERMINAL: &[Desc] = &[
    Desc::new("bTerminalID", 1, DescType::Number),
    Desc::new("wTerminalType", 2, DescType::TerminalStr),
    Desc::new("bAssocTerminal", 1, DescType::Number),
    Desc::new("bSourceID", 1, DescType::Number),
    Desc::new("bCSourceID", 1, DescType::Number),
    Desc::new(
        "bmControls",
        4,
        DescType::BmControl2(UAC3_OUTPUT_TERM_BMCONTROLS),
    ),
    Desc::new("wExTerminalDescrID", 2, DescType::Number),
    Desc::new("wConnectorsDescrID", 2, DescType::Number),
    Desc::new("wTerminalDescrStr", 2, DescType::CsStrDescId),
];

/// Output Terminal descriptor definitions for the three Audio Device Class protocols
pub static DESC_AUDIO_AC_OUTPUT_TERMINAL: AudioDescs = [
    Some(DESC_AUDIO_1_AC_OUTPUT_TERMINAL),
    Some(DESC_AUDIO_2_AC_OUTPUT_TERMINAL),
    Some(DESC_AUDIO_3_AC_OUTPUT_TERMINAL),
];

/// UAC3: 4.5.2.3.1 Extended Terminal Header Descriptor; Table 4-18.
#[allow(dead_code)]
static DESC_AUDIO_3_AC_EXTENDED_TERMINAL_HEADER: &[Desc] = &[
    Desc::new("wDescriptorID", 2, DescType::Number),
    Desc::new("bNrChannels", 1, DescType::Number),
];

/// Extended Terminal descriptor definitions for the three Audio Device Class protocols
// Unused: upstream never dumps this descriptor type either.
#[allow(dead_code)]
pub static DESC_AUDIO_AC_EXTENDED_TERMINAL: AudioDescs = [
    None, // UAC1 not supported
    None, // UAC2 not supported
    Some(DESC_AUDIO_3_AC_EXTENDED_TERMINAL_HEADER),
];

/// UAC3: 4.5.2.15 Power Domain Descriptor; Table 4-46.
static DESC_AUDIO_3_AC_POWER_DOMAIN: &[Desc] = &[
    Desc::new("bPowerDomainID", 1, DescType::Number),
    Desc::new("waRecoveryTime(1)", 2, DescType::Number),
    Desc::new("waRecoveryTime(2)", 2, DescType::Number),
    Desc::new("bNrEntities", 1, DescType::Number),
    Desc::new("baEntityID", 1, DescType::Number).array_len("bNrEntities"),
    Desc::new("wPDomainDescrStr", 2, DescType::CsStrDescId),
];

/// Power Domain descriptor definitions for the three Audio Device Class protocols
pub static DESC_AUDIO_AC_POWER_DOMAIN: AudioDescs = [
    None, // UAC1 not supported
    None, // UAC2 not supported
    Some(DESC_AUDIO_3_AC_POWER_DOMAIN),
];

/// UAC2 Mixer Unit bmControls; Human readable bit meanings.
static UAC2_MIXER_UNIT_BMCONTROLS: &[&str] = &["Cluster", "Underflow", "Overflow"];

/// UAC3 Mixer Unit bmControls; Human readable bit meanings.
static UAC3_MIXER_UNIT_BMCONTROLS: &[&str] = &["Underflow", "Overflow"];

/// UAC1: 4.3.2.3 Mixer Unit Descriptor; Table 4-5.
static DESC_AUDIO_1_AC_MIXER_UNIT: &[Desc] = &[
    Desc::new("bUnitID", 1, DescType::Number),
    Desc::new("bNrInPins", 1, DescType::Number),
    Desc::new("baSourceID", 1, DescType::Number).array_len("bNrInPins"),
    Desc::new("bNrChannels", 1, DescType::Number),
    Desc::new(
        "wChannelConfig",
        2,
        DescType::BitmapStrings(UAC1_CHANNEL_NAMES),
    ),
    Desc::new("iChannelNames", 1, DescType::StrDescIndex),
    Desc::new("bmControls", 1, DescType::Bitmap).array_bits("bNrInPins", "bNrChannels"),
    Desc::new("iMixer", 1, DescType::StrDescIndex),
];

/// UAC2: 4.7.2.6 Mixer Unit Descriptor; Table 4-11.
static DESC_AUDIO_2_AC_MIXER_UNIT: &[Desc] = &[
    Desc::new("bUnitID", 1, DescType::Number),
    Desc::new("bNrInPins", 1, DescType::Number),
    Desc::new("baSourceID", 1, DescType::Number).array_len("bNrInPins"),
    Desc::new("bNrChannels", 1, DescType::Number),
    Desc::new(
        "bmChannelConfig",
        4,
        DescType::BitmapStrings(UAC2_CHANNEL_NAMES),
    ),
    Desc::new("iChannelNames", 1, DescType::StrDescIndex),
    Desc::new("bmMixerControls", 1, DescType::Bitmap).array_bits("bNrInPins", "bNrChannels"),
    Desc::new(
        "bmControls",
        1,
        DescType::BmControl2(UAC2_MIXER_UNIT_BMCONTROLS),
    ),
    Desc::new("iMixer", 1, DescType::StrDescIndex),
];

/// UAC3: 4.5.2.5 Mixer Unit Descriptor; Table 4-29.
static DESC_AUDIO_3_AC_MIXER_UNIT: &[Desc] = &[
    Desc::new("bUnitID", 1, DescType::Number),
    Desc::new("bNrInPins", 1, DescType::Number),
    Desc::new("baSourceID", 1, DescType::Number).array_len("bNrInPins"),
    Desc::new("wClusterDescrID", 2, DescType::Number),
    Desc::new("bmMixerControls", 1, DescType::Bitmap).array(),
    Desc::new(
        "bmControls",
        4,
        DescType::BmControl2(UAC3_MIXER_UNIT_BMCONTROLS),
    ),
    Desc::new("wMixerDescrStr", 2, DescType::CsStrDescId),
];

/// Mixer Unit descriptor definitions for the three Audio Device Class protocols
pub static DESC_AUDIO_AC_MIXER_UNIT: AudioDescs = [
    Some(DESC_AUDIO_1_AC_MIXER_UNIT),
    Some(DESC_AUDIO_2_AC_MIXER_UNIT),
    Some(DESC_AUDIO_3_AC_MIXER_UNIT),
];

/// Selector Unit bmControls; Human readable bit meanings.
static UAC2_SELECTOR_UNIT_BMCONTROLS: &[&str] = &["Selector"];

/// UAC1: 4.3.2.4 Selector Unit Descriptor; Table 4-6.
static DESC_AUDIO_1_AC_SELECTOR_UNIT: &[Desc] = &[
    Desc::new("bUnitID", 1, DescType::Number),
    Desc::new("bNrInPins", 1, DescType::Number),
    Desc::new("baSourceID", 1, DescType::Number).array_len("bNrInPins"),
    Desc::new("iSelector", 1, DescType::StrDescIndex),
];

/// UAC2: 4.7.2.7 Selector Unit Descriptor; Table 4-12.
static DESC_AUDIO_2_AC_SELECTOR_UNIT: &[Desc] = &[
    Desc::new("bUnitID", 1, DescType::Number),
    Desc::new("bNrInPins", 1, DescType::Number),
    Desc::new("baSourceID", 1, DescType::Number).array_len("bNrInPins"),
    Desc::new(
        "bmControls",
        1,
        DescType::BmControl2(UAC2_SELECTOR_UNIT_BMCONTROLS),
    ),
    Desc::new("iSelector", 1, DescType::StrDescIndex),
];

/// UAC3: 4.5.2.6 Selector Unit Descriptor; Table 4-30.
static DESC_AUDIO_3_AC_SELECTOR_UNIT: &[Desc] = &[
    Desc::new("bUnitID", 1, DescType::Number),
    Desc::new("bNrInPins", 1, DescType::Number),
    Desc::new("baSourceID", 1, DescType::Number).array_len("bNrInPins"),
    Desc::new(
        "bmControls",
        4,
        DescType::BmControl2(UAC2_SELECTOR_UNIT_BMCONTROLS),
    ),
    Desc::new("wSelectorDescrStr", 2, DescType::CsStrDescId),
];

/// Selector Unit descriptor definitions for the three Audio Device Class protocols
pub static DESC_AUDIO_AC_SELECTOR_UNIT: AudioDescs = [
    Some(DESC_AUDIO_1_AC_SELECTOR_UNIT),
    Some(DESC_AUDIO_2_AC_SELECTOR_UNIT),
    Some(DESC_AUDIO_3_AC_SELECTOR_UNIT),
];

/// UAC1: Table A-7: Processing Unit Process Types
static UAC1_PROC_UNIT_TYPES: &[&str] = &[
    "Undefined",
    "Up/Down-mix",
    "Dolby Prologic",
    "3D Stereo Extender",
    "Reverberation",
    "Chorus",
    "Dyn Range Comp",
];

/// UAC1: Up/Down-mix and Dolby Prologic proc unit descriptor extensions Table 4-9, Table 4-10.
static DESC_AUDIO_1_AC_PROC_UNIT_EXTENDED: &[Desc] = &[
    Desc::new("bNrModes", 1, DescType::Number),
    Desc::new("waModes", 2, DescType::Bitmap).array_len("bNrModes"),
];

/// UAC1: Table A-7: Processing Unit Process Types
static DESC_AUDIO_1_AC_PROC_UNIT_SPECIFIC: &[DescExt] = &[
    DescExt {
        ty: 1,
        desc: DESC_AUDIO_1_AC_PROC_UNIT_EXTENDED,
    },
    DescExt {
        ty: 2,
        desc: DESC_AUDIO_1_AC_PROC_UNIT_EXTENDED,
    },
];

/// UAC1: 4.3.2.6 Processing Unit Descriptor; Table 4-8.
static DESC_AUDIO_1_AC_PROCESSING_UNIT: &[Desc] = &[
    Desc::new("bUnitID", 1, DescType::Number),
    Desc::new(
        "wProcessType",
        2,
        DescType::NumberStrings(UAC1_PROC_UNIT_TYPES),
    ),
    Desc::new("bNrInPins", 1, DescType::Number),
    Desc::new("baSourceID", 1, DescType::Number).array_len("bNrInPins"),
    Desc::new("bNrChannels", 1, DescType::Number),
    Desc::new(
        "wChannelConfig",
        2,
        DescType::BitmapStrings(UAC1_CHANNEL_NAMES),
    ),
    Desc::new("iChannelNames", 1, DescType::StrDescIndex),
    Desc::new("bControlSize", 1, DescType::Number),
    Desc::new("bmControls", 1, DescType::Bitmap).array_len("bControlSize"),
    Desc::new("iProcessing", 1, DescType::StrDescIndex),
    Desc::new(
        "Process-specific",
        1,
        DescType::Extension {
            type_field: "wProcessType",
            d: DESC_AUDIO_1_AC_PROC_UNIT_SPECIFIC,
        },
    ),
];

/// UAC2: 4.7.2.11.1 Up/Down-mix Processing Unit Descriptor; Table 4-21.
static DESC_AUDIO_2_AC_PROC_UNIT_UP_DOWN_MIX: &[Desc] = &[
    Desc::new("bNrModes", 1, DescType::Number),
    Desc::new("daModes", 4, DescType::Bitmap).array_len("bNrModes"),
];

/// UAC2: 4.7.2.11.2 Dolby prologic Processing Unit Descriptor; Table 4-22.
static DESC_AUDIO_2_AC_PROC_UNIT_DOLBY_PROLOGIC: &[Desc] = &[
    Desc::new("bNrModes", 1, DescType::Number),
    Desc::new("daModes", 4, DescType::Bitmap).array_len("bNrModes"),
];

/// UAC2: Table A-12: Processing Unit Process Types
static DESC_AUDIO_2_AC_PROC_UNIT_SPECIFIC: &[DescExt] = &[
    DescExt {
        ty: 1,
        desc: DESC_AUDIO_2_AC_PROC_UNIT_UP_DOWN_MIX,
    },
    DescExt {
        ty: 2,
        desc: DESC_AUDIO_2_AC_PROC_UNIT_DOLBY_PROLOGIC,
    },
];

/// UAC2: Table A-12: Processing Unit Process Types
static UAC2_PROC_UNIT_TYPES: &[&str] = &[
    "Undefined",
    "Up/Down-mix",
    "Dolby Prologic",
    "Stereo Extender",
];

/// UAC2: 4.7.2.11 Processing Unit Descriptor; Table 4-20.
static DESC_AUDIO_2_AC_PROCESSING_UNIT: &[Desc] = &[
    Desc::new("bUnitID", 1, DescType::Number),
    Desc::new(
        "wProcessType",
        2,
        DescType::NumberStrings(UAC2_PROC_UNIT_TYPES),
    ),
    Desc::new("bNrInPins", 1, DescType::Number),
    Desc::new("baSourceID", 1, DescType::Number).array_len("bNrInPins"),
    Desc::new("bNrChannels", 1, DescType::Number),
    Desc::new(
        "bmChannelConfig",
        4,
        DescType::BitmapStrings(UAC2_CHANNEL_NAMES),
    ),
    Desc::new("iChannelNames", 1, DescType::StrDescIndex),
    Desc::new("bmControls", 2, DescType::Bitmap),
    Desc::new("iProcessing", 1, DescType::StrDescIndex),
    Desc::new(
        "Process-specific",
        1,
        DescType::Extension {
            type_field: "wProcessType",
            d: DESC_AUDIO_2_AC_PROC_UNIT_SPECIFIC,
        },
    ),
];

/// Processor unit Up/Down-mix bmControls; Human readable bit meanings.
static UAC3_PROC_UNIT_UP_DOWN_MIX_BMCONTROLS: &[&str] = &["Mode Select", "Underflow", "Overflow"];

/// UAC3: 4.5.2.10.1 Up/Down-mix Processing Unit Descriptor; Table 4-39.
static DESC_AUDIO_3_AC_PROC_UNIT_UP_DOWN_MIX: &[Desc] = &[
    Desc::new(
        "bmControls",
        4,
        DescType::BmControl2(UAC3_PROC_UNIT_UP_DOWN_MIX_BMCONTROLS),
    ),
    Desc::new("bNrModes", 1, DescType::Number),
    Desc::new("waClusterDescrID", 2, DescType::Number).array_len("bNrModes"),
];

/// Processor unit stereo extender bmControls; Human readable bit meanings.
static UAC3_PROC_UNIT_STEREO_EXTENDER_BMCONTROLS: &[&str] = &["Width", "Underflow", "Overflow"];

/// UAC3: 4.5.2.10.2 Stereo Extender Processing Unit Descriptor; Table 4-40.
static DESC_AUDIO_3_AC_PROC_UNIT_STEREO_EXTENDER: &[Desc] = &[Desc::new(
    "bmControls",
    4,
    DescType::BmControl2(UAC3_PROC_UNIT_STEREO_EXTENDER_BMCONTROLS),
)];

/// UAC3: 4.5.2.10.3 Multi Func Proc Unit Descriptor; Algorithms; Table 4-41.
///
/// (`.bitmap_strings` form with `.count = 6`; the C array's trailing NULL at
/// index 6 is beyond the count and therefore omitted.)
static UAC3_PROC_UNIT_MULTI_FUNC_ALGORITHMS: &[Option<&str>] = &[
    Some("Algorithm Undefined."),
    Some("Beam Forming."),
    Some("Acoustic Echo Cancellation."),
    Some("Active Noise Cancellation."),
    Some("Blind Source Separation."),
    Some("Noise Suppression/Reduction."),
];

/// Processor unit Multi Func bmControls; Human readable bit meanings.
static UAC3_PROC_UNIT_MULTI_FUNC_BMCONTROLS: &[&str] = &["Underflow", "Overflow"];

/// UAC3: 4.5.2.10.3 Multi Function Processing Unit Descriptor; Table 4-41.
static DESC_AUDIO_3_AC_PROC_UNIT_MULTI_FUNCTION: &[Desc] = &[
    Desc::new(
        "bmControls",
        4,
        DescType::BmControl2(UAC3_PROC_UNIT_MULTI_FUNC_BMCONTROLS),
    ),
    Desc::new("wClusterDescrID", 2, DescType::Number),
    Desc::new(
        "bmAlgorithms",
        4,
        DescType::BitmapStrings(UAC3_PROC_UNIT_MULTI_FUNC_ALGORITHMS),
    ),
];

/// UAC3: Table A-20: Processing Unit Process Types
static UAC3_PROC_UNIT_TYPES: &[&str] = &[
    "Undefined",
    "Up/Down-mix",
    "Stereo Extender",
    "Multi-Function",
];

/// UAC3: Table A-20: Processing Unit Process Types
static DESC_AUDIO_3_AC_PROC_UNIT_SPECIFIC: &[DescExt] = &[
    DescExt {
        ty: 1,
        desc: DESC_AUDIO_3_AC_PROC_UNIT_UP_DOWN_MIX,
    },
    DescExt {
        ty: 2,
        desc: DESC_AUDIO_3_AC_PROC_UNIT_STEREO_EXTENDER,
    },
    DescExt {
        ty: 3,
        desc: DESC_AUDIO_3_AC_PROC_UNIT_MULTI_FUNCTION,
    },
];

/// UAC3: 4.5.2.10 Processing Unit Descriptor; Table 4-38.
static DESC_AUDIO_3_AC_PROCESSING_UNIT: &[Desc] = &[
    Desc::new("bUnitID", 1, DescType::Number),
    Desc::new(
        "wProcessType",
        2,
        DescType::NumberStrings(UAC3_PROC_UNIT_TYPES),
    ),
    Desc::new("bNrInPins", 1, DescType::Number),
    Desc::new("baSourceID", 1, DescType::Number).array_len("bNrInPins"),
    Desc::new("wProcessingDescrStr", 2, DescType::CsStrDescId),
    Desc::new(
        "Process-specific",
        1,
        DescType::Extension {
            type_field: "wProcessType",
            d: DESC_AUDIO_3_AC_PROC_UNIT_SPECIFIC,
        },
    ),
];

/// Processing Unit descriptor definitions for the three Audio Device Class protocols
pub static DESC_AUDIO_AC_PROCESSING_UNIT: AudioDescs = [
    Some(DESC_AUDIO_1_AC_PROCESSING_UNIT),
    Some(DESC_AUDIO_2_AC_PROCESSING_UNIT),
    Some(DESC_AUDIO_3_AC_PROCESSING_UNIT),
];

/// Audio Control Feature Unit bmControls; Human readable bit meanings.
static UAC_FEATURE_UNIT_BMCONTROLS: &[&str] = &[
    "Mute",
    "Volume",
    "Bass",
    "Mid",
    "Treble",
    "Graphic Equalizer",
    "Automatic Gain",
    "Delay",
    "Bass Boost",
    "Loudness",
    "Input gain",
    "Input gain pad",
    "Phase inverter",
];

/// UAC1: 4.3.2.5 Feature Unit Descriptor; Table 4-7.
static DESC_AUDIO_1_AC_FEATURE_UNIT: &[Desc] = &[
    Desc::new("bUnitID", 1, DescType::Number),
    Desc::new("bSourceID", 1, DescType::Constant),
    Desc::new("bControlSize", 1, DescType::Number),
    Desc::sized_by(
        "bmaControls",
        "bControlSize",
        DescType::BmControl1(UAC_FEATURE_UNIT_BMCONTROLS),
    )
    .array(),
    Desc::new("iFeature", 1, DescType::StrDescIndex),
];

/// UAC2: 4.7.2.8 Feature Unit Descriptor; Table 4-13.
static DESC_AUDIO_2_AC_FEATURE_UNIT: &[Desc] = &[
    Desc::new("bUnitID", 1, DescType::Number),
    Desc::new("bSourceID", 1, DescType::Constant),
    Desc::new(
        "bmaControls",
        4,
        DescType::BmControl2(UAC_FEATURE_UNIT_BMCONTROLS),
    )
    .array(),
    Desc::new("iFeature", 1, DescType::StrDescIndex),
];

/// UAC3: 4.5.2.7 Feature Unit Descriptor; Table 4-31.
static DESC_AUDIO_3_AC_FEATURE_UNIT: &[Desc] = &[
    Desc::new("bUnitID", 1, DescType::Number),
    Desc::new("bSourceID", 1, DescType::Number),
    Desc::new(
        "bmaControls",
        4,
        DescType::BmControl2(UAC_FEATURE_UNIT_BMCONTROLS),
    )
    .array(),
    Desc::new("wFeatureDescrStr", 2, DescType::CsStrDescId),
];

/// Feature Unit descriptor definitions for the three Audio Device Class protocols
pub static DESC_AUDIO_AC_FEATURE_UNIT: AudioDescs = [
    Some(DESC_AUDIO_1_AC_FEATURE_UNIT),
    Some(DESC_AUDIO_2_AC_FEATURE_UNIT),
    Some(DESC_AUDIO_3_AC_FEATURE_UNIT),
];

/// UAC2 Extension Unit bmControls; Human readable bit meanings.
static UAC2_EXTENSION_UNIT_BMCONTROLS: &[&str] = &["Enable", "Cluster", "Underflow", "Overflow"];

/// UAC3 Extension Unit bmControls; Human readable bit meanings.
static UAC3_EXTENSION_UNIT_BMCONTROLS: &[&str] = &["Underflow", "Overflow"];

/// UAC1: 4.3.2.7 Extension Unit Descriptor; Table 4-15.
static DESC_AUDIO_1_AC_EXTENSION_UNIT: &[Desc] = &[
    Desc::new("bUnitID", 1, DescType::Number),
    Desc::new("wExtensionCode", 2, DescType::Constant),
    Desc::new("bNrInPins", 1, DescType::Number),
    Desc::new("baSourceID", 1, DescType::Number).array_len("bNrInPins"),
    Desc::new("bNrChannels", 1, DescType::Number),
    Desc::new(
        "wChannelConfig",
        2,
        DescType::BitmapStrings(UAC1_CHANNEL_NAMES),
    ),
    Desc::new("iChannelNames", 1, DescType::StrDescIndex),
    Desc::new("bControlSize", 1, DescType::Number),
    Desc::new("bmControls", 1, DescType::Bitmap).array_len("bControlSize"),
    Desc::new("iExtension", 1, DescType::StrDescIndex),
];

/// UAC2: 4.7.2.12 Extension Unit Descriptor; Table 4-24.
static DESC_AUDIO_2_AC_EXTENSION_UNIT: &[Desc] = &[
    Desc::new("bUnitID", 1, DescType::Number),
    Desc::new("wExtensionCode", 2, DescType::Constant),
    Desc::new("bNrInPins", 1, DescType::Number),
    Desc::new("baSourceID", 1, DescType::Number).array_len("bNrInPins"),
    Desc::new("bNrChannels", 1, DescType::Number),
    Desc::new(
        "bmChannelConfig",
        4,
        DescType::BitmapStrings(UAC2_CHANNEL_NAMES),
    ),
    Desc::new("iChannelNames", 1, DescType::StrDescIndex),
    Desc::new(
        "bmControls",
        1,
        DescType::BmControl2(UAC2_EXTENSION_UNIT_BMCONTROLS),
    ),
    Desc::new("iExtension", 1, DescType::StrDescIndex),
];

/// UAC3: 4.5.2.11 Extension Unit Descriptor; Table 4-42.
static DESC_AUDIO_3_AC_EXTENSION_UNIT: &[Desc] = &[
    Desc::new("bUnitID", 1, DescType::Number),
    Desc::new("wExtensionCode", 2, DescType::Constant),
    Desc::new("bNrInPins", 1, DescType::Number),
    Desc::new("baSourceID", 1, DescType::Number).array_len("bNrInPins"),
    Desc::new("wExtensionDescrStr", 2, DescType::CsStrDescId),
    Desc::new(
        "bmControls",
        4,
        DescType::BmControl2(UAC3_EXTENSION_UNIT_BMCONTROLS),
    ),
    Desc::new("wClusterDescrID", 2, DescType::Number),
];

/// Extension Unit descriptor definitions for the three Audio Device Class protocols
pub static DESC_AUDIO_AC_EXTENSION_UNIT: AudioDescs = [
    Some(DESC_AUDIO_1_AC_EXTENSION_UNIT),
    Some(DESC_AUDIO_2_AC_EXTENSION_UNIT),
    Some(DESC_AUDIO_3_AC_EXTENSION_UNIT),
];

/// UAC2 Clock Source bmControls; Human readable bit meanings.
static UAC2_CLOCK_SOURCE_BMCONTROLS: &[&str] = &["Clock Frequency", "Clock Validity"];

static UAC2_CLK_SRC_BMATTR: [&str; 4] = [
    "External",
    "Internal fixed",
    "Internal variable",
    "Internal programmable",
];

static UAC3_CLK_SRC_BMATTR: [&str; 4] = [
    "External",
    "Internal",
    "(asynchronous)",
    "(synchronized to SOF)",
];

/// Special rendering function for UAC2 clock source bmAttributes
fn desc_snowflake_dump_uac2_clk_src_bmattr(value: u64, field: &mut Node) {
    let sync = if (value & 0x4) != 0 {
        UAC3_CLK_SRC_BMATTR[3]
    } else {
        ""
    };
    field.note(
        format!(
            "{} clock {sync}",
            UAC2_CLK_SRC_BMATTR[(value & 0x3) as usize]
        )
        .trim_end(),
    );
}

/// Special rendering function for UAC3 clock source bmAttributes
fn desc_snowflake_dump_uac3_clk_src_bmattr(value: u64, field: &mut Node) {
    field.note(format!(
        "{} clock {}",
        UAC3_CLK_SRC_BMATTR[(value & 0x1) as usize],
        UAC3_CLK_SRC_BMATTR[(0x2 | ((value & 0x2) >> 1)) as usize]
    ));
}

/// UAC2: 4.7.2.1 Clock Source Descriptor; Table 4-6.
static DESC_AUDIO_2_AC_CLOCK_SOURCE: &[Desc] = &[
    Desc::new("bClockID", 1, DescType::Constant),
    Desc::new(
        "bmAttributes",
        1,
        DescType::Snowflake(desc_snowflake_dump_uac2_clk_src_bmattr),
    ),
    Desc::new(
        "bmControls",
        1,
        DescType::BmControl2(UAC2_CLOCK_SOURCE_BMCONTROLS),
    ),
    Desc::new("bAssocTerminal", 1, DescType::Constant),
    Desc::new("iClockSource", 1, DescType::StrDescIndex),
];

/// UAC3: 4.5.2.12 Clock Source Descriptor; Table 4-43.
static DESC_AUDIO_3_AC_CLOCK_SOURCE: &[Desc] = &[
    Desc::new("bClockID", 1, DescType::Number),
    Desc::new(
        "bmAttributes",
        1,
        DescType::Snowflake(desc_snowflake_dump_uac3_clk_src_bmattr),
    ),
    Desc::new(
        "bmControls",
        4,
        DescType::BmControl2(UAC2_CLOCK_SOURCE_BMCONTROLS),
    ),
    Desc::new("bReferenceTerminal", 1, DescType::Number),
    Desc::new("wClockSourceStr", 2, DescType::CsStrDescId),
];

/// Clock Source descriptor definitions for the three Audio Device Class protocols
pub static DESC_AUDIO_AC_CLOCK_SOURCE: AudioDescs = [
    None, // UAC1 not supported
    Some(DESC_AUDIO_2_AC_CLOCK_SOURCE),
    Some(DESC_AUDIO_3_AC_CLOCK_SOURCE),
];

/// UAC2 Clock Selector bmControls; Human readable bit meanings.
static UAC2_CLOCK_SELECTOR_BMCONTROLS: &[&str] = &["Clock Selector"];

/// UAC2: 4.7.2.2 Clock Selector Descriptor; Table 4-7.
static DESC_AUDIO_2_AC_CLOCK_SELECTOR: &[Desc] = &[
    Desc::new("bClockID", 1, DescType::Number),
    Desc::new("bNrInPins", 1, DescType::Number),
    Desc::new("baCSourceID", 1, DescType::Number).array_len("bNrInPins"),
    Desc::new(
        "bmControls",
        1,
        DescType::BmControl2(UAC2_CLOCK_SELECTOR_BMCONTROLS),
    ),
    Desc::new("iClockSelector", 1, DescType::StrDescIndex),
];

/// UAC3: 4.5.2.13 Clock Selector Descriptor; Table 4-44.
static DESC_AUDIO_3_AC_CLOCK_SELECTOR: &[Desc] = &[
    Desc::new("bClockID", 1, DescType::Number),
    Desc::new("bNrInPins", 1, DescType::Number),
    Desc::new("baCSourceID", 1, DescType::Number).array_len("bNrInPins"),
    Desc::new(
        "bmControls",
        4,
        DescType::BmControl2(UAC2_CLOCK_SELECTOR_BMCONTROLS),
    ),
    Desc::new("wCSelectorDescrStr", 2, DescType::CsStrDescId),
];

/// Clock Selector descriptor definitions for the three Audio Device Class protocols
pub static DESC_AUDIO_AC_CLOCK_SELECTOR: AudioDescs = [
    None, // UAC1 not supported
    Some(DESC_AUDIO_2_AC_CLOCK_SELECTOR),
    Some(DESC_AUDIO_3_AC_CLOCK_SELECTOR),
];

/// UAC2 Clock Multiplier bmControls; Human readable bit meanings.
static UAC2_CLOCK_MULTIPLIER_BMCONTROLS: &[&str] = &["Clock Numerator", "Clock Denominator"];

/// UAC2: 4.7.2.3 Clock Multiplier Descriptor; Table 4-8.
static DESC_AUDIO_2_AC_CLOCK_MULTIPLIER: &[Desc] = &[
    Desc::new("bClockID", 1, DescType::Constant),
    Desc::new("bCSourceID", 1, DescType::Number),
    Desc::new(
        "bmControls",
        1,
        DescType::BmControl2(UAC2_CLOCK_MULTIPLIER_BMCONTROLS),
    ),
    Desc::new("iClockMultiplier", 1, DescType::StrDescIndex),
];

/// UAC3: 4.5.2.14 Clock Multiplier Descriptor; Table 4-45.
static DESC_AUDIO_3_AC_CLOCK_MULTIPLIER: &[Desc] = &[
    Desc::new("bClockID", 1, DescType::Number),
    Desc::new("bCSourceID", 1, DescType::Number),
    Desc::new(
        "bmControls",
        4,
        DescType::BmControl2(UAC2_CLOCK_MULTIPLIER_BMCONTROLS),
    ),
    Desc::new("wCMultiplierDescrStr", 2, DescType::CsStrDescId),
];

/// Clock Multiplier descriptor definitions for the three Audio Device Class protocols
pub static DESC_AUDIO_AC_CLOCK_MULTIPLIER: AudioDescs = [
    None, // UAC1 not supported
    Some(DESC_AUDIO_2_AC_CLOCK_MULTIPLIER),
    Some(DESC_AUDIO_3_AC_CLOCK_MULTIPLIER),
];

/// UAC2: 4.7.2.9 Sampling Rate Converter Descriptor; Table 4-14.
#[allow(dead_code)]
static DESC_AUDIO_2_AC_SAMPLE_RATE_CONVERTER: &[Desc] = &[
    Desc::new("bUnitID", 1, DescType::Constant),
    Desc::new("bSourceID", 1, DescType::Constant),
    Desc::new("bCSourceInID", 1, DescType::Constant),
    Desc::new("bCSourceOutID", 1, DescType::Constant),
    Desc::new("iSRC", 1, DescType::StrDescIndex),
];

/// UAC3: 4.5.2.8 Sampling Rate Converter Descriptor; Table 4-32.
#[allow(dead_code)]
static DESC_AUDIO_3_AC_SAMPLE_RATE_CONVERTER: &[Desc] = &[
    Desc::new("bUnitID", 1, DescType::Number),
    Desc::new("bSourceID", 1, DescType::Number),
    Desc::new("bCSourceInID", 1, DescType::Number),
    Desc::new("bCSourceOutID", 1, DescType::Number),
    Desc::new("wSRCDescrStr", 2, DescType::CsStrDescId),
];

/// Sample Rate Converter descriptor definitions for the three Audio Device Class protocols
// Unused: upstream never dumps this descriptor type either.
#[allow(dead_code)]
pub static DESC_AUDIO_AC_SAMPLE_RATE_CONVERTER: AudioDescs = [
    None, // UAC1 not supported
    Some(DESC_AUDIO_2_AC_SAMPLE_RATE_CONVERTER),
    Some(DESC_AUDIO_3_AC_SAMPLE_RATE_CONVERTER),
];

/// UAC2 AudioStreaming Interface bmControls; Human readable bit meanings.
static UAC2_AS_INTERFACE_BMCONTROLS: &[&str] =
    &["Active Alternate Setting", "Valid Alternate Setting"];

/// UAC3 AudioStreaming Interface bmControls; Human readable bit meanings.
static UAC3_AS_INTERFACE_BMCONTROLS: &[&str] = &[
    "Active Alternate Setting",
    "Valid Alternate Setting",
    "Audio Data Format Control",
];

// wFormatTag hex prefix for format type
const UAC_FORMAT_TYPE_I: u64 = 0x0;
const UAC_FORMAT_TYPE_II: u64 = 0x1;
const UAC_FORMAT_TYPE_III: u64 = 0x2;

/// Format type I codes; Human-readable values.
static AUDIO_DATA_FORMAT_TYPE_I: &[&str] = &[
    "TYPE_I_UNDEFINED",
    "PCM",
    "PCM8",
    "IEEE_FLOAT",
    "ALAW",
    "MULAW",
];

/// Format type II codes; Human-readable values.
static AUDIO_DATA_FORMAT_TYPE_II: &[&str] = &["TYPE_II_UNDEFINED", "MPEG", "AC-3"];

/// Format type III codes; Human-readable values.
static AUDIO_DATA_FORMAT_TYPE_III: &[&str] = &[
    "TYPE_III_UNDEFINED",
    "IEC1937_AC-3",
    "IEC1937_MPEG-1_Layer1",
    "IEC1937_MPEG-Layer2/3/NOEXT",
    "IEC1937_MPEG-2_EXT",
    "IEC1937_MPEG-2_Layer1_LS",
    "IEC1937_MPEG-2_Layer2/3_LS",
];

/// Special rendering function for UAC1 AS interface wFormatTag
fn desc_snowflake_dump_uac1_as_interface_wformattag(value: u64, field: &mut Node) {
    let mut format_string = "undefined";

    // Format codes are 0xTNNN, where T=Type prefix, NNN = format code.

    if value < ((UAC_FORMAT_TYPE_I << 12) + AUDIO_DATA_FORMAT_TYPE_I.len() as u64) {
        format_string = AUDIO_DATA_FORMAT_TYPE_I[value as usize];
    } else if (value >= (UAC_FORMAT_TYPE_II << 12))
        && (value < ((UAC_FORMAT_TYPE_II << 12) + AUDIO_DATA_FORMAT_TYPE_II.len() as u64))
    {
        format_string = AUDIO_DATA_FORMAT_TYPE_II[(value & 0xfff) as usize];
    } else if (value >= (UAC_FORMAT_TYPE_III << 12))
        && (value < ((UAC_FORMAT_TYPE_III << 12) + AUDIO_DATA_FORMAT_TYPE_III.len() as u64))
    {
        format_string = AUDIO_DATA_FORMAT_TYPE_III[(value & 0xfff) as usize];
    }

    field.note(format_string);
}

/// Special rendering function for UAC2 AS interface bmFormats
fn desc_snowflake_dump_uac2_as_interface_bmformats(value: u64, field: &mut Node) {
    for i in 0..5u32 {
        if (value >> i) & 0x1 != 0 {
            field.text(AUDIO_DATA_FORMAT_TYPE_I[(i + 1) as usize]);
        }
    }
}

/// UAC1: 4.5.2 Class-Specific AS Interface Descriptor; Table 4-19.
static DESC_AUDIO_1_AS_INTERFACE: &[Desc] = &[
    Desc::new("bTerminalLink", 1, DescType::Constant),
    Desc::new("bDelay", 1, DescType::NumberPostfix(" frames")),
    Desc::new(
        "wFormatTag",
        2,
        DescType::Snowflake(desc_snowflake_dump_uac1_as_interface_wformattag),
    ),
];

/// UAC2: 4.9.2 Class-Specific AS Interface Descriptor; Table 4-27.
static DESC_AUDIO_2_AS_INTERFACE: &[Desc] = &[
    Desc::new("bTerminalLink", 1, DescType::Number),
    Desc::new(
        "bmControls",
        1,
        DescType::BmControl2(UAC2_AS_INTERFACE_BMCONTROLS),
    ),
    Desc::new("bFormatType", 1, DescType::Constant),
    Desc::new(
        "bmFormats",
        4,
        DescType::Snowflake(desc_snowflake_dump_uac2_as_interface_bmformats),
    ),
    Desc::new("bNrChannels", 1, DescType::Number),
    Desc::new(
        "bmChannelConfig",
        4,
        DescType::BitmapStrings(UAC2_CHANNEL_NAMES),
    ),
    Desc::new("iChannelNames", 1, DescType::StrDescIndex),
];

/// UAC3: 4.7.2 Class-Specific AS Interface Descriptor; Table 4-49.
static DESC_AUDIO_3_AS_INTERFACE: &[Desc] = &[
    Desc::new("bTerminalLink", 1, DescType::Number),
    Desc::new(
        "bmControls",
        4,
        DescType::BmControl2(UAC3_AS_INTERFACE_BMCONTROLS),
    ),
    Desc::new("wClusterDescrID", 2, DescType::Number),
    Desc::new("bmFormats", 8, DescType::Bitmap),
    Desc::new("bSubslotSize", 1, DescType::Number),
    Desc::new("bBitResolution", 1, DescType::Number),
    Desc::new("bmAuxProtocols", 2, DescType::Bitmap),
    Desc::new("bControlSize", 1, DescType::Number),
];

/// AudioStreaming Interface descriptor definitions for the three Audio Device Class protocols
pub static DESC_AUDIO_AS_INTERFACE: AudioDescs = [
    Some(DESC_AUDIO_1_AS_INTERFACE),
    Some(DESC_AUDIO_2_AS_INTERFACE),
    Some(DESC_AUDIO_3_AS_INTERFACE),
];

/// UAC1: Data Endpoint bmAttributes; Human readable bit meanings.
///
/// C: `[0]`, `[1]`, `[2]`, `[7]` set; `[3]`..`[6]` implicitly NULL.
static UAC1_AS_ENDPOINT_BMATTRIBUTES: &[Option<&str>] = &[
    Some("Sampling Frequency"),
    Some("Pitch"),
    Some("Audio Data Format Control"),
    None,
    None,
    None,
    None,
    Some("MaxPacketsOnly"),
];

/// UAC2: Data Endpoint bmAttributes; Human readable bit meanings.
///
/// C: only `[7]` set; `[0]`..`[6]` implicitly NULL.
static UAC2_AS_ENDPOINT_BMATTRIBUTES: &[Option<&str>] = &[
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    Some("MaxPacketsOnly"),
];

/// UAC2 AudioStreaming Interface bmControls; Human readable bit meanings.
static UAC2_AS_ISOCHRONOUS_AUDIO_DATA_ENDPOINT_BMCONTROLS: &[&str] =
    &["Pitch", "Data Overrun", "Data Underrun"];

/// Audio Data Endpoint bLockDelayUnits; Human readable value meanings.
static UAC_AS_ISOCHRONOUS_AUDIO_DATA_ENDPOINT_BLOCKDELAYUNITS: &[&str] =
    &["Undefined", "Milliseconds", "Decoded PCM samples"];

/// UAC1: 4.6.1.2 Class-Specific AS Isochronous Audio Data Endpoint Descriptor; Table 4-21.
static DESC_AUDIO_1_AS_ISOCHRONOUS_AUDIO_DATA_ENDPOINT: &[Desc] = &[
    Desc::new(
        "bmAttributes",
        1,
        DescType::BitmapStrings(UAC1_AS_ENDPOINT_BMATTRIBUTES),
    ),
    Desc::new(
        "bLockDelayUnits",
        1,
        DescType::NumberStrings(UAC_AS_ISOCHRONOUS_AUDIO_DATA_ENDPOINT_BLOCKDELAYUNITS),
    ),
    Desc::new("wLockDelay", 2, DescType::Number),
];

/// UAC2: 4.10.1.2 Class-Specific AS Isochronous Audio Data Endpoint Descriptor; Table 4-34.
static DESC_AUDIO_2_AS_ISOCHRONOUS_AUDIO_DATA_ENDPOINT: &[Desc] = &[
    Desc::new(
        "bmAttributes",
        1,
        DescType::BitmapStrings(UAC2_AS_ENDPOINT_BMATTRIBUTES),
    ),
    Desc::new(
        "bmControls",
        1,
        DescType::BmControl2(UAC2_AS_ISOCHRONOUS_AUDIO_DATA_ENDPOINT_BMCONTROLS),
    ),
    Desc::new(
        "bLockDelayUnits",
        1,
        DescType::NumberStrings(UAC_AS_ISOCHRONOUS_AUDIO_DATA_ENDPOINT_BLOCKDELAYUNITS),
    ),
    Desc::new("wLockDelay", 2, DescType::Number),
];

/// UAC3: 4.8.1.2 Class-Specific AS Isochronous Audio Data Endpoint Descriptor; Table 4-52.
static DESC_AUDIO_3_AS_ISOCHRONOUS_AUDIO_DATA_ENDPOINT: &[Desc] = &[
    Desc::new(
        "bmControls",
        4,
        DescType::BmControl2(UAC2_AS_ISOCHRONOUS_AUDIO_DATA_ENDPOINT_BMCONTROLS),
    ),
    Desc::new(
        "bLockDelayUnits",
        1,
        DescType::NumberStrings(UAC_AS_ISOCHRONOUS_AUDIO_DATA_ENDPOINT_BLOCKDELAYUNITS),
    ),
    Desc::new("wLockDelay", 2, DescType::Number),
];

/// Isochronous Audio Data Endpoint descriptor definitions for the three Audio Device Class protocols
pub static DESC_AUDIO_AS_ISOCHRONOUS_AUDIO_DATA_ENDPOINT: AudioDescs = [
    Some(DESC_AUDIO_1_AS_ISOCHRONOUS_AUDIO_DATA_ENDPOINT),
    Some(DESC_AUDIO_2_AS_ISOCHRONOUS_AUDIO_DATA_ENDPOINT),
    Some(DESC_AUDIO_3_AS_ISOCHRONOUS_AUDIO_DATA_ENDPOINT),
];

// ---------------------------------------------------------------------------

// MIDI Streaming (MS) — USB Device Class Definition for MIDI Devices 1.0

static MIDI_JACK_TYPES: &[&str] = &["Undefined", "Embedded", "External"];

static MIDI_ELEMENT_CAPS: &[Option<&str>] = &[
    Some("Undefined"),
    Some("MIDI Clock"),
    Some("MTC (MIDI Time Code)"),
    Some("MMC (MIDI Machine Control)"),
    Some("GM1 (General MIDI v.1)"),
    Some("GM2 (General MIDI v.2)"),
    Some("GS MIDI Extension"),
    Some("XG MIDI Extension"),
    Some("EFX"),
    Some("MIDI Patch Bay"),
    Some("DLS1 (Downloadable Sounds Level 1)"),
    Some("DLS2 (Downloadable Sounds Level 2)"),
];

/// The MIDI OUT Jack and Element descriptors interleave baSourceID(n) /
/// baSourcePin(n) on the wire.  Represent the pair as a 2-byte array entry
/// and unpack it here so the engine's per-field bounds checking still
/// applies and we don't need a hand-rolled offset walk.
fn desc_snowflake_midi_source_pair(value: u64, field: &mut Node) {
    field.note(format!("ID {}, Pin {}", value & 0xff, value >> 8));
}

/// midi10: 6.1.2.1 Class-Specific MS Interface Header Descriptor; Table 6-2.
pub static DESC_MIDI_MS_HEADER: &[Desc] = &[
    Desc::new("bcdMSC", 2, DescType::Bcd),
    Desc::new("wTotalLength", 2, DescType::Constant),
];

/// midi10: 6.1.2.2 MIDI IN Jack Descriptor; Table 6-3.
pub static DESC_MIDI_MS_IN_JACK: &[Desc] = &[
    Desc::new("bJackType", 1, DescType::NumberStrings(MIDI_JACK_TYPES)),
    Desc::new("bJackID", 1, DescType::Number),
    Desc::new("iJack", 1, DescType::StrDescIndex),
];

/// midi10: 6.1.2.3 MIDI OUT Jack Descriptor; Table 6-4.
pub static DESC_MIDI_MS_OUT_JACK: &[Desc] = &[
    Desc::new("bJackType", 1, DescType::NumberStrings(MIDI_JACK_TYPES)),
    Desc::new("bJackID", 1, DescType::Number),
    Desc::new("bNrInputPins", 1, DescType::Number),
    Desc::new(
        "baSource",
        2,
        DescType::Snowflake(desc_snowflake_midi_source_pair),
    )
    .array_len("bNrInputPins"),
    Desc::new("iJack", 1, DescType::StrDescIndex),
];

/// midi10: 6.1.2.4 Element Descriptor; Table 6-5.
pub static DESC_MIDI_MS_ELEMENT: &[Desc] = &[
    Desc::new("bElementID", 1, DescType::Number),
    Desc::new("bNrInputPins", 1, DescType::Number),
    Desc::new(
        "baSource",
        2,
        DescType::Snowflake(desc_snowflake_midi_source_pair),
    )
    .array_len("bNrInputPins"),
    Desc::new("bNrOutputPins", 1, DescType::Number),
    Desc::new("bInTerminalLink", 1, DescType::Number),
    Desc::new("bOutTerminalLink", 1, DescType::Number),
    Desc::new("bElCapsSize", 1, DescType::Number),
    Desc::sized_by(
        "bmElementCaps",
        "bElCapsSize",
        DescType::BitmapStrings(MIDI_ELEMENT_CAPS),
    ),
    Desc::new("iElement", 1, DescType::StrDescIndex),
];

/// midi10: 6.2.2 Class-Specific MS Bulk Data Endpoint Descriptor; Table 6-7.
pub static DESC_MIDI_MS_ENDPOINT_GENERAL: &[Desc] = &[
    Desc::new("bNumEmbMIDIJack", 1, DescType::Number),
    Desc::new("baAssocJackID", 1, DescType::Number).array_len("bNumEmbMIDIJack"),
];

/// USB3: 9.6.2.7 Configuration Summary Descriptor; Table 9-21.
pub static DESC_USB3_DC_CONFIGURATION_SUMMARY: &[Desc] = &[
    Desc::new("bLength", 1, DescType::Number),
    Desc::new("bDescriptorType", 1, DescType::Constant),
    Desc::new("bDevCapabilityType", 1, DescType::Number),
    Desc::new("bcdVersion", 2, DescType::Bcd),
    Desc::new("bClass", 1, DescType::Number),
    Desc::new("bSubClass", 1, DescType::Number),
    Desc::new("bProtocol", 1, DescType::Number),
    Desc::new("bConfigurationCount", 1, DescType::Number),
    Desc::new("bConfigurationIndex", 1, DescType::Number).array_len("bConfigurationCount"),
];
