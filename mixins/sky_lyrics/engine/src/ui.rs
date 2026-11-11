//! The settings screen as plain drawing and hit rectangles. It keeps to the
//! left of the window so the sky stays in view while you change things.

use crate::atlas::FontFile;
use crate::config::{Config, Kind, SPECS, STYLES, SWATCHES, Spec, Tab, Value, hex, parse_hex};

#[derive(Clone, Debug, PartialEq)]
pub enum Draw {
    Fill([f32; 4], u32),
    Text(String, [f32; 2], u32, bool),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    Tab(Tab),
    Slide(usize),
    Toggle(usize),
    Choice(usize),
    Swatch(usize, u32),
    Hex(usize),
    Fonts,
    Player(usize),
    Reset(usize),
    Style(usize),
    FontAuto,
    FontPick(usize),
    FontsClose,
    LookUpAgain,
    ReloadFile,
    Defaults,
    Done,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Widget {
    rect: [f32; 4],
    action: Action,
}

/// What a press asks of the mod around the screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    None,
    Changed,
    LookUpAgain,
    ReloadFile,
    Close,
}

pub struct Info<'a> {
    pub song: &'a str,
    pub status: &'a str,
    pub players: &'a [String],
    pub fonts: &'a [FontFile],
}

#[derive(Default)]
struct FontView {
    filter: String,
    scroll: usize,
}

pub struct Ui {
    tab: Tab,
    scroll: usize,
    max_scroll: usize,
    drag: Option<(usize, [f32; 4])>,
    hex: Option<(usize, String)>,
    fonts: Option<FontView>,
    widgets: Vec<Widget>,
    /// The font rows on the list now, as last drawn.
    shown_fonts: Vec<FontFile>,
}

impl Default for Ui {
    fn default() -> Self {
        Ui {
            tab: Tab::Position,
            scroll: 0,
            max_scroll: 0,
            drag: None,
            hex: None,
            fonts: None,
            widgets: Vec::new(),
            shown_fonts: Vec::new(),
        }
    }
}

const PANEL: u32 = 0xE0141020;
const EDGE: u32 = 0xFF3A2F55;
const BUTTON: u32 = 0xFF251D38;
const BUTTON_HOVER: u32 = 0xFF34294E;
const ACCENT: u32 = 0xFFA57CFF;
const ACCENT_DIM: u32 = 0xFF4A3B72;
const INK: u32 = 0xFFFFFF;
const MUTED: u32 = 0xA9A3BC;
const ROW: f32 = 16.0;
const GAP: f32 = 2.0;
const FONT: f32 = 8.0;

fn inside(r: [f32; 4], at: [f32; 2]) -> bool {
    at[0] >= r[0] && at[0] < r[0] + r[2] && at[1] >= r[1] && at[1] < r[1] + r[3]
}

struct Out<'a> {
    draws: Vec<Draw>,
    widgets: Vec<Widget>,
    mouse: [f32; 2],
    measure: &'a dyn Fn(&str) -> f32,
}

impl Out<'_> {
    fn fill(&mut self, r: [f32; 4], argb: u32) {
        self.draws.push(Draw::Fill(r, argb));
    }

    fn text(&mut self, text: &str, at: [f32; 2], rgb: u32) {
        self.draws.push(Draw::Text(text.to_owned(), at, rgb, true));
    }

    fn fit(&self, text: &str, max: f32) -> String {
        if (self.measure)(text) <= max {
            return text.to_owned();
        }
        let mut cut = text.to_owned();
        while !cut.is_empty() && (self.measure)(&format!("{cut}...")) > max {
            cut.pop();
        }
        format!("{cut}...")
    }

    fn label(&mut self, text: &str, x: f32, y: f32, max: f32, rgb: u32) {
        let shown = self.fit(text, max);
        self.text(&shown, [x, y + (ROW - FONT) / 2.0], rgb);
    }

    fn button(&mut self, r: [f32; 4], text: &str, action: Action, on: bool) {
        let hover = inside(r, self.mouse);
        self.fill(r, if hover { ACCENT } else { EDGE });
        let inner = [r[0] + 1.0, r[1] + 1.0, r[2] - 2.0, r[3] - 2.0];
        self.fill(inner, if on { ACCENT_DIM } else if hover { BUTTON_HOVER } else { BUTTON });
        let shown = self.fit(text, r[2] - 6.0);
        let w = (self.measure)(&shown);
        self.text(&shown, [r[0] + (r[2] - w) / 2.0, r[1] + (r[3] - FONT) / 2.0], INK);
        self.widgets.push(Widget { rect: r, action });
    }

    fn widget(&mut self, rect: [f32; 4], action: Action) {
        self.widgets.push(Widget { rect, action });
    }
}

fn specs_of(tab: Tab) -> Vec<usize> {
    (0..SPECS.len()).filter(|&i| SPECS[i].tab == tab).collect()
}

fn snap(spec: &Spec, value: f32) -> f32 {
    let Kind::Slider { min, max, step, .. } = spec.kind else {
        return value;
    };
    let snapped = min + ((value - min) / step).round() * step;
    let cleaned = (snapped * 1000.0).round() / 1000.0;
    cleaned.clamp(min, max)
}

impl Ui {
    pub fn close_views(&mut self) {
        self.drag = None;
        self.hex = None;
        self.fonts = None;
    }

    pub fn typing(&self) -> bool {
        self.hex.is_some() || self.fonts.is_some()
    }

    pub fn sliding(&self) -> bool {
        self.drag.is_some()
    }

    pub fn render(&mut self, cfg: &Config, size: [f32; 2], mouse: [f32; 2], info: &Info<'_>, measure: &dyn Fn(&str) -> f32) -> Vec<Draw> {
        let mut out = Out { draws: Vec::new(), widgets: Vec::new(), mouse, measure };
        let w = (size[0] - 16.0).min(330.0).max(120.0);
        let (x, y, h) = (8.0, 8.0, (size[1] - 16.0).max(80.0));
        out.fill([x - 1.0, y - 1.0, w + 2.0, h + 2.0], EDGE);
        out.fill([x, y, w, h], PANEL);
        out.text("Sky Lyrics", [x + 6.0, y + 6.0], INK);
        let status = out.fit(info.status, w - 76.0);
        let width = measure(&status);
        out.text(&status, [x + w - 6.0 - width, y + 6.0], MUTED);
        let inner_x = x + 6.0;
        let inner_w = w - 12.0;
        let top = y + 20.0;
        if self.fonts.is_some() {
            self.render_fonts(&mut out, cfg, inner_x, top, inner_w, y + h - 6.0, info);
        } else {
            self.render_tabs(&mut out, cfg, inner_x, top, inner_w, y + h - 24.0, info);
            let foot = y + h - 20.0;
            let third = (inner_w - 4.0) / 3.0;
            out.button([inner_x, foot, third, 14.0], "Reload file", Action::ReloadFile, false);
            out.button([inner_x + third + 2.0, foot, third, 14.0], "All defaults", Action::Defaults, false);
            out.button([inner_x + (third + 2.0) * 2.0, foot, third, 14.0], "Done", Action::Done, false);
        }
        self.widgets = out.widgets;
        out.draws
    }

    #[allow(clippy::too_many_arguments)]
    fn render_tabs(&mut self, out: &mut Out<'_>, cfg: &Config, x: f32, top: f32, w: f32, bottom: f32, info: &Info<'_>) {
        let tab_w = (w - (Tab::ALL.len() as f32 - 1.0) * 2.0) / Tab::ALL.len() as f32;
        for (i, tab) in Tab::ALL.iter().enumerate() {
            out.button([x + (tab_w + 2.0) * i as f32, top, tab_w, 14.0], tab.label(), Action::Tab(*tab), *tab == self.tab);
        }
        let mut y = top + 18.0;
        if self.tab == Tab::Styles {
            let half = (w - 2.0) / 2.0;
            for (i, (name, _)) in STYLES.iter().enumerate() {
                let (col, row) = ((i % 2) as f32, (i / 2) as f32);
                out.button([x + (half + 2.0) * col, y + (ROW + GAP) * row, half, ROW], name, Action::Style(i), false);
            }
            let row = STYLES.len().div_ceil(2) as f32;
            out.label("A style changes several settings: tune it after.", x, y + (ROW + GAP) * row, w, MUTED);
            self.max_scroll = 0;
            return;
        }
        if self.tab == Tab::Music {
            out.label(&format!("Now: {}", info.song), x, y, w, MUTED);
            y += ROW;
            let label = "Look up lyrics again";
            out.button([x, y, w, ROW - 2.0], label, Action::LookUpAgain, false);
            y += ROW + GAP;
        }
        let rows = specs_of(self.tab);
        let fits = (((bottom - y) / (ROW + GAP)).floor() as usize).max(1);
        self.max_scroll = rows.len().saturating_sub(fits);
        self.scroll = self.scroll.min(self.max_scroll);
        for &i in rows.iter().skip(self.scroll).take(fits) {
            self.render_row(out, cfg, i, x, y, w, info);
            y += ROW + GAP;
        }
        if self.max_scroll > 0 {
            let track = (bottom - (top + 18.0)).max(1.0);
            let thumb = (track * fits as f32 / rows.len() as f32).max(8.0);
            let at = (track - thumb) * self.scroll as f32 / self.max_scroll as f32;
            out.fill([x + w + 2.0, top + 18.0 + at, 2.0, thumb], ACCENT_DIM);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_row(&self, out: &mut Out<'_>, cfg: &Config, i: usize, x: f32, y: f32, w: f32, info: &Info<'_>) {
        let spec = &SPECS[i];
        let label_w = (w * 0.42).min(130.0);
        out.label(spec.label, x, y, label_w - 4.0, INK);
        let control_x = x + label_w;
        let reset_w = 12.0;
        let control_w = w - label_w - reset_w - 2.0;
        match spec.kind {
            Kind::Slider { min, max, .. } => {
                let value = if let Value::Num(v) = (spec.get)(cfg) { v } else { min };
                let text = spec.display(cfg);
                let text_w = (out.measure)(&text).max(38.0);
                let track = [control_x, y + 2.0, (control_w - text_w - 4.0).max(10.0), ROW - 4.0];
                let bar = [track[0], y + ROW / 2.0 - 1.5, track[2], 3.0];
                let t = ((value - min) / (max - min)).clamp(0.0, 1.0);
                out.fill(bar, ACCENT_DIM);
                out.fill([bar[0], bar[1], bar[2] * t, bar[3]], ACCENT);
                let knob = [bar[0] + bar[2] * t - 2.0, y + 3.0, 4.0, ROW - 6.0];
                out.fill(knob, if inside(track, out.mouse) { INK } else { 0xFFD8CCF5 });
                out.widget(track, Action::Slide(i));
                out.text(&text, [control_x + control_w - text_w, y + (ROW - FONT) / 2.0], MUTED);
            }
            Kind::Toggle => {
                let on = matches!((spec.get)(cfg), Value::Flag(true));
                out.button([control_x, y, control_w, ROW - 2.0], if on { "On" } else { "Off" }, Action::Toggle(i), on);
            }
            Kind::Choice(_) => {
                out.button([control_x, y, control_w, ROW - 2.0], &spec.display(cfg), Action::Choice(i), false);
            }
            Kind::Font => {
                out.button([control_x, y, control_w, ROW - 2.0], &spec.display(cfg), Action::Fonts, false);
            }
            Kind::Player => {
                let shown = format!("{} ({} found)", spec.display(cfg), info.players.len());
                out.button([control_x, y, control_w, ROW - 2.0], &shown, Action::Player(i), false);
            }
            Kind::Color => {
                let current = if let Value::Color(v) = (spec.get)(cfg) { v } else { 0 };
                let hex_w = 44.0;
                let swatch_room = control_w - hex_w - 2.0;
                let step = (swatch_room / SWATCHES.len() as f32).min(12.0);
                let side = (step - 1.0).min(ROW - 4.0).max(3.0);
                for (n, swatch) in SWATCHES.iter().enumerate() {
                    let r = [control_x + step * n as f32, y + 1.0, side, ROW - 4.0];
                    if *swatch == current {
                        out.fill([r[0] - 1.0, r[1] - 1.0, r[2] + 2.0, r[3] + 2.0], INK);
                    }
                    out.fill(r, 0xFF00_0000 | swatch);
                    out.widget(r, Action::Swatch(i, *swatch));
                }
                let field = [control_x + control_w - hex_w, y, hex_w, ROW - 2.0];
                let editing = self.hex.as_ref().filter(|(n, _)| *n == i);
                out.fill(field, if editing.is_some() { ACCENT } else { EDGE });
                out.fill([field[0] + 1.0, field[1] + 1.0, field[2] - 2.0, field[3] - 2.0], 0xFF0E0A18);
                let text = editing.map_or_else(|| hex(current), |(_, t)| format!("{t}_"));
                out.text(&text, [field[0] + 3.0, y + (ROW - 2.0 - FONT) / 2.0], if editing.is_some() { INK } else { MUTED });
                out.widget(field, Action::Hex(i));
            }
        }
        out.button([x + w - reset_w, y, reset_w, ROW - 2.0], "x", Action::Reset(i), false);
    }

    #[allow(clippy::too_many_arguments)]
    fn render_fonts(&mut self, out: &mut Out<'_>, cfg: &Config, x: f32, top: f32, w: f32, bottom: f32, info: &Info<'_>) {
        let (filter, scroll) = self.fonts.as_ref().map_or((String::new(), 0), |f| (f.filter.to_lowercase(), f.scroll));
        out.button([x + w - 44.0, top, 44.0, 14.0], "Back", Action::FontsClose, false);
        let search = format!("Search: {}_", self.fonts.as_ref().map_or("", |f| f.filter.as_str()));
        out.label(&search, x, top - 1.0, w - 50.0, INK);
        let list: Vec<&FontFile> = info.fonts.iter().filter(|f| f.name.to_lowercase().contains(&filter)).collect();
        let mut y = top + 18.0;
        out.button([x, y, w, ROW - 2.0], "Automatic", Action::FontAuto, cfg.font.is_empty());
        y += ROW;
        if info.fonts.is_empty() {
            out.label("Checking which fonts can write text...", x, y, w, MUTED);
        }
        let fits = (((bottom - y) / ROW).floor() as usize).max(1);
        self.max_scroll = list.len().saturating_sub(fits);
        let scroll = scroll.min(self.max_scroll);
        if let Some(view) = &mut self.fonts {
            view.scroll = scroll;
        }
        self.shown_fonts.clear();
        for (n, font) in list.iter().skip(scroll).take(fits).enumerate() {
            let on = cfg.font == font.path.to_string_lossy();
            out.button([x, y, w, ROW - 2.0], &font.name, Action::FontPick(n), on);
            self.shown_fonts.push((*font).clone());
            y += ROW;
        }
    }

    fn at_press(&self, mouse: [f32; 2]) -> Option<Widget> {
        self.widgets.iter().rev().find(|w| inside(w.rect, mouse)).copied()
    }

    fn slide_to(&self, cfg: &mut Config, i: usize, rect: [f32; 4], x: f32) {
        let spec = &SPECS[i];
        if let Kind::Slider { min, max, .. } = spec.kind {
            let t = ((x - rect[0]) / rect[2]).clamp(0.0, 1.0);
            (spec.set)(cfg, Value::Num(snap(spec, min + (max - min) * t)));
        }
    }

    /// A left press at `mouse`.
    pub fn press(&mut self, cfg: &mut Config, mouse: [f32; 2], info_players: &[String]) -> Effect {
        let Some(widget) = self.at_press(mouse) else {
            self.hex = None;
            return Effect::None;
        };
        if !matches!(widget.action, Action::Hex(_)) {
            self.commit_hex(cfg);
        }
        match widget.action {
            Action::Tab(tab) => {
                self.tab = tab;
                self.scroll = 0;
                Effect::None
            }
            Action::Slide(i) => {
                self.drag = Some((i, widget.rect));
                self.slide_to(cfg, i, widget.rect, mouse[0]);
                Effect::Changed
            }
            Action::Toggle(i) => {
                let spec = &SPECS[i];
                if let Value::Flag(v) = (spec.get)(cfg) {
                    (spec.set)(cfg, Value::Flag(!v));
                }
                Effect::Changed
            }
            Action::Choice(i) => {
                let spec = &SPECS[i];
                if let (Kind::Choice(options), Value::Pick(now)) = (spec.kind, (spec.get)(cfg)) {
                    let at = options.iter().position(|o| *o == now).unwrap_or(0);
                    (spec.set)(cfg, Value::Pick(options[(at + 1) % options.len()]));
                }
                Effect::Changed
            }
            Action::Swatch(i, rgb) => {
                (SPECS[i].set)(cfg, Value::Color(rgb));
                Effect::Changed
            }
            Action::Hex(i) => {
                let text = hex(if let Value::Color(v) = (SPECS[i].get)(cfg) { v } else { 0 });
                self.hex = Some((i, text));
                Effect::None
            }
            Action::Fonts => {
                self.fonts = Some(FontView::default());
                Effect::None
            }
            Action::Player(i) => {
                let spec = &SPECS[i];
                let now = spec.encode(cfg);
                let mut names = vec!["auto".to_owned()];
                names.extend(info_players.iter().cloned());
                names.dedup();
                let at = names.iter().position(|n| *n == now).unwrap_or(0);
                (spec.set)(cfg, Value::Text(names[(at + 1) % names.len()].clone()));
                Effect::Changed
            }
            Action::Reset(i) => {
                let spec = &SPECS[i];
                (spec.set)(cfg, (spec.get)(&Config::default()));
                Effect::Changed
            }
            Action::Style(i) => {
                cfg.apply_style(i);
                Effect::Changed
            }
            Action::FontAuto => {
                cfg.font.clear();
                Effect::Changed
            }
            Action::FontPick(n) => {
                if let Some(font) = self.shown_fonts.get(n) {
                    cfg.font = font.path.to_string_lossy().into_owned();
                }
                Effect::Changed
            }
            Action::FontsClose => {
                self.fonts = None;
                Effect::None
            }
            Action::LookUpAgain => Effect::LookUpAgain,
            Action::ReloadFile => Effect::ReloadFile,
            Action::Defaults => {
                let shown = cfg.shown;
                *cfg = Config { shown, ..Config::default() };
                Effect::Changed
            }
            Action::Done => {
                self.close_views();
                Effect::Close
            }
        }
    }

    /// The mouse moved with a slider held: true when one changed.
    pub fn dragged(&mut self, cfg: &mut Config, mouse: [f32; 2]) -> bool {
        match self.drag {
            Some((i, rect)) => {
                self.slide_to(cfg, i, rect, mouse[0]);
                true
            }
            None => false,
        }
    }

    /// True when a slider was let go, and the file should be written.
    pub fn released(&mut self) -> bool {
        self.drag.take().is_some()
    }

    pub fn wheel(&mut self, steps: f64) {
        let down = steps < 0.0;
        if let Some(view) = &mut self.fonts {
            view.scroll = if down { (view.scroll + 1).min(self.max_scroll) } else { view.scroll.saturating_sub(1) };
        } else {
            self.scroll = if down { (self.scroll + 1).min(self.max_scroll) } else { self.scroll.saturating_sub(1) };
        }
    }

    fn commit_hex(&mut self, cfg: &mut Config) -> bool {
        match self.hex.take() {
            Some((i, text)) => match parse_hex(&text) {
                Some(rgb) => {
                    (SPECS[i].set)(cfg, Value::Color(rgb));
                    true
                }
                None => false,
            },
            None => false,
        }
    }

    /// A key on the screen: true when the screen used it. Escape backs out of
    /// a field or the font list before it closes anything.
    pub fn key(&mut self, cfg: &mut Config, key: &str) -> (bool, Effect) {
        if self.hex.is_some() {
            match key {
                "key.keyboard.enter" | "key.keyboard.keypad.enter" => {
                    let changed = self.commit_hex(cfg);
                    return (true, if changed { Effect::Changed } else { Effect::None });
                }
                "key.keyboard.escape" => {
                    self.hex = None;
                    return (true, Effect::None);
                }
                "key.keyboard.backspace" => {
                    if let Some((_, text)) = &mut self.hex {
                        text.pop();
                    }
                    return (true, Effect::None);
                }
                _ => return (true, Effect::None),
            }
        }
        if let Some(view) = &mut self.fonts {
            match key {
                "key.keyboard.escape" => self.fonts = None,
                "key.keyboard.backspace" => {
                    view.filter.pop();
                    view.scroll = 0;
                }
                "key.keyboard.up" => view.scroll = view.scroll.saturating_sub(1),
                "key.keyboard.down" => view.scroll = (view.scroll + 1).min(self.max_scroll),
                _ => {}
            }
            return (true, Effect::None);
        }
        (false, Effect::None)
    }

    pub fn typed(&mut self, c: char) {
        if let Some((_, text)) = &mut self.hex {
            if (c.is_ascii_hexdigit() || c == '#') && text.len() < 7 {
                text.push(c);
            }
        } else if let Some(view) = &mut self.fonts {
            if !c.is_control() && view.filter.len() < 40 {
                view.filter.push(c);
                view.scroll = 0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::spec;

    fn measure(text: &str) -> f32 {
        text.chars().count() as f32 * 6.0
    }

    fn info<'a>(players: &'a [String], fonts: &'a [FontFile]) -> Info<'a> {
        Info { song: "A - B", status: "ok", players, fonts }
    }

    fn drawn(ui: &mut Ui, cfg: &Config, players: &[String], fonts: &[FontFile]) -> Vec<Draw> {
        ui.render(cfg, [640.0, 360.0], [0.0, 0.0], &info(players, fonts), &measure)
    }

    fn centre_of(ui: &Ui, wanted: impl Fn(&Action) -> bool) -> [f32; 2] {
        let w = ui.widgets.iter().find(|w| wanted(&w.action)).expect("widget on screen");
        [w.rect[0] + w.rect[2] / 2.0, w.rect[1] + w.rect[3] / 2.0]
    }

    fn index(key: &str) -> usize {
        SPECS.iter().position(|s| std::ptr::eq(s, spec(key).unwrap())).unwrap()
    }

    #[test]
    fn a_slider_follows_the_mouse_and_snaps() {
        let (mut ui, mut cfg) = (Ui::default(), Config::default());
        drawn(&mut ui, &cfg, &[], &[]);
        let i = index("elevation");
        let w = *ui.widgets.iter().find(|w| w.action == Action::Slide(i)).unwrap();
        let far_right = [w.rect[0] + w.rect[2] + 50.0, w.rect[1] + 1.0];
        let start = [w.rect[0] + 1.0, w.rect[1] + 1.0];
        assert_eq!(ui.press(&mut cfg, start, &[]), Effect::Changed);
        assert!(cfg.elevation < -30.0);
        assert!(ui.dragged(&mut cfg, far_right));
        assert_eq!(cfg.elevation, 85.0);
        assert!(ui.released());
        assert!(!ui.dragged(&mut cfg, start), "let go");
    }

    #[test]
    fn choices_cycle_and_the_small_button_resets() {
        let (mut ui, mut cfg) = (Ui::default(), Config::default());
        drawn(&mut ui, &cfg, &[], &[]);
        let i = index("anchor");
        let at = centre_of(&ui, |a| *a == Action::Choice(i));
        ui.press(&mut cfg, at, &[]);
        assert_eq!(cfg.anchor, "compass");
        let reset = centre_of(&ui, |a| *a == Action::Reset(i));
        ui.press(&mut cfg, reset, &[]);
        assert_eq!(cfg.anchor, "follow");
    }

    #[test]
    fn colours_take_swatches_and_typed_hex() {
        let (mut ui, mut cfg) = (Ui::default(), Config::default());
        drawn(&mut ui, &cfg, &[], &[]);
        let tab = centre_of(&ui, |a| *a == Action::Tab(Tab::Colour));
        ui.press(&mut cfg, tab, &[]);
        drawn(&mut ui, &cfg, &[], &[]);
        let i = index("color");
        let swatch = centre_of(&ui, |a| *a == Action::Swatch(i, SWATCHES[2]));
        ui.press(&mut cfg, swatch, &[]);
        assert_eq!(cfg.color, SWATCHES[2]);
        drawn(&mut ui, &cfg, &[], &[]);
        let field = centre_of(&ui, |a| *a == Action::Hex(i));
        ui.press(&mut cfg, field, &[]);
        assert!(ui.typing());
        for _ in 0..7 {
            ui.key(&mut cfg, "key.keyboard.backspace");
        }
        for c in "#12ab34".chars() {
            ui.typed(c);
        }
        assert_eq!(ui.key(&mut cfg, "key.keyboard.enter"), (true, Effect::Changed));
        assert_eq!(cfg.color, 0x12AB34);
        assert!(!ui.typing());
    }

    #[test]
    fn the_font_list_filters_and_picks() {
        let fonts = vec![
            FontFile { name: "Alpha".into(), path: "/a.ttf".into() },
            FontFile { name: "Beta".into(), path: "/b.ttf".into() },
        ];
        let (mut ui, mut cfg) = (Ui::default(), Config::default());
        drawn(&mut ui, &cfg, &[], &fonts);
        ui.press(&mut cfg, centre_of(&ui, |a| *a == Action::Tab(Tab::Text)), &[]);
        drawn(&mut ui, &cfg, &[], &fonts);
        ui.press(&mut cfg, centre_of(&ui, |a| *a == Action::Fonts), &[]);
        ui.typed('b');
        drawn(&mut ui, &cfg, &[], &fonts);
        let pick = centre_of(&ui, |a| *a == Action::FontPick(0));
        assert_eq!(ui.press(&mut cfg, pick, &[]), Effect::Changed);
        assert_eq!(cfg.font, "/b.ttf");
        assert_eq!(ui.key(&mut cfg, "key.keyboard.escape"), (true, Effect::None));
        assert!(!ui.typing());
    }

    #[test]
    fn the_player_row_cycles_through_those_found() {
        let players = vec!["spotify".to_owned(), "firefox".to_owned()];
        let (mut ui, mut cfg) = (Ui::default(), Config::default());
        drawn(&mut ui, &cfg, &players, &[]);
        ui.press(&mut cfg, centre_of(&ui, |a| *a == Action::Tab(Tab::Music)), &players);
        drawn(&mut ui, &cfg, &players, &[]);
        let i = index("player");
        let at = centre_of(&ui, |a| *a == Action::Player(i));
        ui.press(&mut cfg, at, &players);
        assert_eq!(cfg.player, "spotify");
        ui.press(&mut cfg, at, &players);
        assert_eq!(cfg.player, "firefox");
        ui.press(&mut cfg, at, &players);
        assert_eq!(cfg.player, "auto");
    }

    #[test]
    fn long_tabs_scroll_and_every_row_can_be_reached() {
        let mut ui = Ui::default();
        let cfg = Config::default();
        ui.render(&cfg, [640.0, 180.0], [0.0, 0.0], &info(&[], &[]), &measure);
        assert!(ui.max_scroll > 0);
        let last = *specs_of(Tab::Position).last().unwrap();
        for _ in 0..40 {
            ui.wheel(-1.0);
        }
        ui.render(&cfg, [640.0, 180.0], [0.0, 0.0], &info(&[], &[]), &measure);
        assert!(ui.widgets.iter().any(|w| w.action == Action::Reset(last)));
    }

    #[test]
    fn styles_apply_and_done_closes() {
        let (mut ui, mut cfg) = (Ui::default(), Config::default());
        drawn(&mut ui, &cfg, &[], &[]);
        ui.press(&mut cfg, centre_of(&ui, |a| *a == Action::Tab(Tab::Styles)), &[]);
        drawn(&mut ui, &cfg, &[], &[]);
        ui.press(&mut cfg, centre_of(&ui, |a| *a == Action::Style(1)), &[]);
        assert_eq!(cfg.glow, 90.0);
        assert_eq!(ui.press(&mut cfg, centre_of(&ui, |a| *a == Action::Done), &[]), Effect::Close);
    }
}
