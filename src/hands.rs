//! The pointers: the built-in hands, the user's own hands and the laser dot. A hand picture is a
//! hand on a stick on a transparent background; where its parts are is found from its
//! transparency, so any such picture works.

use image::{RgbaImage, imageops};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const LASER: &str = "laser";

/// Where the parts of a hand picture are, as fractions of its height (from the top) and width.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    /// Width over height.
    pub aspect: f32,
    /// A little above the fingertip.
    pub hand_top: f32,
    pub tip: f32,
    /// Where the hand ends and the stick starts.
    pub wrist: f32,
    /// Where the stick ends, or a handle starts.
    pub rod_end: f32,
    pub tip_x: f32,
    pub rod_x: f32,
    pub hand_min_x: f32,
    pub hand_max_x: f32,
}

impl Metrics {
    pub fn measure(img: &RgbaImage) -> Option<Metrics> {
        let (w, h) = (img.width() as usize, img.height() as usize);
        let raw = img.as_raw();
        let opaque = |x: usize, y: usize| raw[(y * w + x) * 4 + 3] > 128;
        let counts: Vec<usize> = (0..h).map(|y| (0..w).filter(|&x| opaque(x, y)).count()).collect();
        let center = |y: usize| {
            let lo = (0..w).find(|&x| opaque(x, y));
            let hi = (0..w).rev().find(|&x| opaque(x, y));
            match (lo, hi) {
                (Some(lo), Some(hi)) => (lo + hi) as f32 / 2.0 / w as f32,
                _ => 0.5,
            }
        };
        let top = counts.iter().position(|&c| c > 0)?;
        let search_end = h.min(top + (0.45 * h as f32) as usize);
        let max_w = *counts[top..search_end].iter().max()?;
        if max_w == 0 {
            return None;
        }
        let fist = (top..h).find(|&y| counts[y] as f32 > 0.6 * max_w as f32)?;
        let wrist = (fist..h).find(|&y| (counts[y] as f32) < 0.35 * max_w as f32)?;
        let rod_w = counts[(h - 1).min(wrist + h / 50)];
        let last_row = counts.iter().rposition(|&c| c > 0).unwrap_or(h - 1);
        let from = (h - 1).min(wrist + h / 33);
        let rod_end = (from..h).find(|&y| counts[y] as f32 > 1.8 * rod_w as f32).unwrap_or(last_row);
        let (mut min_x, mut max_x) = (w, 0);
        for y in top..wrist {
            for x in 0..w {
                if opaque(x, y) {
                    min_x = min_x.min(x);
                    max_x = max_x.max(x);
                }
            }
        }
        let fh = h as f32;
        let m = Metrics {
            aspect: w as f32 / fh,
            hand_top: (top as f32 - 0.004 * fh).max(0.0) / fh,
            tip: top as f32 / fh,
            wrist: wrist as f32 / fh,
            rod_end: rod_end as f32 / fh,
            tip_x: center((h - 1).min(top + h / 200)),
            rod_x: center((wrist + rod_end) / 2),
            hand_min_x: min_x as f32 / w as f32,
            hand_max_x: max_x as f32 / w as f32,
        };
        (m.rod_end > m.wrist && m.wrist > m.hand_top).then_some(m)
    }
}

pub struct BuiltIn {
    pub id: &'static str,
    pub title: &'static str,
    pub ink: u32,
}

const PURPLE: u32 = 0x8754F2;
pub const RED: u32 = 0xFF291F;

/// The same hands, names and ink colours as TinyHand for macOS.
pub const BUILT_IN: &[BuiltIn] = &[
    BuiltIn { id: "lavender", title: n!("Lavender toy"), ink: PURPLE },
    BuiltIn { id: "glove", title: n!("Cursor glove"), ink: PURPLE },
    BuiltIn { id: "coral", title: n!("Coral vinyl"), ink: RED },
    BuiltIn { id: "plush", title: n!("Plush"), ink: 0x2F86F0 },
    BuiltIn { id: "robot", title: n!("Robot"), ink: 0x16A87A },
    BuiltIn { id: "pink", title: n!("Bubblegum"), ink: 0xEE4F98 },
    BuiltIn { id: "blue", title: n!("Sky"), ink: 0x2F86F0 },
    BuiltIn { id: "mint", title: n!("Mint"), ink: 0x16A87A },
    BuiltIn { id: "lemon", title: n!("Lemon"), ink: 0xF2B705 },
    BuiltIn { id: "black", title: n!("Midnight"), ink: 0x1D1D21 },
    BuiltIn { id: "chrome", title: n!("Chrome"), ink: 0x6B7380 },
    BuiltIn { id: "holo", title: n!("Holo"), ink: PURPLE },
    BuiltIn { id: "fuzzy", title: n!("Fuzzy"), ink: PURPLE },
    BuiltIn { id: "wood", title: n!("Wood"), ink: 0x9A5E2C },
    BuiltIn { id: "marble", title: n!("Marble"), ink: 0x6B7380 },
    BuiltIn { id: "terrazzo", title: n!("Terrazzo"), ink: 0xF0561D },
    BuiltIn { id: "glass", title: n!("Glass"), ink: 0x1597E5 },
    BuiltIn { id: "jelly", title: n!("Jelly"), ink: 0xA33FE6 },
    BuiltIn { id: "pastel", title: n!("Pastel"), ink: 0xEE4F98 },
    BuiltIn { id: "pixel", title: n!("Pixel"), ink: 0x1D1D21 },
    BuiltIn { id: "lowpoly", title: n!("Low poly"), ink: PURPLE },
    BuiltIn { id: "knit", title: n!("Knit"), ink: PURPLE },
    BuiltIn { id: "skeleton", title: n!("Skeleton"), ink: 0x9A5E2C },
    BuiltIn { id: "astro", title: n!("Astronaut"), ink: 0x2F86F0 },
    BuiltIn { id: "space", title: n!("Space"), ink: RED },
    BuiltIn { id: "monster", title: n!("Monster"), ink: 0x3E9E2E },
    BuiltIn { id: "water", title: n!("Water"), ink: 0x1597E5 },
    BuiltIn { id: "lava", title: n!("Lava"), ink: 0xF0561D },
    BuiltIn { id: "grass", title: n!("Grass"), ink: 0x3E9E2E },
    BuiltIn { id: "cactus", title: n!("Cactus"), ink: 0x3E9E2E },
    BuiltIn { id: "choco", title: n!("Chocolate"), ink: 0x9A5E2C },
    BuiltIn { id: "cloud", title: n!("Cloud"), ink: 0x2F86F0 },
    BuiltIn { id: "mech", title: n!("Mech"), ink: 0xF0561D },
    BuiltIn { id: "neon", title: n!("Neon"), ink: 0xE0309A },
    BuiltIn { id: "leaf", title: n!("Leaf"), ink: 0x3E9E2E },
    BuiltIn { id: "cookie", title: n!("Gingerbread"), ink: 0x9A5E2C },
];

pub const DEFAULT_PINNED: &[&str] = &["lavender", "glove", "coral", "plush", "robot", LASER];
pub const MAX_PINNED: usize = 8;

/// One of the user's own hands, as listed in hands.json (the same file TinyHand writes).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Custom {
    pub id: String,
    pub name: String,
    pub ink: u32,
}

/// A file that comes with Waypointer: `hands/hand_lavender.png`, `sounds/tap_thock.wav`.
pub fn data_file(relative: &str) -> Option<PathBuf> {
    let name = std::path::Path::new(relative).file_name()?;
    let mut dirs = Vec::new();
    if let Some(dir) = std::env::var_os("WP_DATA_DIR") {
        dirs.push(PathBuf::from(dir));
    }
    // An install for one user (`make install PREFIX=~/.local`), then the system's.
    if let Some(data) = dirs::data_dir() {
        dirs.push(data.join("waypointer"));
    }
    dirs.push(PathBuf::from("/usr/local/share/waypointer"));
    dirs.push(PathBuf::from("/usr/share/waypointer"));
    // A build run from its checkout.
    dirs.push(PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/assets")));
    for dir in dirs {
        for candidate in [dir.join(relative), dir.join(name)] {
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    None
}

pub fn custom_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("WP_HANDS_DIR") {
        return PathBuf::from(dir);
    }
    dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("waypointer/hands")
}

pub fn load_custom() -> Vec<Custom> {
    std::fs::read(custom_dir().join("hands.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Vec<Custom>>(&bytes).ok())
        .unwrap_or_default()
        .into_iter()
        .filter(|c| c.id.starts_with("custom-"))
        .collect()
}

pub fn save_custom(list: &[Custom]) -> std::io::Result<()> {
    let dir = custom_dir();
    std::fs::create_dir_all(&dir)?;
    let tmp = dir.join("hands.json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(list)?)?;
    std::fs::rename(tmp, dir.join("hands.json"))
}

pub fn picture_path(id: &str) -> Option<PathBuf> {
    if id.starts_with("custom-") {
        let path = custom_dir().join(format!("{id}.png"));
        return path.exists().then_some(path);
    }
    data_file(&format!("hands/hand_{id}.png"))
}

/// A hand picture and where its parts are.
pub struct Picture {
    pub image: RgbaImage,
    pub metrics: Metrics,
}

impl Picture {
    pub fn load(id: &str) -> Option<Picture> {
        let image = image::open(picture_path(id)?).ok()?.to_rgba8();
        let metrics = Metrics::measure(&image)?;
        Some(Picture { image, metrics })
    }

    /// The hand or the stick, scaled so the whole picture would be `full_height` pixels tall.
    pub fn part(&self, rod: bool, full_height: u32) -> RgbaImage {
        let h = self.image.height() as f32;
        let m = &self.metrics;
        let (from, to) = if rod { (m.wrist, m.rod_end) } else { (m.hand_top, m.wrist) };
        let (y0, y1) = ((from * h).round() as u32, (to * h).round() as u32);
        let section = imageops::crop_imm(&self.image, 0, y0, self.image.width(), (y1 - y0).max(1)).to_image();
        let scale = (full_height as f32 / h).min(1.0);
        if scale > 0.9 {
            return section;
        }
        let nw = ((section.width() as f32 * scale).round() as u32).max(1);
        let nh = ((section.height() as f32 * scale).round() as u32).max(1);
        imageops::resize(&section, nw, nh, imageops::FilterType::CatmullRom)
    }

    /// A square picture of just the hand, for the toolbar and the settings.
    pub fn thumbnail(&self, side: u32) -> RgbaImage {
        let (w, h) = (self.image.width() as f32, self.image.height() as f32);
        let m = &self.metrics;
        let hand_w = (m.hand_max_x - m.hand_min_x) * w;
        let hand_h = (m.wrist - m.hand_top) * h;
        let size = hand_w.max(hand_h) * 1.06;
        let (cx, cy) = ((m.hand_min_x + m.hand_max_x) / 2.0 * w, (m.hand_top + m.wrist) / 2.0 * h);
        let x0 = (cx - size / 2.0).max(0.0) as u32;
        let y0 = (cy - size / 2.0).max(0.0) as u32;
        let cw = (size as u32).min(self.image.width() - x0);
        let ch = (size as u32).min(self.image.height() - y0);
        let crop = imageops::crop_imm(&self.image, x0, y0, cw, ch).to_image();
        let scale = side as f32 / cw.max(ch) as f32;
        let small = imageops::resize(&crop, ((cw as f32 * scale) as u32).max(1), ((ch as f32 * scale) as u32).max(1), imageops::FilterType::Lanczos3);
        let mut out = RgbaImage::new(side, side);
        imageops::overlay(&mut out, &small, ((side - small.width()) / 2) as i64, ((side - small.height()) / 2) as i64);
        out
    }
}

/// A soft black copy of a picture for its shadow, with `pad` pixels of room around it for the
/// blur. Made small and blurred, then drawn larger: it is soft anyway.
pub fn silhouette(img: &RgbaImage, pad: u32, blur: f32) -> RgbaImage {
    let (w, h) = (img.width() + 2 * pad, img.height() + 2 * pad);
    let mut out = RgbaImage::new(w, h);
    for (x, y, p) in img.enumerate_pixels() {
        out.put_pixel(x + pad, y + pad, image::Rgba([0, 0, 0, p[3]]));
    }
    if blur > 0.5 {
        out = imageops::blur(&out, blur);
    }
    out
}

pub fn rgb(ink: u32) -> rune::Color {
    rune::Color {
        r: ((ink >> 16) & 0xFF) as f32 / 255.0,
        g: ((ink >> 8) & 0xFF) as f32 / 255.0,
        b: (ink & 0xFF) as f32 / 255.0,
        a: 1.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_built_in_hand_measures() {
        for hand in BUILT_IN {
            let p = Picture::load(hand.id).unwrap_or_else(|| panic!("{} missing", hand.id));
            let m = p.metrics;
            assert!(m.tip < m.wrist && m.wrist < m.rod_end && m.rod_end <= 1.0, "{}: {m:?}", hand.id);
            assert!(m.wrist < 0.7, "{}: hand in the top part", hand.id);
            assert!(m.tip_x > 0.2 && m.tip_x < 0.8 && m.rod_x > 0.3 && m.rod_x < 0.7, "{}: {m:?}", hand.id);
            let hand_part = p.part(false, 256);
            assert!((hand_part.height() as f32 - (m.wrist - m.hand_top) * 256.0).abs() <= 2.0);
            assert_eq!(p.thumbnail(64).dimensions(), (64, 64));
        }
    }

    #[test]
    fn every_tap_sound_is_there() {
        for sound in crate::settings::TAP_SOUNDS {
            assert!(data_file(&format!("sounds/tap_{}.wav", sound.0)).is_some(), "{}", sound.0);
        }
    }
}
