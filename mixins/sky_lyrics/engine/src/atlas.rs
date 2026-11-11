//! The song's letters as one signed distance field texture: 0.5 is the edge
//! of the letter, so weight, outline and glow are decided when drawing and
//! stay sharp at any size.

pub use ab_glyph::{Font, FontArc};
use ab_glyph::{FontVec, Glyph as AbGlyph, GlyphId, PxScale, ScaleFont, point};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

/// Pixels per unit of text height in the texture.
const HEIGHT: u32 = 48;
/// How far the field reaches past a letter's edge, in texture pixels.
const SPREAD: u32 = 14;
/// Letters are drawn this much larger, and the field averaged back down.
const SUPER: u32 = 3;
const CELL_GAP: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glyph {
    pub uv: [f32; 4],
    /// Left, top, right, bottom from the pen, in text heights; y grows down.
    pub plane: [f32; 4],
    pub advance: f32,
    font: u16,
    id: GlyphId,
}

pub struct Atlas {
    pub size: u32,
    pub pixels: Vec<u8>,
    pub glyphs: HashMap<char, Glyph>,
    pub ascent: f32,
    pub descent: f32,
    fonts: Vec<FontArc>,
}

impl Atlas {
    pub fn empty() -> Atlas {
        Atlas {
            size: 1,
            pixels: vec![0],
            glyphs: HashMap::new(),
            ascent: 0.8,
            descent: -0.2,
            fonts: Vec::new(),
        }
    }

    /// The extra advance between two letters of one font, in text heights.
    pub fn kern(&self, a: &Glyph, b: &Glyph) -> f32 {
        if a.font != b.font {
            return 0.0;
        }
        match self.fonts.get(usize::from(a.font)) {
            Some(font) => font.as_scaled(PxScale::from(1000.0)).kern(a.id, b.id) / 1000.0,
            None => 0.0,
        }
    }
}

pub fn load_font(path: &Path) -> Option<FontArc> {
    let data = std::fs::read(path).ok()?;
    FontVec::try_from_vec_and_index(data, 0).ok().map(FontArc::new)
}

#[derive(Clone, Debug, PartialEq)]
pub struct FontFile {
    pub name: String,
    pub path: PathBuf,
}

fn collect(dir: &Path, out: &mut Vec<FontFile>, depth: u32) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if depth < 5 {
                collect(&path, out, depth + 1);
            }
            continue;
        }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        if matches!(ext.as_str(), "ttf" | "otf" | "ttc") {
            let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            out.push(FontFile { name, path });
        }
    }
}

/// Fonts you added first, then the system's, by name.
pub fn scan(own: &Path, home: Option<&Path>) -> Vec<FontFile> {
    let mut mine = Vec::new();
    collect(own, &mut mine, 0);
    mine.sort_by_key(|f| f.name.to_lowercase());
    let mut system = Vec::new();
    if let Some(home) = home {
        collect(&home.join(".local/share/fonts"), &mut system, 0);
        collect(&home.join(".fonts"), &mut system, 0);
    }
    collect(Path::new("/usr/share/fonts"), &mut system, 0);
    collect(Path::new("/usr/local/share/fonts"), &mut system, 0);
    system.sort_by_key(|f| f.name.to_lowercase());
    system.dedup_by(|a, b| a.path == b.path);
    mine.extend(system);
    mine
}

/// Whether the font can write ordinary text: 571 of 797 fonts on a typical
/// Linux desktop are for other scripts, icons or emoji, and choosing one would
/// only show the fallback font's letters.
pub fn usable(path: &Path) -> bool {
    let Some(font) = load_font(path) else {
        return false;
    };
    let full = ('a'..='z').chain('A'..='Z').all(|c| font.glyph_id(c).0 != 0);
    full && font.outline_glyph(font.glyph_id('a').with_scale(64.0)).is_some()
}

const WANTED: [&str; 10] = [
    "Poppins-SemiBold",
    "Poppins-Medium",
    "Montserrat-SemiBold",
    "Inter-SemiBold",
    "NotoSans-SemiBold",
    "NotoSans-Medium",
    "NotoSans-Bold",
    "DejaVuSans-Bold",
    "LiberationSans-Bold",
    "NotoSans-Regular",
];
const FALLBACKS: [&str; 7] = [
    "NotoSans-Regular",
    "NotoSansCJK-Regular",
    "NotoSansCJK-VF",
    "NotoSansArabic-Regular",
    "NotoSansHebrew-Regular",
    "NotoSansDevanagari-Regular",
    "DejaVuSans",
];

/// The font files to use, the chosen one first and those for missing letters after.
pub fn resolve(chosen: &str, fonts: &[FontFile]) -> Vec<PathBuf> {
    let named = |name: &str| fonts.iter().find(|f| f.name == name).map(|f| f.path.clone());
    let primary = Some(chosen)
        .filter(|c| !c.is_empty())
        .and_then(|c| {
            let path = PathBuf::from(c);
            if path.is_file() { Some(path) } else { named(c) }
        })
        .or_else(|| WANTED.iter().find_map(|w| named(w)))
        .or_else(|| fonts.first().map(|f| f.path.clone()));
    let mut out: Vec<PathBuf> = primary.into_iter().collect();
    for name in FALLBACKS {
        if let Some(path) = named(name) {
            if !out.contains(&path) {
                out.push(path);
            }
        }
    }
    out
}

/// Felzenszwalb's squared distance transform of one row, in place.
fn distance_1d(f: &mut [f32], v: &mut [usize], z: &mut [f32], out: &mut [f32]) {
    let n = f.len();
    let mut k = 0usize;
    v[0] = 0;
    z[0] = f32::NEG_INFINITY;
    z[1] = f32::INFINITY;
    for q in 1..n {
        let mut s;
        loop {
            let p = v[k];
            s = ((f[q] + (q * q) as f32) - (f[p] + (p * p) as f32)) / (2.0 * (q as f32 - p as f32));
            if s > z[k] || k == 0 {
                break;
            }
            k -= 1;
        }
        k += 1;
        v[k] = q;
        z[k] = s;
        z[k + 1] = f32::INFINITY;
    }
    k = 0;
    for (q, slot) in out.iter_mut().enumerate().take(n) {
        while z[k + 1] < q as f32 {
            k += 1;
        }
        let d = q as f32 - v[k] as f32;
        *slot = d * d + f[v[k]];
    }
    f.copy_from_slice(&out[..n]);
}

/// Squared distance to the nearest set cell.
fn distance_2d(set: &[bool], w: usize, h: usize) -> Vec<f32> {
    const FAR: f32 = 1e12;
    let mut grid: Vec<f32> = set.iter().map(|&s| if s { 0.0 } else { FAR }).collect();
    let side = w.max(h);
    let (mut v, mut z, mut out) = (vec![0usize; side], vec![0f32; side + 1], vec![0f32; side]);
    let mut column = vec![0f32; h];
    for x in 0..w {
        for y in 0..h {
            column[y] = grid[y * w + x];
        }
        distance_1d(&mut column, &mut v, &mut z, &mut out);
        for y in 0..h {
            grid[y * w + x] = column[y];
        }
    }
    for row in grid.chunks_exact_mut(w) {
        distance_1d(row, &mut v, &mut z, &mut out);
    }
    grid
}

struct Bitmap {
    c: char,
    w: u32,
    h: u32,
    field: Vec<u8>,
    plane: [f32; 4],
    advance: f32,
    font: u16,
    id: GlyphId,
}

/// One letter's field, or just its advance for a space.
fn raster(font: &FontArc, font_index: u16, c: char) -> Bitmap {
    let id = font.glyph_id(c);
    let scale = PxScale::from((HEIGHT * SUPER) as f32);
    let scaled = font.as_scaled(scale);
    let advance = scaled.h_advance(id) / (HEIGHT * SUPER) as f32;
    let blank = Bitmap { c, w: 0, h: 0, field: Vec::new(), plane: [0.0; 4], advance, font: font_index, id };
    let glyph: AbGlyph = id.with_scale_and_position(scale, point(0.0, 0.0));
    let Some(outline) = font.outline_glyph(glyph) else {
        return blank;
    };
    let bounds = outline.px_bounds();
    let pad = SPREAD * SUPER;
    let gw = bounds.width().ceil() as usize;
    let gh = bounds.height().ceil() as usize;
    // Whole texture pixels, so the field averages down without a seam.
    let w_big = ((gw as u32 + 2 * pad).div_ceil(SUPER) * SUPER) as usize;
    let h_big = ((gh as u32 + 2 * pad).div_ceil(SUPER) * SUPER) as usize;
    let mut inside = vec![false; w_big * h_big];
    outline.draw(|x, y, coverage| {
        if coverage >= 0.5 {
            let (px, py) = (x as usize + pad as usize, y as usize + pad as usize);
            if px < w_big && py < h_big {
                inside[py * w_big + px] = true;
            }
        }
    });
    let to_inside = distance_2d(&inside, w_big, h_big);
    let outside: Vec<bool> = inside.iter().map(|s| !s).collect();
    let to_outside = distance_2d(&outside, w_big, h_big);
    let (w, h) = ((w_big as u32) / SUPER, (h_big as u32) / SUPER);
    let mut field = vec![0u8; (w * h) as usize];
    let reach = (SPREAD * SUPER) as f32;
    let step = SUPER as usize;
    for y in 0..h as usize {
        for x in 0..w as usize {
            let mut sum = 0.0;
            for by in 0..step {
                for bx in 0..step {
                    let i = (y * step + by) * w_big + x * step + bx;
                    // Half a cell each way: the edge lies between set and unset.
                    let d = if inside[i] { to_outside[i].sqrt() - 0.5 } else { -(to_inside[i].sqrt() - 0.5) };
                    sum += d;
                }
            }
            let average = sum / (step * step) as f32;
            let value = (0.5 + average / (2.0 * reach)).clamp(0.0, 1.0);
            field[y * w as usize + x] = (value * 255.0).round() as u8;
        }
    }
    let unit = (HEIGHT * SUPER) as f32;
    let left = (bounds.min.x - pad as f32) / unit;
    let top = (bounds.min.y - pad as f32) / unit;
    let plane = [left, top, left + w_big as f32 / unit, top + h_big as f32 / unit];
    Bitmap { c, w, h, field, plane, advance, font: font_index, id }
}

fn printable(c: char) -> bool {
    !c.is_control()
}

/// The letters of `text` in every case the settings can ask for, plus ASCII.
pub fn letters(text: &str) -> BTreeSet<char> {
    let mut set: BTreeSet<char> = (' '..='~').collect();
    for c in text.chars().filter(|c| printable(*c)) {
        set.insert(c);
        set.extend(c.to_uppercase().filter(|c| printable(*c)));
        set.extend(c.to_lowercase().filter(|c| printable(*c)));
    }
    set
}

/// Packs into the smallest square that holds everything, tallest first.
fn pack(items: &[Bitmap]) -> Option<(u32, Vec<[u32; 2]>)> {
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(items[i].h));
    'size: for size in [256u32, 512, 1024, 2048, 4096, 8192] {
        let mut at = vec![[0u32; 2]; items.len()];
        let (mut x, mut y, mut row) = (0u32, 0u32, 0u32);
        for &i in &order {
            let (w, h) = (items[i].w + CELL_GAP, items[i].h + CELL_GAP);
            if items[i].w == 0 {
                continue;
            }
            if x + w > size {
                x = 0;
                y += row;
                row = 0;
            }
            if w > size || y + h > size {
                continue 'size;
            }
            at[i] = [x, y];
            x += w;
            row = row.max(h);
        }
        return Some((size, at));
    }
    None
}

/// Builds the atlas of `chars` from the first of `fonts` that has each one.
pub fn build(fonts: Vec<FontArc>, chars: &BTreeSet<char>) -> Atlas {
    let mut items = Vec::new();
    for &c in chars {
        if let Some((i, font)) = fonts.iter().enumerate().find(|(_, f)| f.glyph_id(c).0 != 0) {
            items.push(raster(font, i as u16, c));
        }
    }
    let Some((size, at)) = pack(&items) else {
        return Atlas::empty();
    };
    let mut pixels = vec![0u8; (size * size) as usize];
    let mut glyphs = HashMap::with_capacity(items.len());
    let s = size as f32;
    for (item, [x, y]) in items.iter().zip(&at) {
        for row in 0..item.h {
            let from = (row * item.w) as usize;
            let to = ((y + row) * size + x) as usize;
            pixels[to..to + item.w as usize].copy_from_slice(&item.field[from..from + item.w as usize]);
        }
        let uv = [*x as f32 / s, *y as f32 / s, (x + item.w) as f32 / s, (y + item.h) as f32 / s];
        glyphs.insert(item.c, Glyph { uv, plane: item.plane, advance: item.advance, font: item.font, id: item.id });
    }
    let (ascent, descent) = fonts.first().map_or((0.8, -0.2), |f| {
        let scaled = f.as_scaled(PxScale::from(1.0));
        (scaled.ascent(), scaled.descent())
    });
    Atlas { size, pixels, glyphs, ascent, descent, fonts }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn system_font() -> Option<FontArc> {
        let fonts = scan(Path::new("/nonexistent"), None);
        resolve("", &fonts).first().and_then(|p| load_font(p))
    }

    #[test]
    fn the_distance_transform_measures_steps() {
        let mut set = vec![false; 25];
        set[12] = true;
        let d = distance_2d(&set, 5, 5);
        assert_eq!(d[12], 0.0);
        assert_eq!(d[13], 1.0);
        assert_eq!(d[0], 8.0);
    }

    #[test]
    fn a_path_that_is_no_font_is_not_usable() {
        assert!(!usable(Path::new("/nonexistent/font.ttf")));
    }

    #[test]
    fn letters_cover_every_case() {
        let set = letters("Straße Ünï");
        assert!(set.contains(&'ü') && set.contains(&'Ü') && set.contains(&'S'));
        assert!(set.contains(&'~'));
    }

    #[test]
    fn a_built_atlas_has_a_field_with_letter_inside_and_air_outside() {
        let Some(font) = system_font() else {
            eprintln!("no system font, skipped");
            return;
        };
        let atlas = build(vec![font], &letters("Ab"));
        let g = atlas.glyphs[&'A'];
        assert!(g.advance > 0.1);
        assert!(g.plane[0] < g.plane[2] && g.plane[1] < g.plane[3]);
        assert_eq!(atlas.glyphs[&' '].uv, [0.0; 4]);
        let size = atlas.size as f32;
        let (x0, y0, x1, y1) = ((g.uv[0] * size) as usize, (g.uv[1] * size) as usize, (g.uv[2] * size) as usize, (g.uv[3] * size) as usize);
        let at = |x: usize, y: usize| atlas.pixels[y * atlas.size as usize + x];
        assert_eq!(at(x0, y0), 0, "the corner of a cell is far from the letter");
        let peak = (y0..y1).flat_map(|y| (x0..x1).map(move |x| (x, y))).map(|(x, y)| at(x, y)).max().unwrap();
        assert!(peak > 128, "the letter's middle is inside: {peak}");
    }

    #[test]
    fn a_chosen_font_comes_first_and_automatic_finds_one() {
        let fonts = vec![
            FontFile { name: "Other".into(), path: "/f/o.ttf".into() },
            FontFile { name: "DejaVuSans-Bold".into(), path: "/f/d.ttf".into() },
            FontFile { name: "DejaVuSans".into(), path: "/f/r.ttf".into() },
        ];
        assert_eq!(resolve("Other", &fonts)[0], PathBuf::from("/f/o.ttf"));
        let auto = resolve("", &fonts);
        assert_eq!(auto[0], PathBuf::from("/f/d.ttf"));
        assert!(auto.contains(&PathBuf::from("/f/r.ttf")));
    }
}
