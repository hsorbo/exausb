// SPDX-License-Identifier: GPL-2.0-or-later
//! `exausb -i`: interactive mode.
//!
//! The device tree (as in `-t`) on the left and the selected device's
//! descriptors (as in `-v`) on the right, as a foldable tree.  The device
//! list refreshes every second, so plugging and unplugging shows up live.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io;
use std::time::{Duration, Instant};

use anstyle::AnsiColor;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line as TuiLine, Span as TuiSpan};
use ratatui::widgets::{Block, Paragraph};
use ratatui::{DefaultTerminal, Frame};
use rusb::UsbContext;

use crate::model::Node;
use crate::render::{self, Line};
use crate::usb::{self, Context, Device};
use crate::{dump, tree};

const REFRESH: Duration = Duration::from_secs(1);

/// Convert an `anstyle` style (as used by the renderer) to ratatui.
fn convert_style(s: anstyle::Style) -> Style {
    let mut style = Style::default();
    if let Some(anstyle::Color::Ansi(c)) = s.get_fg_color() {
        style = style.fg(match c {
            AnsiColor::Black => Color::Black,
            AnsiColor::Red => Color::Red,
            AnsiColor::Green => Color::Green,
            AnsiColor::Yellow => Color::Yellow,
            AnsiColor::Blue => Color::Blue,
            AnsiColor::Magenta => Color::Magenta,
            AnsiColor::Cyan => Color::Cyan,
            AnsiColor::White => Color::Gray,
            AnsiColor::BrightBlack => Color::DarkGray,
            AnsiColor::BrightRed => Color::LightRed,
            AnsiColor::BrightGreen => Color::LightGreen,
            AnsiColor::BrightYellow => Color::LightYellow,
            AnsiColor::BrightBlue => Color::LightBlue,
            AnsiColor::BrightMagenta => Color::LightMagenta,
            AnsiColor::BrightCyan => Color::LightCyan,
            AnsiColor::BrightWhite => Color::White,
        });
    }
    let effects = s.get_effects();
    if effects.contains(anstyle::Effects::BOLD) {
        style = style.add_modifier(Modifier::BOLD);
    }
    if effects.contains(anstyle::Effects::DIMMED) {
        style = style.add_modifier(Modifier::DIM);
    }
    style
}

fn convert_line(line: &Line) -> TuiLine<'static> {
    TuiLine::from(
        line.spans
            .iter()
            .map(|(style, text)| TuiSpan::styled(text.clone(), convert_style(*style)))
            .collect::<Vec<_>>(),
    )
}

/// Keep `cursor` visible in a window of `height` lines starting at `scroll`.
fn scroll_to(scroll: usize, cursor: usize, height: usize) -> usize {
    if cursor < scroll {
        cursor
    } else if height > 0 && cursor >= scroll + height {
        cursor + 1 - height
    } else {
        scroll
    }
}

/// The left pane: the device tree.  The selection is a device key, so it
/// survives refreshes.
#[derive(Default)]
struct DeviceList {
    lines: Vec<Line>,
    selected: Option<u32>,
    scroll: usize,
}

impl DeviceList {
    fn set_tree(&mut self, busses: &[Node]) {
        self.lines = busses.iter().flat_map(|b| render::lines(b, None)).collect();
        let keys: Vec<u32> = self.keys().collect();
        if !self.selected.is_some_and(|k| keys.contains(&k)) {
            self.selected = keys.first().copied();
        }
    }

    fn keys(&self) -> impl Iterator<Item = u32> + '_ {
        self.lines.iter().filter_map(|l| l.key)
    }

    fn selected_line(&self) -> Option<usize> {
        self.lines
            .iter()
            .position(|l| l.key.is_some() && l.key == self.selected)
    }

    /// Move the selection by `delta` devices (clamped).
    fn step(&mut self, delta: isize) {
        let keys: Vec<u32> = self.keys().collect();
        if keys.is_empty() {
            return;
        }
        let pos = self
            .selected
            .and_then(|k| keys.iter().position(|&x| x == k))
            .unwrap_or(0) as isize;
        let new = (pos + delta).clamp(0, keys.len() as isize - 1);
        self.selected = Some(keys[new as usize]);
    }
}

/// The right pane: a device's descriptors as a foldable tree.
#[derive(Default)]
struct Details {
    /// Expanded sections, by path; everything else is folded.
    expanded: HashSet<Vec<usize>>,
    cursor: usize,
    scroll: usize,
}

impl Details {
    fn lines(&self, node: &Node) -> Vec<Line> {
        let folded = |path: &[usize]| !self.expanded.contains(path);
        render::lines(node, Some(&folded))
    }

    /// The foldable section at the cursor, if any.
    fn section_at_cursor(&self, lines: &[Line]) -> Option<Vec<usize>> {
        lines
            .get(self.cursor)
            .and_then(|l| l.section.clone())
            .filter(|p| !p.is_empty())
    }

    fn toggle(&mut self, lines: &[Line]) {
        if let Some(path) = self.section_at_cursor(lines) {
            if !self.expanded.remove(&path) {
                self.expanded.insert(path);
            }
        }
    }

    /// Expand (`true`) or fold (`false`) the section at the cursor.
    /// Returns false if there was nothing to change.
    fn set_expanded(&mut self, lines: &[Line], expand: bool) -> bool {
        match self.section_at_cursor(lines) {
            Some(path) if expand => self.expanded.insert(path),
            Some(path) => self.expanded.remove(&path),
            None => false,
        }
    }

    fn expand_all(&mut self, node: &Node) {
        fn walk(node: &Node, path: &mut Vec<usize>, out: &mut HashSet<Vec<usize>>) {
            for (i, c) in node.children.iter().enumerate() {
                path.push(i);
                if !c.children.is_empty() {
                    out.insert(path.clone());
                    walk(c, path, out);
                }
                path.pop();
            }
        }
        walk(node, &mut Vec::new(), &mut self.expanded);
    }

    fn step(&mut self, delta: isize, len: usize) {
        let max = len.saturating_sub(1) as isize;
        self.cursor = (self.cursor as isize + delta).clamp(0, max) as usize;
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Focus {
    Devices,
    Details,
}

/// What the device list looked like at the last refresh.
type Snapshot = BTreeMap<u32, (u16, u16, Vec<u8>)>;

/// Access to the actual devices.
struct Loader {
    ctx: Context,
    devices: HashMap<u32, Device>,
    snapshot: Snapshot,
}

impl Loader {
    /// Re-read the device list.  Returns the new tree if anything changed.
    fn refresh(&mut self) -> Option<Vec<Node>> {
        let list = self.ctx.devices().ok()?;
        let mut snapshot = Snapshot::new();
        let mut devices = HashMap::new();
        for dev in list.iter() {
            let desc = usb::device_descriptor(&dev);
            let key = tree::device_key(&dev);
            let ports = dev.port_numbers().unwrap_or_default();
            snapshot.insert(key, (desc.id_vendor, desc.id_product, ports));
            devices.insert(key, dev);
        }
        if snapshot == self.snapshot {
            return None;
        }
        self.snapshot = snapshot;
        self.devices = devices;
        tree::tree(&self.ctx, false).ok()
    }
}

struct App {
    loader: Option<Loader>,
    devices: DeviceList,
    /// Decoded descriptors, per device key.
    details: HashMap<u32, Node>,
    /// Fold/cursor state, per device key.
    views: HashMap<u32, Details>,
    focus: Focus,
    /// Height of the details pane at the last draw (for paging).
    page: usize,
    quit: bool,
}

impl App {
    fn new(loader: Option<Loader>) -> App {
        App {
            loader,
            devices: DeviceList::default(),
            details: HashMap::new(),
            views: HashMap::new(),
            focus: Focus::Devices,
            page: 10,
            quit: false,
        }
    }

    fn refresh(&mut self, force: bool) {
        let Some(loader) = &mut self.loader else {
            return;
        };
        if force {
            loader.snapshot.clear();
            self.details.clear();
        }
        if let Some(busses) = loader.refresh() {
            // Forget devices that are gone (or were replaced).
            let snapshot = &loader.snapshot;
            self.details.retain(|k, _| snapshot.contains_key(k));
            self.views.retain(|k, _| snapshot.contains_key(k));
            self.devices.set_tree(&busses);
        }
    }

    /// The selected device's decoded descriptors (decoded on first use).
    fn selected_details(&mut self) -> Option<(&Node, &mut Details)> {
        let key = self.devices.selected?;
        if !self.details.contains_key(&key) {
            let dev = self.loader.as_ref()?.devices.get(&key)?;
            self.details.insert(key, dump::dumpdev(dev));
        }
        Some((&self.details[&key], self.views.entry(key).or_default()))
    }

    fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        let page = self.page.max(1) as isize;
        match code {
            KeyCode::Char('q') | KeyCode::Esc => self.quit = true,
            KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => self.quit = true,
            KeyCode::Char('r') => self.refresh(true),
            KeyCode::Tab | KeyCode::BackTab => {
                self.focus = match self.focus {
                    Focus::Devices => Focus::Details,
                    Focus::Details => Focus::Devices,
                }
            }
            _ => match self.focus {
                Focus::Devices => match code {
                    KeyCode::Up | KeyCode::Char('k') => self.devices.step(-1),
                    KeyCode::Down | KeyCode::Char('j') => self.devices.step(1),
                    KeyCode::PageUp => self.devices.step(-page),
                    KeyCode::PageDown => self.devices.step(page),
                    KeyCode::Home | KeyCode::Char('g') => self.devices.step(isize::MIN / 2),
                    KeyCode::End | KeyCode::Char('G') => self.devices.step(isize::MAX / 2),
                    KeyCode::Right | KeyCode::Char('l') | KeyCode::Enter => {
                        self.focus = Focus::Details
                    }
                    _ => {}
                },
                Focus::Details => {
                    let mut back_to_devices = false;
                    let Some((node, view)) = self.selected_details() else {
                        return;
                    };
                    let lines = view.lines(node);
                    match code {
                        KeyCode::Up | KeyCode::Char('k') => view.step(-1, lines.len()),
                        KeyCode::Down | KeyCode::Char('j') => view.step(1, lines.len()),
                        KeyCode::PageUp => view.step(-page, lines.len()),
                        KeyCode::PageDown => view.step(page, lines.len()),
                        KeyCode::Home | KeyCode::Char('g') => view.cursor = 0,
                        KeyCode::End | KeyCode::Char('G') => view.step(isize::MAX / 2, lines.len()),
                        KeyCode::Enter | KeyCode::Char(' ') => view.toggle(&lines),
                        KeyCode::Right | KeyCode::Char('l') => {
                            view.set_expanded(&lines, true);
                        }
                        KeyCode::Left | KeyCode::Char('h') => {
                            back_to_devices = !view.set_expanded(&lines, false);
                        }
                        KeyCode::Char('e') => view.expand_all(node),
                        KeyCode::Char('c') => {
                            view.expanded.clear();
                            view.cursor = 0;
                        }
                        _ => {}
                    }
                    if back_to_devices {
                        self.focus = Focus::Devices;
                    }
                }
            },
        }
    }

    fn draw(&mut self, frame: &mut Frame) {
        let [main, status] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(frame.area());
        let tree_width = self
            .devices
            .lines
            .iter()
            .map(|l| l.text().chars().count() as u16 + 2)
            .max()
            .unwrap_or(20)
            .clamp(24, (main.width * 2 / 5).max(24));
        let [left, right] =
            Layout::horizontal([Constraint::Length(tree_width), Constraint::Min(20)]).areas(main);

        self.draw_devices(frame, left);
        self.draw_details(frame, right);

        let hints = match self.focus {
            Focus::Devices => "↑↓ select  →/Tab details  r refresh  q quit",
            Focus::Details => {
                "↑↓ move  ⏎/←→ fold  e expand all  c collapse all  Tab devices  q quit"
            }
        };
        frame.render_widget(
            Paragraph::new(hints).style(Style::default().add_modifier(Modifier::DIM)),
            status,
        );
    }

    fn block(&self, title: &'static str, pane: Focus) -> Block<'static> {
        let color = if self.focus == pane {
            Color::Cyan
        } else {
            Color::DarkGray
        };
        Block::bordered()
            .title(title)
            .border_style(Style::default().fg(color))
    }

    fn draw_devices(&mut self, frame: &mut Frame, area: Rect) {
        let block = self.block(" Devices ", Focus::Devices);
        let inner = block.inner(area);
        let height = inner.height as usize;
        let selected = self.devices.selected_line();
        if let Some(sel) = selected {
            self.devices.scroll = scroll_to(self.devices.scroll, sel, height);
        }
        let lines: Vec<TuiLine> = self
            .devices
            .lines
            .iter()
            .enumerate()
            .skip(self.devices.scroll)
            .take(height)
            .map(|(i, l)| {
                let line = convert_line(l);
                if Some(i) == selected {
                    line.style(Style::default().add_modifier(Modifier::REVERSED))
                } else {
                    line
                }
            })
            .collect();
        let text = if lines.is_empty() {
            vec![TuiLine::from("No USB devices")]
        } else {
            lines
        };
        frame.render_widget(Paragraph::new(text).block(block), area);
    }

    fn draw_details(&mut self, frame: &mut Frame, area: Rect) {
        let block = self.block(" Details ", Focus::Details);
        let inner = block.inner(area);
        let height = inner.height as usize;
        self.page = height;
        let focused = self.focus == Focus::Details;

        let has_selection = self.devices.selected.is_some();
        let Some((node, view)) = self.selected_details() else {
            let msg = if has_selection {
                "Device not available"
            } else {
                "No device selected"
            };
            frame.render_widget(Paragraph::new(msg).block(block), area);
            return;
        };
        let lines = view.lines(node);
        view.cursor = view.cursor.min(lines.len().saturating_sub(1));
        view.scroll = scroll_to(view.scroll, view.cursor, height);
        let cursor = view.cursor;
        let text: Vec<TuiLine> = lines
            .iter()
            .enumerate()
            .skip(view.scroll)
            .take(height)
            .map(|(i, l)| {
                let line = convert_line(l);
                if focused && i == cursor {
                    line.style(Style::default().bg(Color::DarkGray))
                } else {
                    line
                }
            })
            .collect();
        frame.render_widget(Paragraph::new(text).block(block), area);
    }

    fn run(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        let mut last_refresh = Instant::now();
        while !self.quit {
            terminal.draw(|frame| self.draw(frame))?;
            let timeout = REFRESH.saturating_sub(last_refresh.elapsed());
            if event::poll(timeout)? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        self.handle_key(key.code, key.modifiers);
                    }
                }
            }
            if last_refresh.elapsed() >= REFRESH {
                self.refresh(false);
                last_refresh = Instant::now();
            }
        }
        Ok(())
    }
}

/// Run the interactive mode until the user quits.
pub fn run(ctx: Context) -> io::Result<()> {
    let mut app = App::new(Some(Loader {
        ctx,
        devices: HashMap::new(),
        snapshot: Snapshot::new(),
    }));
    app.refresh(true);
    ratatui::run(|terminal| app.run(terminal))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn device(name: &str, key: u32) -> Node {
        let mut n = Node::new(name);
        n.key(key);
        n
    }

    fn sample_tree() -> Vec<Node> {
        let mut bus = Node::new("Bus 001");
        let mut hub = device("Port 1  Hub", 0x0102);
        hub.children.push(device("Port 2  Mouse", 0x0105));
        bus.children.push(hub);
        bus.children.push(device("Port 3  Phone", 0x0103));
        vec![bus]
    }

    fn sample_details() -> Node {
        let mut dev = Node::new("Bus 001 Device 005  Mouse");
        dev.section("Device Descriptor").field("bcdUSB", "2.00");
        let cfg = dev.section("Configuration 1").note("100mA");
        cfg.field("bNumInterfaces", 1);
        cfg.section("Interface 0").field("bNumEndpoints", 1);
        dev
    }

    fn app() -> App {
        let mut app = App::new(None);
        app.devices.set_tree(&sample_tree());
        app.details.insert(0x0105, sample_details());
        app
    }

    fn screen(app: &mut App, w: u16, h: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        terminal.draw(|f| app.draw(f)).unwrap();
        let buf = terminal.backend().buffer().clone();
        (0..h)
            .map(|y| {
                (0..w)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn device_selection() {
        let mut app = app();
        // The first device in tree order is selected; buses aren't selectable.
        assert_eq!(app.devices.selected, Some(0x0102));
        app.handle_key(KeyCode::Down, KeyModifiers::NONE);
        assert_eq!(app.devices.selected, Some(0x0105));
        app.handle_key(KeyCode::End, KeyModifiers::NONE);
        assert_eq!(app.devices.selected, Some(0x0103));
        app.handle_key(KeyCode::Down, KeyModifiers::NONE);
        assert_eq!(app.devices.selected, Some(0x0103));

        // The selection survives a refresh that keeps the device...
        app.devices.set_tree(&sample_tree());
        assert_eq!(app.devices.selected, Some(0x0103));
        // ... and falls back to the first device when it's gone.
        let mut tree = sample_tree();
        tree[0].children.pop();
        app.devices.set_tree(&tree);
        assert_eq!(app.devices.selected, Some(0x0102));
    }

    #[test]
    fn folding() {
        let mut app = app();
        app.handle_key(KeyCode::Down, KeyModifiers::NONE);
        app.handle_key(KeyCode::Tab, KeyModifiers::NONE);
        assert_eq!(app.focus, Focus::Details);

        let lines = |app: &mut App| {
            let (node, view) = app.selected_details().unwrap();
            view.lines(node).iter().map(Line::text).collect::<Vec<_>>()
        };
        // Everything starts folded.
        assert_eq!(
            lines(&mut app),
            [
                "Bus 001 Device 005  Mouse",
                "├─ ▸ Device Descriptor",
                "└─ ▸ Configuration 1  100mA",
            ]
        );

        // Down twice to the configuration, expand it.
        app.handle_key(KeyCode::Down, KeyModifiers::NONE);
        app.handle_key(KeyCode::Down, KeyModifiers::NONE);
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(
            lines(&mut app)[2..],
            [
                "└─ ▾ Configuration 1  100mA",
                "   bNumInterfaces  1",
                "   └─ ▸ Interface 0",
            ]
        );

        // Left folds it again; left on a folded line goes back to the list.
        app.handle_key(KeyCode::Left, KeyModifiers::NONE);
        assert_eq!(lines(&mut app).len(), 3);
        app.handle_key(KeyCode::Left, KeyModifiers::NONE);
        assert_eq!(app.focus, Focus::Devices);

        // Expand all / collapse all.
        app.handle_key(KeyCode::Tab, KeyModifiers::NONE);
        app.handle_key(KeyCode::Char('e'), KeyModifiers::NONE);
        assert_eq!(lines(&mut app).len(), 7);
        app.handle_key(KeyCode::Char('c'), KeyModifiers::NONE);
        assert_eq!(lines(&mut app).len(), 3);
    }

    #[test]
    fn draws_both_panes() {
        let mut app = app();
        app.handle_key(KeyCode::Down, KeyModifiers::NONE);
        let screen = screen(&mut app, 80, 12);
        assert!(screen.contains("Devices"), "{screen}");
        assert!(screen.contains("Bus 001"), "{screen}");
        assert!(screen.contains("Port 2  Mouse"), "{screen}");
        assert!(screen.contains("▸ Configuration 1  100mA"), "{screen}");
        assert!(screen.contains("q quit"), "{screen}");

        // Without decoded details (no loader in tests) the pane says so.
        app.handle_key(KeyCode::Down, KeyModifiers::NONE);
        assert!(screen_contains(&mut app, "Device not available"));
    }

    fn screen_contains(app: &mut App, s: &str) -> bool {
        screen(app, 80, 12).contains(s)
    }

    #[test]
    fn scrolling() {
        assert_eq!(scroll_to(0, 5, 10), 0);
        assert_eq!(scroll_to(0, 12, 10), 3);
        assert_eq!(scroll_to(5, 2, 10), 2);
    }
}
