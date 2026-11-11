//! Every setting, once: its field, default, control and label. The struct, the
//! file format and the settings screen all come from the one table.

use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Position,
    Text,
    Colour,
    Motion,
    Music,
    Styles,
}

impl Tab {
    pub const ALL: [Tab; 6] = [Tab::Position, Tab::Text, Tab::Colour, Tab::Motion, Tab::Music, Tab::Styles];

    pub fn label(self) -> &'static str {
        match self {
            Tab::Position => "Place",
            Tab::Text => "Text",
            Tab::Colour => "Colour",
            Tab::Motion => "Motion",
            Tab::Music => "Music",
            Tab::Styles => "Styles",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Num(f32),
    Flag(bool),
    Pick(&'static str),
    Color(u32),
    Text(String),
}

impl From<f32> for Value {
    fn from(v: f32) -> Self {
        Value::Num(v)
    }
}
impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Value::Flag(v)
    }
}
impl From<&'static str> for Value {
    fn from(v: &'static str) -> Self {
        Value::Pick(v)
    }
}
impl From<u32> for Value {
    fn from(v: u32) -> Self {
        Value::Color(v)
    }
}
impl From<String> for Value {
    fn from(v: String) -> Self {
        Value::Text(v)
    }
}

/// How a setting is changed on the screen.
#[derive(Clone, Copy, Debug)]
pub enum Kind {
    Slider { min: f32, max: f32, step: f32, unit: &'static str },
    Toggle,
    Choice(&'static [&'static str]),
    Color,
    Font,
    Player,
}

pub struct Spec {
    pub key: &'static str,
    pub label: &'static str,
    pub tab: Tab,
    pub kind: Kind,
    pub get: fn(&Config) -> Value,
    pub set: fn(&mut Config, Value),
}

const fn slider(min: f32, max: f32, step: f32, unit: &'static str) -> Kind {
    Kind::Slider { min, max, step, unit }
}

macro_rules! settings {
    ($([$tab:ident] $($key:ident $ty:tt = $default:expr, $kind:expr, $label:literal;)+)+) => {
        #[derive(Clone, Debug, PartialEq)]
        pub struct Config { $($(pub $key: settings!(@ty $ty),)+)+ }

        impl Default for Config {
            fn default() -> Self {
                Config { $($($key: $default.into(),)+)+ }
            }
        }

        pub static SPECS: &[Spec] = &[$($(Spec {
            key: stringify!($key),
            label: $label,
            tab: Tab::$tab,
            kind: $kind,
            get: |c| c.$key.clone().into(),
            set: |c, v| settings!(@set $ty, c.$key, v),
        },)+)+];
    };
    (@ty num) => { f32 };
    (@ty flag) => { bool };
    (@ty pick) => { &'static str };
    (@ty color) => { u32 };
    (@ty text) => { String };
    (@set num, $f:expr, $v:ident) => { if let Value::Num(x) = $v { $f = x; } };
    (@set flag, $f:expr, $v:ident) => { if let Value::Flag(x) = $v { $f = x; } };
    (@set pick, $f:expr, $v:ident) => { if let Value::Pick(x) = $v { $f = x; } };
    (@set color, $f:expr, $v:ident) => { if let Value::Color(x) = $v { $f = x; } };
    (@set text, $f:expr, $v:ident) => { if let Value::Text(x) = $v { $f = x; } };
}

settings! {
    [Position]
        shown flag = true, Kind::Toggle, "Show lyrics";
        anchor pick = "follow", Kind::Choice(&["follow", "compass", "locked"]), "Anchor";
        elevation num = 22.0f32, slider(-40.0, 85.0, 1.0, " deg"), "Height in the sky";
        side num = 0.0f32, slider(-90.0, 90.0, 1.0, " deg"), "Left / right";
        compass num = 180.0f32, slider(0.0, 360.0, 1.0, " deg"), "Compass heading";
        follow_speed num = 2.5f32, slider(0.1, 12.0, 0.1, ""), "Follow speed";
        deadzone num = 12.0f32, slider(0.0, 60.0, 1.0, " deg"), "Follow dead zone";
        follow_pitch flag = false, Kind::Toggle, "Follow looking up / down";
        facing pick = "viewer", Kind::Choice(&["viewer", "upright"]), "Facing";
        curve num = 30.0f32, slider(0.0, 100.0, 1.0, "%"), "Curve around you";
        tilt num = 0.0f32, slider(-45.0, 45.0, 1.0, " deg"), "Tilt";
        lean num = 0.0f32, slider(-60.0, 60.0, 1.0, " deg"), "Lean back";
        sway num = 30.0f32, slider(0.0, 100.0, 1.0, "%"), "Float amount";
        sway_speed num = 1.0f32, slider(0.1, 4.0, 0.1, "x"), "Float speed";
    [Text]
        font text = "", Kind::Font, "Font";
        size num = 13.0f32, slider(1.0, 30.0, 0.1, ""), "Size";
        weight num = 0.0f32, slider(-40.0, 60.0, 1.0, "%"), "Weight";
        italic num = 0.0f32, slider(-25.0, 25.0, 1.0, " deg"), "Slant";
        letter_spacing num = 0.0f32, slider(-10.0, 60.0, 1.0, "%"), "Letter spacing";
        word_spacing num = 100.0f32, slider(50.0, 400.0, 5.0, "%"), "Word spacing";
        line_height num = 125.0f32, slider(80.0, 250.0, 5.0, "%"), "Line height";
        wrap num = 34.0f32, slider(8.0, 120.0, 1.0, " ch"), "Wrap width";
        align pick = "center", Kind::Choice(&["center", "left", "right"]), "Alignment";
        case pick = "original", Kind::Choice(&["original", "upper", "lower"]), "Letter case";
    [Colour]
        color_mode pick = "sky", Kind::Choice(&["sky", "solid", "gradient", "rainbow"]), "Colour mode";
        color color = 0xFFFFFFu32, Kind::Color, "Colour";
        color2 color = 0xB18CFFu32, Kind::Color, "Gradient end";
        gradient pick = "across", Kind::Choice(&["across", "down", "by_word"]), "Gradient runs";
        rainbow_speed num = 30.0f32, slider(0.0, 180.0, 1.0, " deg/s"), "Rainbow speed";
        use_highlight flag = false, Kind::Toggle, "Colour the sung word";
        highlight color = 0xFFE27Au32, Kind::Color, "Sung word colour";
        opacity num = 95.0f32, slider(10.0, 100.0, 1.0, "%"), "Opacity";
        outline num = 0.0f32, slider(0.0, 100.0, 1.0, "%"), "Outline";
        outline_color color = 0x101018u32, Kind::Color, "Outline colour";
        glow num = 45.0f32, slider(0.0, 100.0, 1.0, "%"), "Glow size";
        glow_strength num = 60.0f32, slider(0.0, 200.0, 1.0, "%"), "Glow strength";
        glow_match flag = true, Kind::Toggle, "Glow matches the text";
        glow_color color = 0xB18CFFu32, Kind::Color, "Glow colour";
        shadow num = 45.0f32, slider(0.0, 100.0, 1.0, "%"), "Shadow";
        shadow_offset num = 8.0f32, slider(0.0, 60.0, 1.0, "%"), "Shadow distance";
        shadow_color color = 0x000000u32, Kind::Color, "Shadow colour";
    [Motion]
        reveal pick = "word", Kind::Choice(&["word", "letter", "line", "karaoke"]), "Reveal";
        enter pick = "rise", Kind::Choice(&["rise", "drop", "pop", "zoom", "slide", "fade", "none"]), "Entrance";
        exit pick = "fade", Kind::Choice(&["fade", "rise", "drop", "shrink", "none"]), "Exit";
        enter_ms num = 320.0f32, slider(0.0, 1500.0, 10.0, " ms"), "Entrance time";
        exit_ms num = 260.0f32, slider(0.0, 1500.0, 10.0, " ms"), "Exit time";
        bounce num = 8.0f32, slider(0.0, 80.0, 1.0, "%"), "Sung word pop";
        dim num = 40.0f32, slider(0.0, 100.0, 1.0, "%"), "Karaoke unsung opacity";
        wave num = 0.0f32, slider(0.0, 100.0, 1.0, "%"), "Wave";
        wave_speed num = 1.5f32, slider(0.1, 6.0, 0.1, "x"), "Wave speed";
        context_before num = 0.0f32, slider(0.0, 3.0, 1.0, ""), "Lines shown before";
        context_after num = 0.0f32, slider(0.0, 3.0, 1.0, ""), "Lines shown after";
        context_opacity num = 35.0f32, slider(0.0, 100.0, 1.0, "%"), "Other lines' opacity";
        gap_hide num = 5.0f32, slider(1.0, 30.0, 0.5, " s"), "Hide in gaps over";
        title_card flag = true, Kind::Toggle, "Song title card";
        title_seconds num = 5.0f32, slider(1.0, 20.0, 0.5, " s"), "Title card time";
    [Music]
        player text = "auto", Kind::Player, "Music player";
        offset num = 0.0f32, slider(-5000.0, 5000.0, 50.0, " ms"), "Timing offset";
        online flag = true, Kind::Toggle, "Look up lyrics online";
        when_paused pick = "hide", Kind::Choice(&["hide", "freeze"]), "When paused";
        unsynced pick = "estimate", Kind::Choice(&["estimate", "hide"]), "Lyrics with no timing";
        hud pick = "off", Kind::Choice(&["off", "top_left", "top_right", "bottom_left", "bottom_right"]), "Line on the HUD";
}

/// The swatches every colour row offers.
pub const SWATCHES: [u32; 8] =
    [0xFFFFFF, 0xB18CFF, 0xFF7AC6, 0x6FD3FF, 0x7DF0B0, 0xFFE27A, 0xFF8A65, 0x101018];

/// Looks made of several settings at once.
pub const STYLES: &[(&str, &[(&str, &str)])] = &[
    ("Dreamy (default)", &[("color_mode", "sky"), ("glow", "45"), ("glow_strength", "60"), ("glow_match", "true"), ("outline", "0"), ("shadow", "45"), ("reveal", "word"), ("enter", "rise"), ("case", "original"), ("letter_spacing", "0"), ("wave", "0"), ("weight", "0")]),
    ("Neon", &[("color_mode", "solid"), ("color", "#FFFFFF"), ("glow", "90"), ("glow_strength", "170"), ("glow_match", "false"), ("glow_color", "#FF2FB3"), ("outline", "0"), ("shadow", "0"), ("reveal", "word"), ("enter", "pop"), ("weight", "12")]),
    ("Aurora", &[("color_mode", "gradient"), ("color", "#7DF0B0"), ("color2", "#B18CFF"), ("gradient", "across"), ("glow", "70"), ("glow_strength", "100"), ("glow_match", "true"), ("shadow", "0"), ("wave", "25"), ("reveal", "letter"), ("enter", "fade")]),
    ("Sunset", &[("color_mode", "gradient"), ("color", "#FFD27A"), ("color2", "#FF5E8A"), ("gradient", "down"), ("glow", "55"), ("glow_strength", "80"), ("glow_match", "true"), ("shadow", "35"), ("reveal", "word"), ("enter", "rise")]),
    ("Karaoke", &[("color_mode", "solid"), ("color", "#FFFFFF"), ("reveal", "karaoke"), ("use_highlight", "true"), ("highlight", "#FFE27A"), ("dim", "40"), ("enter", "fade"), ("context_after", "1"), ("glow", "25"), ("glow_strength", "50"), ("outline", "20"), ("outline_color", "#101018")]),
    ("Cinema", &[("color_mode", "solid"), ("color", "#F4F1EA"), ("case", "upper"), ("letter_spacing", "28"), ("weight", "-8"), ("glow", "0"), ("outline", "0"), ("shadow", "60"), ("reveal", "line"), ("enter", "fade"), ("enter_ms", "700"), ("exit_ms", "700"), ("wave", "0")]),
    ("Rainbow wave", &[("color_mode", "rainbow"), ("rainbow_speed", "70"), ("wave", "60"), ("wave_speed", "2"), ("glow", "60"), ("glow_strength", "90"), ("glow_match", "true"), ("reveal", "letter"), ("enter", "pop")]),
    ("Terminal", &[("color_mode", "solid"), ("color", "#7DF0B0"), ("glow", "65"), ("glow_strength", "120"), ("glow_match", "true"), ("reveal", "letter"), ("enter", "none"), ("exit", "none"), ("shadow", "0"), ("case", "lower")]),
    ("Plain", &[("color_mode", "solid"), ("color", "#FFFFFF"), ("glow", "0"), ("outline", "30"), ("outline_color", "#000000"), ("shadow", "0"), ("reveal", "line"), ("enter", "none"), ("exit", "none"), ("bounce", "0"), ("sway", "0"), ("curve", "0"), ("wave", "0")]),
];

pub fn hex(rgb: u32) -> String {
    format!("#{:06X}", rgb & 0xFF_FFFF)
}

pub fn parse_hex(text: &str) -> Option<u32> {
    let digits = text.trim().trim_start_matches('#');
    if digits.len() != 6 {
        return None;
    }
    u32::from_str_radix(digits, 16).ok()
}

impl Spec {
    /// The value as the file and the styles write it.
    pub fn encode(&self, c: &Config) -> String {
        match (self.get)(c) {
            Value::Num(v) => trim_number(v),
            Value::Flag(v) => v.to_string(),
            Value::Pick(v) => v.to_owned(),
            Value::Color(v) => hex(v),
            Value::Text(v) => v,
        }
    }

    /// Sets from file text. False leaves the value alone.
    pub fn decode(&self, c: &mut Config, text: &str) -> bool {
        let text = text.trim();
        let value = match self.kind {
            Kind::Slider { min, max, .. } => match text.parse::<f32>() {
                Ok(v) if v.is_finite() => Value::Num(v.clamp(min, max)),
                _ => return false,
            },
            Kind::Toggle => match text {
                "true" | "on" => Value::Flag(true),
                "false" | "off" => Value::Flag(false),
                _ => return false,
            },
            Kind::Choice(options) => match options.iter().find(|o| **o == text) {
                Some(o) => Value::Pick(o),
                None => return false,
            },
            Kind::Color => match parse_hex(text) {
                Some(v) => Value::Color(v),
                None => return false,
            },
            Kind::Font | Kind::Player => Value::Text(text.to_owned()),
        };
        (self.set)(c, value);
        true
    }

    /// The value as the screen shows it.
    pub fn display(&self, c: &Config) -> String {
        match ((self.get)(c), self.kind) {
            (Value::Num(v), Kind::Slider { unit, .. }) => format!("{}{unit}", trim_number(v)),
            (Value::Flag(v), _) => if v { "On" } else { "Off" }.to_owned(),
            (Value::Pick(v), _) => v.replace('_', " "),
            (Value::Color(v), _) => hex(v),
            (Value::Text(v), Kind::Font) if v.is_empty() => "Automatic".to_owned(),
            (Value::Text(v), Kind::Font) => {
                Path::new(&v).file_stem().map_or(v.clone(), |s| s.to_string_lossy().into_owned())
            }
            (Value::Text(v), _) => v,
            (Value::Num(v), _) => v.to_string(),
        }
    }
}

fn trim_number(v: f32) -> String {
    let text = format!("{v:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

pub fn spec(key: &str) -> Option<&'static Spec> {
    SPECS.iter().find(|s| s.key == key)
}

impl Config {
    pub fn encode(&self) -> String {
        let mut out = String::from("# Sky Lyrics. Press K in game for the settings screen.\n");
        for s in SPECS {
            out.push_str(&format!("{} = {}\n", s.key, s.encode(self)));
        }
        out
    }

    /// Reads the file's text, keeping defaults for anything missing and
    /// naming the lines it could not use.
    pub fn decode(text: &str) -> (Config, Vec<String>) {
        let mut config = Config::default();
        let mut bad = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                bad.push(line.to_owned());
                continue;
            };
            match spec(key.trim()) {
                Some(s) if s.decode(&mut config, value) => {}
                _ => bad.push(key.trim().to_owned()),
            }
        }
        (config, bad)
    }

    pub fn apply_style(&mut self, index: usize) {
        let Some((_, values)) = STYLES.get(index) else {
            return;
        };
        for (key, value) in *values {
            if let Some(s) = spec(key) {
                s.decode(self, value);
            }
        }
    }

    pub fn load(path: &Path) -> (Config, Vec<String>) {
        match std::fs::read_to_string(path) {
            Ok(text) => Config::decode(&text),
            Err(_) => (Config::default(), Vec::new()),
        }
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, self.encode())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_round_trips() {
        let mut c = Config::default();
        c.size = 12.5;
        c.anchor = "compass";
        c.color = 0x123456;
        c.font = "/some/Font Name.ttf".into();
        c.shown = false;
        let (back, bad) = Config::decode(&c.encode());
        assert!(bad.is_empty(), "{bad:?}");
        assert_eq!(back, c);
    }

    #[test]
    fn bad_lines_are_named_and_values_are_clamped() {
        let (c, bad) = Config::decode("size = 9000\nanchor = nowhere\nmystery = 1\ncolor = pink\n");
        assert_eq!(c.size, 30.0);
        assert_eq!(c.anchor, "follow");
        assert_eq!(bad, ["anchor", "mystery", "color"]);
    }

    #[test]
    fn every_style_names_real_settings_with_valid_values() {
        for (name, values) in STYLES {
            for (key, value) in *values {
                let s = spec(key).unwrap_or_else(|| panic!("{name}: no setting {key}"));
                assert!(s.decode(&mut Config::default(), value), "{name}: {key} = {value}");
            }
        }
    }

    #[test]
    fn a_font_shows_by_name_not_path() {
        let c = Config { font: "/usr/share/fonts/TTF/Some-Font.ttf".into(), ..Config::default() };
        assert_eq!(spec("font").unwrap().display(&c), "Some-Font");
        assert_eq!(spec("font").unwrap().display(&Config::default()), "Automatic");
    }

    #[test]
    fn keys_are_unique() {
        let mut keys: Vec<_> = SPECS.iter().map(|s| s.key).collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), SPECS.len());
    }
}
