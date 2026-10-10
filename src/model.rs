// SPDX-License-Identifier: GPL-2.0-or-later
//! The output model: a tree of sections, fields and messages.
//!
//! Everything exausb shows is first built as a [`Node`] tree and then drawn
//! by [`crate::render`].  Dumpers use the small builder API:
//!
//! ```ignore
//! let ep = parent.section("Endpoint 0x81 IN").note("Bulk · 512 bytes");
//! ep.field("bLength", 7);
//! ep.field("bmAttributes", hex2(2)).note("Bulk").text("Synch Type None");
//! ep.warn("Descriptor too short");
//! ```

use std::fmt::Display;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Kind {
    /// A titled group of nodes, e.g. a descriptor.
    #[default]
    Section,
    /// `name  value  note`, e.g. `bDeviceClass  9  Hub`.
    Field,
    /// A line of text, e.g. a decoded flag under a bitmap field.
    Text,
    /// Something wrong with the device or its descriptors.
    Warning,
}

/// One node of the output tree.
///
/// For sections, `name` is the title and `value`/`note` are optional extras
/// shown after it.  For text and warnings only `name` is used.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Node {
    pub kind: Kind,
    pub name: String,
    pub value: String,
    pub note: String,
    pub children: Vec<Node>,
}

impl Node {
    /// An empty section to build into.
    pub fn new(title: impl Into<String>) -> Node {
        Node {
            name: title.into(),
            ..Default::default()
        }
    }

    fn push(&mut self, node: Node) -> &mut Node {
        self.children.push(node);
        self.children.last_mut().unwrap()
    }

    /// Add a child section and return it.
    pub fn section(&mut self, title: impl Into<String>) -> &mut Node {
        self.push(Node::new(title))
    }

    /// Add a field and return it (to add a note or children).
    pub fn field(&mut self, name: impl Into<String>, value: impl Display) -> &mut Node {
        self.push(Node {
            kind: Kind::Field,
            name: name.into(),
            value: value.to_string(),
            ..Default::default()
        })
    }

    /// Add a line of text.
    pub fn text(&mut self, text: impl Into<String>) -> &mut Node {
        self.push(Node {
            kind: Kind::Text,
            name: text.into(),
            ..Default::default()
        })
    }

    /// Add a warning.
    pub fn warn(&mut self, text: impl Into<String>) -> &mut Node {
        self.push(Node {
            kind: Kind::Warning,
            name: text.into(),
            ..Default::default()
        })
    }

    /// Set this node's note (empty notes are ignored) and return it.
    pub fn note(&mut self, note: impl Into<String>) -> &mut Node {
        let note = note.into();
        if !note.is_empty() {
            self.note = note;
        }
        self
    }

    /// Set this node's value and return it.
    pub fn value(&mut self, value: impl Display) -> &mut Node {
        self.value = value.to_string();
        self
    }

    /// The first child with this name (for tests and summaries).
    pub fn get(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.name == name)
    }

    /// Whether this node or any descendant is a warning.
    pub fn has_warnings(&self) -> bool {
        self.kind == Kind::Warning || self.children.iter().any(Node::has_warnings)
    }
}

/// `0x12`
pub fn hex2(v: impl Into<u64>) -> String {
    format!("0x{:02x}", v.into())
}

/// `0x1234`
pub fn hex4(v: impl Into<u64>) -> String {
    format!("0x{:04x}", v.into())
}

/// `0x12345678`
pub fn hex8(v: impl Into<u64>) -> String {
    format!("0x{:08x}", v.into())
}

/// A BCD version, e.g. `0x0210` → `2.10`.
pub fn bcd(v: u16) -> String {
    format!("{:x}.{:02x}", v >> 8, v & 0xff)
}

/// Bytes as `01 02 ff`.
pub fn hex_bytes(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Join the non-empty parts with ` · `.
pub fn join(parts: &[&str]) -> String {
    parts
        .iter()
        .filter(|p| !p.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder() {
        let mut root = Node::new("Device");
        let cfg = root.section("Configuration 1").note("500mA");
        cfg.field("bmAttributes", hex2(0xc0u8))
            .note("")
            .text("Self Powered");
        cfg.warn("odd");

        let cfg = root.get("Configuration 1").unwrap();
        assert_eq!(cfg.note, "500mA");
        let attr = cfg.get("bmAttributes").unwrap();
        assert_eq!((attr.kind, attr.value.as_str()), (Kind::Field, "0xc0"));
        assert_eq!(attr.children[0].name, "Self Powered");
        assert!(root.has_warnings());
    }

    #[test]
    fn formatting() {
        assert_eq!(hex4(0x5acu16), "0x05ac");
        assert_eq!(bcd(0x0210), "2.10");
        assert_eq!(bcd(0x1601), "16.01");
        assert_eq!(hex_bytes(&[1, 0xab]), "01 ab");
        assert_eq!(join(&["PTP", "", "500mA"]), "PTP · 500mA");
    }
}
