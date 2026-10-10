// SPDX-License-Identifier: GPL-2.0-or-later
//! Drawing the output model.
//!
//! Output is built as a string with ANSI styles and written through
//! `anstream`, which drops the styles when stdout isn't a terminal, when
//! `NO_COLOR` is set, or with `--color never`.

use std::io::Write;

use anstyle::{AnsiColor, Style};

use crate::model::{Kind, Node};

const HEADER: Style = Style::new().bold();
const TITLE: Style = AnsiColor::Cyan.on_default().bold();
const VALUE: Style = AnsiColor::Yellow.on_default();
const NOTE: Style = AnsiColor::Green.on_default();
const WARNING: Style = AnsiColor::Yellow.on_default().bold();
const GLYPH: Style = Style::new().dimmed();
const DIM: Style = Style::new().dimmed();

/// Longest field name / value used for column alignment.
const MAX_NAME_WIDTH: usize = 28;
const MAX_VALUE_WIDTH: usize = 12;

fn styled(style: Style, text: &str) -> String {
    if text.is_empty() {
        String::new()
    } else {
        format!("{style}{text}{style:#}")
    }
}

fn width(s: &str) -> usize {
    s.chars().count()
}

fn pad(s: &str, w: usize) -> String {
    " ".repeat(w.saturating_sub(width(s)))
}

/// A piece of text in one style.
pub type Span = (Style, String);

/// One rendered line.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Line {
    pub spans: Vec<Span>,
    /// The key of the node this line shows, if it has one.
    pub key: Option<u32>,
    /// For section lines: the section's path (child indices from the
    /// root), used to fold it in interactive views.
    pub section: Option<Vec<usize>>,
}

impl Line {
    fn push(&mut self, style: Style, text: impl Into<String>) {
        let text = text.into();
        if !text.is_empty() {
            self.spans.push((style, text));
        }
    }

    /// The line with ANSI styles.
    pub fn to_ansi(&self) -> String {
        self.spans
            .iter()
            .map(|(style, text)| styled(*style, text))
            .collect()
    }

    /// The line without styles.
    pub fn text(&self) -> String {
        self.spans.iter().map(|(_, text)| text.as_str()).collect()
    }
}

/// Which sections are folded (interactive views); `None` means "draw
/// everything, without fold markers".
pub type Folds<'a> = Option<&'a dyn Fn(&[usize]) -> bool>;

struct Renderer<'a> {
    folds: Folds<'a>,
    lines: Vec<Line>,
}

impl Renderer<'_> {
    /// Append `  value  note` (as present) to `line`.
    fn value_note(line: &mut Line, value: &str, value_w: usize, note: &str) {
        if !value.is_empty() {
            line.push(Style::new(), "  ");
            line.push(VALUE, value);
        }
        if !note.is_empty() {
            line.push(Style::new(), format!("{}  ", pad(value, value_w)));
            line.push(NOTE, note);
        }
    }

    fn children(&mut self, node: &Node, path: &mut Vec<usize>, prefix: &[Span]) {
        let children = &node.children;
        let fields = children.iter().filter(|c| c.kind == Kind::Field);
        let name_w = fields
            .clone()
            .map(|c| width(&c.name))
            .filter(|&w| w <= MAX_NAME_WIDTH)
            .max()
            .unwrap_or(0);
        let value_w = fields
            .filter(|c| !c.note.is_empty())
            .map(|c| width(&c.value))
            .filter(|&w| w <= MAX_VALUE_WIDTH)
            .max()
            .unwrap_or(0);

        for (i, c) in children.iter().enumerate() {
            path.push(i);
            let last = i + 1 == children.len();
            let mut line = Line {
                spans: prefix.to_vec(),
                key: c.key,
                section: None,
            };
            let mut child_prefix = prefix.to_vec();
            match c.kind {
                Kind::Section => {
                    line.push(GLYPH, if last { "└─ " } else { "├─ " });
                    let folded = self.folds.map(|f| f(path));
                    if let Some(folded) = folded {
                        let marker = match (c.children.is_empty(), folded) {
                            (true, _) => "  ",
                            (false, true) => "▸ ",
                            (false, false) => "▾ ",
                        };
                        line.push(GLYPH, marker);
                    }
                    line.push(TITLE, &c.name);
                    Self::value_note(&mut line, &c.value, 0, &c.note);
                    line.section = Some(path.clone());
                    self.lines.push(line);
                    if folded == Some(true) {
                        path.pop();
                        continue;
                    }
                    child_prefix.push((GLYPH, (if last { "   " } else { "│  " }).into()));
                }
                Kind::Field => {
                    line.push(Style::new(), format!("{}{}", c.name, pad(&c.name, name_w)));
                    let value = if c.value.is_empty() { "-" } else { &c.value };
                    Self::value_note(&mut line, value, value_w, &c.note);
                    self.lines.push(line);
                    child_prefix.push((Style::new(), " ".repeat(name_w + 2)));
                }
                Kind::Text => {
                    line.push(Style::new(), &c.name);
                    self.lines.push(line);
                    child_prefix.push((Style::new(), "  ".into()));
                }
                Kind::Warning => {
                    line.push(WARNING, format!("! {}", c.name));
                    self.lines.push(line);
                }
            }
            self.children(c, path, &child_prefix);
            path.pop();
        }
    }
}

/// Render a top-level node as lines: a bold header line, then its children
/// as a tree.  With `folds`, sections get fold markers and folded sections
/// hide their contents.
pub fn lines(node: &Node, folds: Folds) -> Vec<Line> {
    let mut header = Line {
        key: node.key,
        section: Some(Vec::new()),
        ..Default::default()
    };
    header.push(HEADER, &node.name);
    Renderer::value_note(&mut header, &node.value, 0, &node.note);

    let mut r = Renderer {
        folds,
        lines: vec![header],
    };
    r.children(node, &mut Vec::new(), &[]);
    r.lines
}

/// Render a top-level node: a bold header line, then its children as a tree.
pub fn render(node: &Node) -> String {
    lines(node, None)
        .iter()
        .map(|line| line.to_ansi() + "\n")
        .collect()
}

/// Render rows as aligned columns under a dimmed header.  `styles` gives
/// the style of each column.
pub fn table(headers: &[&str], rows: &[Vec<String>], styles: &[Style]) -> String {
    let mut widths: Vec<usize> = headers.iter().map(|h| width(h)).collect();
    for row in rows {
        for (w, cell) in widths.iter_mut().zip(row) {
            *w = (*w).max(width(cell));
        }
    }
    let format_row = |cells: Vec<String>, raw: &[&str]| {
        let n = cells.len();
        let mut line = String::new();
        for (i, (cell, raw)) in cells.iter().zip(raw).enumerate() {
            line += cell;
            if i + 1 < n {
                line += &pad(raw, widths[i]);
                line += "  ";
            }
        }
        line + "\n"
    };

    let mut out = format_row(headers.iter().map(|h| styled(DIM, h)).collect(), headers);
    for row in rows {
        let raw: Vec<&str> = row.iter().map(String::as_str).collect();
        let cells = row
            .iter()
            .zip(styles)
            .map(|(cell, style)| styled(*style, cell))
            .collect();
        out += &format_row(cells, &raw);
    }
    out
}

/// Write rendered output to stdout (styles are stripped as appropriate).
/// Exits quietly if stdout is gone (e.g. `exausb | head`).
pub fn print(s: &str) {
    let mut stdout = anstream::stdout();
    if stdout
        .write_all(s.as_bytes())
        .and_then(|()| stdout.flush())
        .is_err()
    {
        std::process::exit(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::hex2;

    fn plain(s: &str) -> String {
        anstream::adapter::strip_str(s).to_string()
    }

    #[test]
    fn tree() {
        let mut dev = Node::new("Bus 002 Device 001: iPhone");
        dev.value = "05ac:12a8".into();
        let d = dev.section("Device Descriptor");
        d.field("bcdUSB", "2.10");
        d.field("bDeviceClass", 0)
            .note("Defined at Interface level");
        let c = dev.section("Configuration 1").note("PTP · 500mA");
        c.field("bmAttributes", hex2(0xc0u8)).text("Self Powered");
        c.section("Interface 0").field("bNumEndpoints", 3);
        c.warn("Descriptor too short");

        let expected = "\
Bus 002 Device 001: iPhone  05ac:12a8
├─ Device Descriptor
│  bcdUSB        2.10
│  bDeviceClass  0  Defined at Interface level
└─ Configuration 1  PTP · 500mA
   bmAttributes  0xc0
                 Self Powered
   ├─ Interface 0
   │  bNumEndpoints  3
   ! Descriptor too short
";
        assert_eq!(plain(&render(&dev)), expected);
    }

    #[test]
    fn aligned_table() {
        let rows = vec![
            vec!["002".into(), "05ac:12a8".into(), "iPhone".into()],
            vec!["010".into(), "1d6b:0002".into(), "2.0 root hub".into()],
        ];
        let out = table(&["BUS", "ID", "NAME"], &rows, &[Style::new(); 3]);
        assert_eq!(
            plain(&out),
            "BUS  ID         NAME\n002  05ac:12a8  iPhone\n010  1d6b:0002  2.0 root hub\n"
        );
    }
}
