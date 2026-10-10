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

/// `name  value  note` with the parts that are present.
fn line(name: String, value: &str, value_w: usize, note: &str) -> String {
    let mut out = name;
    if !value.is_empty() {
        out += "  ";
        out += &styled(VALUE, value);
    }
    if !note.is_empty() {
        out += &pad(value, value_w);
        out += "  ";
        out += &styled(NOTE, note);
    }
    out
}

fn render_children(children: &[Node], prefix: &str, out: &mut String) {
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
        let last = i + 1 == children.len();
        match c.kind {
            Kind::Section => {
                let branch = if last { "└─ " } else { "├─ " };
                let title = styled(TITLE, &c.name);
                out.push_str(&format!(
                    "{prefix}{}{}\n",
                    styled(GLYPH, branch),
                    line(title, &c.value, 0, &c.note)
                ));
                let cont = if last { "   " } else { "│  " };
                render_children(
                    &c.children,
                    &format!("{prefix}{}", styled(GLYPH, cont)),
                    out,
                );
            }
            Kind::Field => {
                let name = format!("{}{}", c.name, pad(&c.name, name_w));
                let value = if c.value.is_empty() { "-" } else { &c.value };
                out.push_str(&format!(
                    "{prefix}{}\n",
                    line(name, value, value_w, &c.note)
                ));
                render_children(
                    &c.children,
                    &format!("{prefix}{}", " ".repeat(name_w + 2)),
                    out,
                );
            }
            Kind::Text => {
                out.push_str(&format!("{prefix}{}\n", c.name));
                render_children(&c.children, &format!("{prefix}  "), out);
            }
            Kind::Warning => {
                out.push_str(&format!(
                    "{prefix}{}\n",
                    styled(WARNING, &format!("! {}", c.name))
                ));
            }
        }
    }
}

/// Render a top-level node: a bold header line, then its children as a tree.
pub fn render(node: &Node) -> String {
    let mut out = format!(
        "{}\n",
        line(styled(HEADER, &node.name), &node.value, 0, &node.note)
    );
    render_children(&node.children, "", &mut out);
    out
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
