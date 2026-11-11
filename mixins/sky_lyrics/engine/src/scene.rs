//! Which words are up at this moment of the song and where each letter is on
//! the sky: everything is placed on a sphere round the camera, so the text
//! has perspective but never parallax.

use crate::atlas::{Atlas, Glyph};
use crate::config::Config;
use crate::lrc::{Line, Lyrics, Word};
use std::borrow::Cow;

const RADIUS: f32 = 100.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quad {
    /// Camera-relative: top left, top right, bottom right, bottom left.
    pub corners: [[f32; 3]; 4],
    pub uv: [f32; 4],
    pub fill: [u8; 4],
    pub line: [u8; 4],
    pub glow: [u8; 4],
    /// Weight, outline, glow reach, glow strength.
    pub params: [f32; 4],
}

#[derive(Clone, Copy, Debug)]
pub struct View {
    pub yaw: f32,
    pub pitch: f32,
    pub dt: f32,
    pub time: f32,
    pub sky: [f32; 3],
}

/// Where a following text has drifted to.
#[derive(Clone, Copy, Debug, Default)]
pub struct Motion {
    yaw: Option<f32>,
    pitch: f32,
    chasing: bool,
}

/// One block of text at this moment.
#[derive(Clone, Debug)]
pub struct Para<'a> {
    words: Cow<'a, [Word]>,
    start: i64,
    /// Above (-) or below (+) the current line; 0 is the current line.
    row: i32,
    scale: f32,
    opacity: f32,
    /// How far through leaving, 0 to 1.
    leaving: f32,
    /// A whole line that needs no reveal.
    settled: bool,
    karaoke: bool,
}

fn line_para(line: &Line, row: i32, opacity: f32, leaving: f32, settled: bool, karaoke: bool) -> Para<'_> {
    Para {
        words: Cow::Borrowed(&line.words),
        start: i64::from(line.start),
        row,
        scale: 1.0,
        opacity,
        leaving,
        settled,
        karaoke,
    }
}

/// When a line leaves: with the next line, unless a long gap follows.
fn stays_until(cfg: &Config, lines: &[Line], i: usize) -> i64 {
    let end = i64::from(lines[i].end) + 400;
    match lines.get(i + 1) {
        Some(next) if i64::from(next.start) - end <= (cfg.gap_hide * 1000.0) as i64 => i64::from(next.start),
        _ => end,
    }
}

/// The blocks up at `at` (song milliseconds, offset already applied).
pub fn timeline<'a>(cfg: &Config, lyrics: &'a Lyrics, at: i64, title: Option<(&str, &str)>) -> Vec<Para<'a>> {
    let mut out = Vec::new();
    let lines = &lyrics.lines;
    let karaoke = cfg.reveal == "karaoke";
    let first_line = lines.first().map_or(i64::MAX, |l| i64::from(l.start));
    if let Some((song, artist)) = title.filter(|_| cfg.title_card) {
        let end = ((cfg.title_seconds * 1000.0) as i64).min(first_line);
        if at < end + cfg.exit_ms as i64 {
            let leaving = ((at - end) as f32 / cfg.exit_ms.max(1.0)).clamp(0.0, 1.0);
            let block = |text: &str, row, scale| Para {
                words: Cow::Owned(
                    text.split_whitespace().map(|w| Word { start: 0, end: 0, text: w.to_owned() }).collect(),
                ),
                start: 0,
                row,
                scale,
                opacity: 1.0,
                leaving,
                settled: false,
                karaoke: false,
            };
            out.push(block(song, 0, 1.2));
            if !artist.is_empty() {
                out.push(block(artist, 1, 0.6));
            }
        }
    }
    let Some(i) = at.try_into().ok().and_then(|a: u32| lyrics.index_at(a)) else {
        return out;
    };
    let until = stays_until(cfg, lines, i);
    let exit = cfg.exit_ms.max(1.0);
    let with_context = cfg.context_before > 0.5 || cfg.context_after > 0.5;
    let gone = cfg.exit == "none" && at >= until;
    if at < until + cfg.exit_ms as i64 && !gone {
        let leaving = ((at - until) as f32 / exit).clamp(0.0, 1.0);
        out.push(line_para(&lines[i], 0, 1.0, leaving, false, karaoke));
        // The context only stands round a line that is still up.
        if leaving == 0.0 {
            let context = cfg.context_opacity / 100.0;
            for n in 1..=cfg.context_before as usize {
                if let Some(line) = i.checked_sub(n).map(|p| &lines[p]) {
                    out.push(line_para(line, -(n as i32), context, 0.0, true, false));
                }
            }
            for n in 1..=cfg.context_after as usize {
                if let Some(line) = lines.get(i + n) {
                    out.push(line_para(line, n as i32, context, 0.0, true, false));
                }
            }
        }
    }
    // The line before fades while this one arrives.
    if i > 0 && !with_context && cfg.exit != "none" {
        let until = stays_until(cfg, lines, i - 1);
        if at < until + cfg.exit_ms as i64 {
            let leaving = ((at - until) as f32 / exit).clamp(0.0, 1.0);
            out.push(line_para(&lines[i - 1], 0, 1.0, leaving, true, false));
        }
    }
    out
}

/// The current line's text, for the HUD.
pub fn current_text(cfg: &Config, lyrics: &Lyrics, at: i64) -> Option<String> {
    let i = at.try_into().ok().and_then(|a: u32| lyrics.index_at(a))?;
    (at < stays_until(cfg, &lyrics.lines, i)).then(|| lyrics.lines[i].text())
}

fn wrap_degrees(a: f32) -> f32 {
    (a + 540.0).rem_euclid(360.0) - 180.0
}

fn anchor(cfg: &Config, view: &View, motion: &mut Motion) -> (f32, f32) {
    match cfg.anchor {
        "compass" => (cfg.compass + cfg.side, -cfg.elevation),
        "locked" => (view.yaw + cfg.side, view.pitch - cfg.elevation),
        _ => {
            let target = view.yaw + cfg.side;
            let pitch_target = if cfg.follow_pitch { view.pitch - cfg.elevation } else { -cfg.elevation };
            let Some(yaw) = motion.yaw else {
                *motion = Motion { yaw: Some(target), pitch: pitch_target, chasing: false };
                return (target, pitch_target);
            };
            let off = wrap_degrees(target - yaw);
            let pitch_off = pitch_target - motion.pitch;
            if off.abs() > cfg.deadzone || pitch_off.abs() > cfg.deadzone {
                motion.chasing = true;
            } else if off.abs() < 1.0 && pitch_off.abs() < 1.0 {
                motion.chasing = false;
            }
            if motion.chasing {
                let k = 1.0 - (-cfg.follow_speed * view.dt).exp();
                motion.yaw = Some(wrap_degrees(yaw + off * k));
                motion.pitch += pitch_off * k;
            }
            (motion.yaw.unwrap_or(target), motion.pitch)
        }
    }
}

type V3 = [f32; 3];

fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn scaled(a: V3, s: f32) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// The plane the text lies on, bent round the viewer.
struct Sheet {
    centre: V3,
    forward: V3,
    right: V3,
    up: V3,
    radius: Option<f32>,
    roll: (f32, f32),
}

impl Sheet {
    fn new(cfg: &Config, yaw: f32, pitch: f32, roll: f32) -> Sheet {
        let (y, p) = (yaw.to_radians(), pitch.to_radians());
        let forward = [-y.sin() * p.cos(), -p.sin(), y.cos() * p.cos()];
        let right = [-y.cos(), 0.0, -y.sin()];
        let up = if cfg.facing == "upright" { [0.0, 1.0, 0.0] } else { cross(right, forward) };
        let lean = cfg.lean.to_radians();
        let up = add(scaled(up, lean.cos()), scaled(forward, lean.sin()));
        let curve = cfg.curve / 100.0;
        let roll = roll.to_radians();
        Sheet {
            centre: scaled(forward, RADIUS),
            forward,
            right,
            up,
            radius: (curve > 0.001).then(|| RADIUS / curve),
            roll: (roll.sin(), roll.cos()),
        }
    }

    /// A point of the text at `(x, y)`, `pivot` being where it bends.
    fn place(&self, pivot: [f32; 2], x: f32, y: f32) -> V3 {
        let (s, c) = self.roll;
        let rotate = |x: f32, y: f32| (x * c - y * s, x * s + y * c);
        let (px, _) = rotate(pivot[0], pivot[1]);
        let (u, v) = rotate(x, y);
        match self.radius {
            None => add(add(self.centre, scaled(self.right, u)), scaled(self.up, v)),
            Some(r) => {
                let a = px / r;
                let (sa, ca) = a.sin_cos();
                let hub = add(self.centre, scaled(self.forward, -r));
                let spot = add(hub, add(scaled(self.forward, r * ca), scaled(self.right, r * sa)));
                let right = add(scaled(self.right, ca), scaled(self.forward, -sa));
                add(add(spot, scaled(right, u - px)), scaled(self.up, v))
            }
        }
    }
}

fn hsv(h: f32, s: f32, v: f32) -> [f32; 3] {
    let h = h.rem_euclid(360.0) / 60.0;
    let c = v * s;
    let x = c * (1.0 - (h.rem_euclid(2.0) - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    [r + m, g + m, b + m]
}

fn to_hsv([r, g, b]: [f32; 3]) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let d = max - r.min(g).min(b);
    let h = if d <= 1e-5 {
        0.0
    } else if max == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    (h, if max <= 1e-5 { 0.0 } else { d / max }, max)
}

/// A bright, readable colour of the sky's own hue.
pub fn sky_tint(sky: [f32; 3]) -> [f32; 3] {
    let (h, s, _) = to_hsv(sky);
    hsv(h, (s * 0.45).clamp(0.12, 0.42), 1.0)
}

fn rgb(c: u32) -> [f32; 3] {
    [((c >> 16) & 255) as f32 / 255.0, ((c >> 8) & 255) as f32 / 255.0, (c & 255) as f32 / 255.0]
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

fn bytes(c: [f32; 3], a: f32) -> [u8; 4] {
    let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    [q(c[0]), q(c[1]), q(c[2]), q(a)]
}

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn overshoot(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0) - 1.0;
    1.0 + t * t * (2.7 * t + 1.7)
}

/// Alpha, offset and scale of a letter arriving or leaving, in text heights.
struct Pose {
    alpha: f32,
    dx: f32,
    dy: f32,
    scale: f32,
}

fn entering(kind: &str, p: f32) -> Pose {
    let eased = smooth(p);
    let miss = 1.0 - eased;
    let base = Pose { alpha: eased, dx: 0.0, dy: 0.0, scale: 1.0 };
    match kind {
        "rise" => Pose { dy: -0.55 * miss, ..base },
        "drop" => Pose { dy: 0.55 * miss, ..base },
        "pop" => Pose { scale: overshoot(p).max(0.0), alpha: smooth(p * 2.0), ..base },
        "zoom" => Pose { scale: 1.8 - 0.8 * eased, ..base },
        "slide" => Pose { dx: -0.9 * miss, ..base },
        "none" => Pose { alpha: 1.0, ..base },
        _ => base,
    }
}

fn leaving(kind: &str, q: f32) -> Pose {
    let eased = smooth(q);
    let base = Pose { alpha: 1.0 - eased, dx: 0.0, dy: 0.0, scale: 1.0 };
    match kind {
        "rise" => Pose { dy: 0.5 * eased, ..base },
        "drop" => Pose { dy: -0.5 * eased, ..base },
        "shrink" => Pose { scale: 1.0 - 0.7 * eased, ..base },
        _ => base,
    }
}

struct Letter {
    glyph: Glyph,
    x: f32,
    word: usize,
    index: usize,
    count: usize,
}

struct Row {
    letters: Vec<Letter>,
    left: f32,
    width: f32,
    y: f32,
}

fn cased(cfg: &Config, word: &str) -> Vec<char> {
    match cfg.case {
        "upper" => word.chars().flat_map(char::to_uppercase).collect(),
        "lower" => word.chars().flat_map(char::to_lowercase).collect(),
        _ => word.chars().collect(),
    }
}

/// Breaks a block into centred rows, `y` 0 at the block's middle, in units of `size`.
fn lay_out(cfg: &Config, atlas: &Atlas, para: &Para<'_>, size: f32) -> (Vec<Row>, f32) {
    let spacing = cfg.letter_spacing / 100.0;
    let space = atlas.glyphs.get(&' ').map_or(0.3, |g| g.advance) * cfg.word_spacing / 100.0;
    let limit = cfg.wrap * 0.5;
    let mut rows: Vec<(Vec<Letter>, f32)> = vec![(Vec::new(), 0.0)];
    for (word_index, word) in para.words.iter().enumerate() {
        let chars = cased(cfg, &word.text);
        let found: Vec<Glyph> = chars.iter().filter_map(|c| atlas.glyphs.get(c).copied()).collect();
        let mut width = 0.0;
        let mut xs = Vec::with_capacity(found.len());
        for (i, glyph) in found.iter().enumerate() {
            if i > 0 {
                width += atlas.kern(&found[i - 1], glyph);
            }
            xs.push(width);
            width += glyph.advance + spacing;
        }
        let row = rows.last_mut().expect("there is always a row");
        if !row.0.is_empty() && row.1 + space + width > limit {
            rows.push((Vec::new(), 0.0));
        }
        let row = rows.last_mut().expect("there is always a row");
        if !row.0.is_empty() {
            row.1 += space;
        }
        let pen = row.1;
        let count = found.len();
        for (i, (glyph, x)) in found.into_iter().zip(xs).enumerate() {
            row.0.push(Letter { glyph, x: pen + x, word: word_index, index: i, count });
        }
        row.1 += width;
    }
    let step = cfg.line_height / 100.0;
    let height = step * rows.len() as f32;
    let mid = (atlas.ascent + atlas.descent) / 2.0;
    let laid = rows
        .into_iter()
        .enumerate()
        .map(|(n, (letters, width))| {
            let left = match cfg.align {
                "left" => -limit / 2.0,
                "right" => limit / 2.0 - width,
                _ => -width / 2.0,
            };
            let y = (height / 2.0 - step * (n as f32 + 0.5) - mid) * size;
            Row { letters, left: left * size, width: width * size, y }
        })
        .collect();
    (laid, height * size)
}

/// Every letter up at `at`, with the shadows drawn first.
pub fn build(cfg: &Config, atlas: &Atlas, view: &View, motion: &mut Motion, paras: &[Para<'_>], at: i64) -> Vec<Quad> {
    let (yaw, pitch) = anchor(cfg, view, motion);
    let t = view.time * cfg.sway_speed;
    let sway = cfg.sway / 100.0;
    let sheet = Sheet::new(
        cfg,
        yaw + (t * 0.7).sin() * 1.6 * sway,
        pitch + (t * 0.53).sin() * 1.1 * sway,
        cfg.tilt + (t * 0.41).sin() * 2.2 * sway,
    );

    let base = |para: &Para<'_>| cfg.size * para.scale;
    // Blocks stack outwards from the current line; those at row 0 share it.
    let mut laid: Vec<(usize, Vec<Row>, f32)> = Vec::with_capacity(paras.len());
    for (i, para) in paras.iter().enumerate() {
        let (rows, height) = lay_out(cfg, atlas, para, base(para));
        laid.push((i, rows, height));
    }
    let half_current = laid
        .iter()
        .find(|(i, _, _)| paras[*i].row == 0)
        .map_or(cfg.size * 0.6, |(_, _, h)| h / 2.0);
    let gap = cfg.size * 0.2;
    let shift_of = |row: i32| -> f32 {
        let mut y = if row == 0 { 0.0 } else { half_current + gap };
        let sign = row.signum();
        for n in 1..row.abs() {
            let height = laid.iter().find(|(i, _, _)| paras[*i].row == sign * n).map_or(0.0, |(_, _, h)| *h);
            y += height + gap;
        }
        if row != 0 {
            let own = laid.iter().find(|(i, _, _)| paras[*i].row == row).map_or(0.0, |(_, _, h)| *h);
            y += own / 2.0;
        }
        -(sign as f32) * y
    };

    let opacity = cfg.opacity / 100.0;
    let weight = cfg.weight / 100.0 * 0.2;
    let outline = cfg.outline / 100.0 * 0.35;
    let reach = cfg.glow / 100.0 * 0.42;
    let strength = cfg.glow_strength / 100.0;
    let params = [weight, outline, reach, strength];
    let slant = cfg.italic.to_radians().tan();
    let (cfill, cend, chigh) = (rgb(cfg.color), rgb(cfg.color2), rgb(cfg.highlight));
    let (cline, cglow, cshadow) = (rgb(cfg.outline_color), rgb(cfg.glow_color), rgb(cfg.shadow_color));
    let tint = sky_tint(view.sky);
    let mut shadows = Vec::new();
    let mut quads = Vec::new();

    for (index, rows, height) in &laid {
        let para = &paras[*index];
        let size = base(para);
        let lift = shift_of(para.row);
        let out = (para.leaving > 0.0).then(|| leaving(cfg.exit, para.leaving));
        let word_total = para.words.len().max(1) as f32;
        for (row_index, row) in rows.iter().enumerate() {
            // Each word's extent, for scaling about its middle.
            let mut extents = vec![(f32::MAX, f32::MIN); para.words.len()];
            for l in &row.letters {
                let x = row.left + l.x * size;
                let e = &mut extents[l.word];
                e.0 = e.0.min(x + l.glyph.plane[0] * size);
                e.1 = e.1.max(x + l.glyph.plane[2] * size);
            }
            for letter in &row.letters {
                let word = &para.words[letter.word];
                let span = (i64::from(word.end) - i64::from(word.start)).max(1);
                let share = letter.index as f32 / letter.count.max(1) as f32;
                let sung_at = i64::from(word.start) + (span as f32 * share) as i64;
                let starts = if para.settled || para.karaoke || cfg.reveal == "line" {
                    para.start
                } else if cfg.reveal == "letter" {
                    sung_at
                } else {
                    i64::from(word.start)
                };
                let mut pose = if para.settled {
                    Pose { alpha: 1.0, dx: 0.0, dy: 0.0, scale: 1.0 }
                } else {
                    if at < starts {
                        continue;
                    }
                    let p = if cfg.enter_ms <= 0.0 { 1.0 } else { (at - starts) as f32 / cfg.enter_ms };
                    entering(cfg.enter, p.min(1.0))
                };
                if let Some(out) = &out {
                    pose.alpha *= out.alpha;
                    pose.dx += out.dx;
                    pose.dy += out.dy;
                    pose.scale *= out.scale;
                }
                let singing = !para.settled && at >= i64::from(word.start) && at < i64::from(word.end);
                let mut scale = pose.scale;
                if singing {
                    let through = (at - i64::from(word.start)) as f32 / span as f32;
                    scale *= 1.0 + cfg.bounce / 100.0 * (through * std::f32::consts::PI).sin();
                }
                let (extent_left, extent_right) = extents[letter.word];
                let cx = (extent_left + extent_right) / 2.0;
                let cy = row.y + (atlas.ascent + atlas.descent) / 2.0 * size;
                let pen = row.left + letter.x * size;
                let wave = cfg.wave / 100.0
                    * 0.35
                    * size
                    * (view.time * cfg.wave_speed * 3.8 + pen / cfg.size.max(0.1) * 0.7).sin();
                let [left, top, right, bottom] = letter.glyph.plane;
                let spot = |px: f32, py: f32| {
                    let x = pen + px * size;
                    let y = row.y - py * size;
                    let skew = (y - row.y) * slant;
                    [
                        cx + (x - cx) * scale + skew + pose.dx * size,
                        cy + (y - cy) * scale + lift + wave + pose.dy * size,
                    ]
                };
                if right <= left {
                    continue;
                }
                let middle = spot((left + right) / 2.0, (top + bottom) / 2.0);
                let square = [spot(left, top), spot(right, top), spot(right, bottom), spot(left, bottom)];
                let corners = square.map(|p| sheet.place(middle, p[0], p[1]));

                let colour = match cfg.color_mode {
                    "solid" => cfill,
                    "gradient" => {
                        let across = if row.width > 0.0 { (pen - row.left) / row.width } else { 0.0 };
                        let down = if rows.len() > 1 { row_index as f32 / (rows.len() - 1) as f32 } else { 0.0 };
                        let by_word = letter.word as f32 / (word_total - 1.0).max(1.0);
                        mix(cfill, cend, match cfg.gradient {
                            "down" => down + (top + bottom) / 2.0 * 0.2,
                            "by_word" => by_word,
                            _ => across,
                        })
                    }
                    "rainbow" => hsv(view.time * cfg.rainbow_speed + pen / cfg.size.max(0.1) * 30.0, 0.65, 1.0),
                    _ => tint,
                };
                let sung = para.karaoke && at >= sung_at;
                let highlight = cfg.use_highlight && if para.karaoke { sung } else { singing };
                let colour = if highlight { chigh } else { colour };
                let unsung = if para.karaoke && !sung { cfg.dim / 100.0 } else { 1.0 };
                let alpha = (pose.alpha * opacity * para.opacity * unsung).clamp(0.0, 1.0);
                if alpha <= 0.003 {
                    continue;
                }
                let glow = if cfg.glow_match { colour } else { cglow };
                let _ = height;
                quads.push(Quad {
                    corners,
                    uv: letter.glyph.uv,
                    fill: bytes(colour, alpha),
                    line: bytes(cline, if outline > 0.0 { alpha } else { 0.0 }),
                    glow: bytes(glow, if reach > 0.0 { alpha } else { 0.0 }),
                    params,
                });
                if cfg.shadow > 0.0 {
                    let drop = size * cfg.shadow_offset / 100.0;
                    let moved = square.map(|p| sheet.place(middle, p[0] + drop * 0.6, p[1] - drop));
                    shadows.push(Quad {
                        corners: moved,
                        uv: letter.glyph.uv,
                        fill: bytes(cshadow, alpha * cfg.shadow / 100.0),
                        line: [0; 4],
                        glow: [0; 4],
                        params: [weight, 0.0, 0.0, 0.0],
                    });
                }
            }
        }
    }
    shadows.extend(quads);
    shadows
}

/// A short verse of the mod's own, looping, for the settings screen's preview.
pub fn sample() -> Lyrics {
    let line = |start: u32, end: u32, words: &[&str]| {
        let span = (end - start) / words.len() as u32;
        Line {
            start,
            end,
            words: words
                .iter()
                .enumerate()
                .map(|(i, w)| Word { start: start + span * i as u32, end: start + span * (i as u32 + 1), text: (*w).to_owned() })
                .collect(),
        }
    };
    Lyrics {
        lines: vec![
            line(0, 3200, &["Your", "words", "float", "here"]),
            line(4000, 7600, &["above", "the", "open", "world"]),
        ],
        synced: true,
    }
}

/// Where the sample loops.
pub const SAMPLE_LOOP_MS: i64 = 8400;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atlas;
    use crate::lrc::parse_lrc;
    use std::path::Path;

    fn font_atlas(text: &str) -> Option<Atlas> {
        let fonts = atlas::scan(Path::new("/nonexistent"), None);
        let font = atlas::resolve("", &fonts).first().and_then(|p| atlas::load_font(p))?;
        Some(atlas::build(vec![font], &atlas::letters(text)))
    }

    fn view(yaw: f32, pitch: f32) -> View {
        View { yaw, pitch, dt: 0.016, time: 1.0, sky: [0.4, 0.6, 1.0] }
    }

    fn song() -> Lyrics {
        parse_lrc("[00:01.00]one two three\n[00:04.00]four five\n[00:30.00]far later\n")
    }

    #[test]
    fn only_the_right_line_is_up() {
        let cfg = Config { title_card: false, ..Config::default() };
        let l = song();
        assert!(timeline(&cfg, &l, 500, None).is_empty());
        let up = timeline(&cfg, &l, 2000, None);
        assert_eq!(up.len(), 1);
        assert_eq!(up[0].words[0].text, "one");
        // The next line arrives and the old one is leaving.
        let change = timeline(&cfg, &l, 4100, None);
        assert_eq!(change.len(), 2);
        assert_eq!(change[0].words[0].text, "four");
        assert_eq!(change[1].words[0].text, "one");
        assert!(change[1].leaving > 0.0);
    }

    #[test]
    fn a_long_gap_hides_the_line() {
        let cfg = Config { title_card: false, ..Config::default() };
        assert!(timeline(&cfg, &song(), 20_000, None).is_empty());
    }

    #[test]
    fn the_context_stands_above_and_below() {
        let cfg = Config { title_card: false, context_before: 1.0, context_after: 1.0, ..Config::default() };
        let l = song();
        let up = timeline(&cfg, &l, 4500, None);
        let rows: Vec<i32> = up.iter().map(|p| p.row).collect();
        assert_eq!(rows, [0, -1, 1]);
    }

    #[test]
    fn the_title_card_waits_for_the_first_line() {
        let cfg = Config::default();
        let l = song();
        let up = timeline(&cfg, &l, 100, Some(("A Song", "A Band")));
        assert_eq!(up.len(), 2);
        assert_eq!(up[0].words[1].text, "Song");
        assert!(timeline(&cfg, &song(), 2000, Some(("A Song", "A Band"))).iter().all(|p| p.words[0].text == "one"));
    }

    #[test]
    fn words_appear_as_they_are_sung() {
        let Some(atlas) = font_atlas("one two three") else {
            return;
        };
        let cfg = Config { title_card: false, shadow: 0.0, enter: "none", ..Config::default() };
        let l = song();
        let mut m = Motion::default();
        let count = |at| {
            let mut m2 = m;
            build(&cfg, &atlas, &view(0.0, 0.0), &mut m2, &timeline(&cfg, &l, at, None), at).len()
        };
        assert_eq!(count(1000), 3, "only 'one'");
        assert!(count(3500) > count(1000), "more words by now");
        let karaoke = Config { reveal: "karaoke", ..cfg.clone() };
        let up = timeline(&karaoke, &l, 1000, None);
        assert_eq!(build(&karaoke, &atlas, &view(0.0, 0.0), &mut m, &up, 1000).len(), 11, "karaoke shows all");
    }

    #[test]
    fn text_sits_where_you_look_above_the_horizon() {
        let Some(atlas) = font_atlas("x") else {
            return;
        };
        let cfg = Config { title_card: false, reveal: "line", enter: "none", shadow: 0.0, sway: 0.0, curve: 0.0, ..Config::default() };
        let l = parse_lrc("[00:00.00]x\n");
        let up = timeline(&cfg, &l, 1000, None);
        // Yaw 0 looks along +Z.
        let quads = build(&cfg, &atlas, &view(0.0, 0.0), &mut Motion::default(), &up, 1000);
        assert_eq!(quads.len(), 1);
        let c = quads[0].corners[0];
        assert!(c[2] > 80.0, "ahead: {c:?}");
        assert!(c[1] > 20.0, "above the horizon: {c:?}");
        // The letter's top is higher than its bottom, and its right side is to the right (-X at yaw 0).
        assert!(quads[0].corners[0][1] > quads[0].corners[3][1]);
        assert!(quads[0].corners[1][0] < quads[0].corners[0][0]);
    }

    #[test]
    fn following_waits_for_the_dead_zone_then_catches_up() {
        let cfg = Config { deadzone: 10.0, follow_speed: 8.0, ..Config::default() };
        let mut m = Motion::default();
        let v = |yaw| View { dt: 0.1, ..view(yaw, 0.0) };
        assert_eq!(anchor(&cfg, &v(0.0), &mut m).0, 0.0);
        assert_eq!(anchor(&cfg, &v(5.0), &mut m).0, 0.0, "inside the dead zone");
        let mut yaw = anchor(&cfg, &v(50.0), &mut m).0;
        assert!(yaw > 0.0 && yaw < 50.0);
        for _ in 0..60 {
            yaw = anchor(&cfg, &v(50.0), &mut m).0;
        }
        assert!((yaw - 50.0).abs() < 1.5, "{yaw}");
    }

    #[test]
    fn following_turns_the_short_way_past_the_back() {
        let cfg = Config { deadzone: 0.0, follow_speed: 8.0, ..Config::default() };
        let mut m = Motion::default();
        anchor(&cfg, &view(170.0, 0.0), &mut m);
        let yaw = anchor(&cfg, &View { dt: 0.05, ..view(-170.0, 0.0) }, &mut m).0;
        assert!(yaw > 170.0 || yaw < -170.0, "{yaw}");
    }

    #[test]
    fn a_long_line_wraps() {
        let Some(atlas) = font_atlas("aaaa") else {
            return;
        };
        let cfg = Config { wrap: 8.0, ..Config::default() };
        let words: Vec<Word> = (0..6).map(|i| Word { start: i, end: i + 1, text: "aaaa".into() }).collect();
        let para = Para { words: words.into(), start: 0, row: 0, scale: 1.0, opacity: 1.0, leaving: 0.0, settled: true, karaoke: false };
        let (rows, _) = lay_out(&cfg, &atlas, &para, 8.0);
        assert!(rows.len() > 1);
        assert!(rows[0].y > rows[1].y, "rows go down");
    }

    #[test]
    fn the_sky_tint_keeps_the_hue_and_is_bright() {
        let sunset = sky_tint([0.9, 0.5, 0.2]);
        assert!(sunset[0] > sunset[2]);
        assert!(sunset.iter().cloned().fold(0.0, f32::max) > 0.95);
    }
}
