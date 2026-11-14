//! Turns a picture of a hand into a pointer: checks that it looks like a hand pointing up, then
//! puts it on a stick. The same checks and the same stick as TinyHand for macOS.

use image::{Rgba, RgbaImage, imageops};

use crate::i18n::{tr, trf};

pub struct Made {
    /// 1024 × 1536, the hand on its stick.
    pub image: RgbaImage,
    /// An ink colour taken from the hand.
    pub ink: u32,
}

#[derive(Clone, Copy)]
struct Box {
    min_x: u32,
    min_y: u32,
    max_x: u32,
    max_y: u32,
}

impl Box {
    fn w(&self) -> u32 {
        self.max_x - self.min_x + 1
    }
    fn h(&self) -> u32 {
        self.max_y - self.min_y + 1
    }
}

fn alpha(p: &RgbaImage, x: u32, y: u32) -> u8 {
    p.get_pixel(x, y)[3]
}

fn bounds(p: &RgbaImage) -> Option<Box> {
    let mut b = Box { min_x: u32::MAX, min_y: u32::MAX, max_x: 0, max_y: 0 };
    let mut any = false;
    for (x, y, px) in p.enumerate_pixels() {
        if px[3] > 128 {
            any = true;
            b.min_x = b.min_x.min(x);
            b.max_x = b.max_x.max(x);
            b.min_y = b.min_y.min(y);
            b.max_y = b.max_y.max(y);
        }
    }
    any.then_some(b)
}

/// The width and centre of the bottom few rows: the wrist.
fn bottom_span(p: &RgbaImage, b: Box) -> (u32, f32) {
    let from = b.min_y.max(b.max_y.saturating_sub((b.h() * 4 / 100).max(2)));
    let (mut lo, mut hi) = (u32::MAX, None::<u32>);
    for y in from..=b.max_y {
        for x in b.min_x..=b.max_x {
            if alpha(p, x, y) > 128 {
                lo = lo.min(x);
                hi = Some(hi.map_or(x, |h| h.max(x)));
            }
        }
    }
    match hi {
        Some(hi) => (hi - lo + 1, (lo + hi) as f32 / 2.0),
        None => (0, (b.min_x + b.max_x) as f32 / 2.0),
    }
}

fn check(p: &RgbaImage) -> Result<Box, String> {
    let (w, h) = p.dimensions();
    let mut edge = 0u32;
    let mut solid = 0u32;
    for x in 0..w {
        edge += 2;
        solid += u32::from(alpha(p, x, 0) > 32) + u32::from(alpha(p, x, h - 1) > 32);
    }
    for y in 0..h {
        edge += 2;
        solid += u32::from(alpha(p, 0, y) > 32) + u32::from(alpha(p, w - 1, y) > 32);
    }
    if solid as f32 / edge as f32 > 0.02 {
        return Err(tr("The background isn't transparent, or the hand touches the edge of the picture.").into());
    }

    // One thing in the picture, not several.
    let step = (w.min(h) / 256).max(1);
    let (gw, gh) = ((w / step) as usize, (h / step) as usize);
    let grid: Vec<bool> = (0..gw * gh).map(|i| alpha(p, (i % gw) as u32 * step, (i / gw) as u32 * step) > 128).collect();
    let mut seen = vec![false; gw * gh];
    let mut sizes = Vec::new();
    for start in 0..grid.len() {
        if !grid[start] || seen[start] {
            continue;
        }
        let mut stack = vec![start];
        seen[start] = true;
        let mut n = 0;
        while let Some(i) = stack.pop() {
            n += 1;
            let (x, y) = (i % gw, i / gw);
            let mut visit = |nx: usize, ny: usize| {
                let j = ny * gw + nx;
                if grid[j] && !seen[j] {
                    seen[j] = true;
                    stack.push(j);
                }
            };
            if x + 1 < gw { visit(x + 1, y); }
            if x > 0 { visit(x - 1, y); }
            if y + 1 < gh { visit(x, y + 1); }
            if y > 0 { visit(x, y - 1); }
        }
        sizes.push(n);
    }
    let total: usize = sizes.iter().sum();
    let biggest = sizes.iter().copied().max().ok_or(tr("The picture is empty."))?;
    if (biggest as f32) / (total as f32) < 0.97 {
        return Err(tr("The picture has more than one thing in it. Use a single hand.").into());
    }

    let b = bounds(p).ok_or(tr("The picture is empty."))?;
    let aspect = b.h() as f32 / b.w() as f32;
    if !(1.05..=2.8).contains(&aspect) {
        return Err(tr("The hand should stand upright, taller than it is wide.").into());
    }
    if (b.h() as f32) / (h as f32) < 0.5 {
        return Err(tr("The hand is too small in the picture. Crop it closer.").into());
    }
    let band = (b.h() * 8 / 100).max(2);
    let (mut lo, mut hi) = (w, 0);
    for y in b.min_y..b.min_y + band {
        for x in b.min_x..=b.max_x {
            if alpha(p, x, y) > 128 {
                lo = lo.min(x);
                hi = hi.max(x);
            }
        }
    }
    if (hi.saturating_sub(lo) + 1) as f32 / b.w() as f32 > 0.4 {
        return Err(tr("The top of the picture should be a single finger pointing up.").into());
    }
    if (bottom_span(p, b).0 as f32) / (b.w() as f32) < 0.2 {
        return Err(tr("The hand should end in a flat wrist or cuff at the bottom.").into());
    }
    let (mut soft, mut any) = (0u32, 0u32);
    for y in (0..h).step_by(2) {
        for x in (0..w).step_by(2) {
            let a = alpha(p, x, y);
            if a > 20 {
                any += 1;
                soft += u32::from(a < 200);
            }
        }
    }
    if soft as f32 / any.max(1) as f32 > 0.08 {
        return Err(tr("The hand has a soft shadow or glow around it. Ask for no shadow.").into());
    }
    Ok(b)
}

fn compose(src: &RgbaImage, b: Box) -> RgbaImage {
    let (cw, ch) = (1024u32, 1536u32);
    let hand_top = 150.0;
    let hand_height = 560.0;
    let scale = (hand_height / b.h() as f32).min(700.0 / b.w() as f32);
    let draw_w = (src.width() as f32 * scale).round() as u32;
    let draw_h = (src.height() as f32 * scale).round() as u32;
    let wrist_x = (bottom_span(src, b).1 + 0.5) * scale;
    let origin_x = cw as f32 / 2.0 - wrist_x;
    let origin_top = hand_top - b.min_y as f32 * scale;
    let hand_bottom = hand_top + b.h() as f32 * scale;

    let mut out = RgbaImage::new(cw, ch);
    // A dark round stick, a little shiny in the middle.
    let rod_w = (b.w() as f32 * scale * 0.18).clamp(54.0, 76.0);
    let (rod_top, rod_bottom) = (hand_bottom - 30.0, 1462.0f32);
    let left = cw as f32 / 2.0 - rod_w / 2.0;
    let stops = [(0.0, 0x00), (0.22, 0x12), (0.45, 0x3A), (0.75, 0x12), (1.0, 0x00)];
    let shade = |t: f32| -> u8 {
        for pair in stops.windows(2) {
            let ((t0, v0), (t1, v1)) = (pair[0], pair[1]);
            if t <= t1 {
                let f = (t - t0) / (t1 - t0);
                return (v0 as f32 + (v1 as f32 - v0 as f32) * f) as u8;
            }
        }
        0
    };
    let r = rod_w / 2.0;
    for y in rod_top as u32..rod_bottom as u32 {
        for x in left as u32..(left + rod_w).ceil() as u32 {
            let fx = x as f32 + 0.5 - left;
            let fy = y as f32 + 0.5;
            // Round ends.
            let cy = if fy < rod_top + r { rod_top + r } else if fy > rod_bottom - r { rod_bottom - r } else { fy };
            let d = ((fx - r).powi(2) + (fy - cy).powi(2)).sqrt();
            let a = (r - d + 0.5).clamp(0.0, 1.0);
            if a > 0.0 {
                let v = shade(fx / rod_w);
                out.put_pixel(x, y, Rgba([v, v, v + 2, (a * 255.0) as u8]));
            }
        }
    }
    let hand = imageops::resize(src, draw_w.max(1), draw_h.max(1), imageops::FilterType::Lanczos3);
    imageops::overlay(&mut out, &hand, origin_x.round() as i64, origin_top.round() as i64);
    out
}

/// The hand's main colour, a bit darker when it is very light, grey when it has none.
fn ink(p: &RgbaImage) -> u32 {
    use std::collections::HashMap;
    let mut buckets: HashMap<u32, (u32, f32, f32, f32)> = HashMap::new();
    for y in (0..p.height()).step_by(3) {
        for x in (0..p.width()).step_by(3) {
            let px = p.get_pixel(x, y);
            if px[3] <= 200 {
                continue;
            }
            let (r, g, b) = (px[0] as f32 / 255.0, px[1] as f32 / 255.0, px[2] as f32 / 255.0);
            let key = (r * 5.0) as u32 * 36 + (g * 5.0) as u32 * 6 + (b * 5.0) as u32;
            let e = buckets.entry(key).or_default();
            e.0 += 1;
            e.1 += r;
            e.2 += g;
            e.3 += b;
        }
    }
    let (mut best, mut c) = (-1.0f32, (0.53, 0.33, 0.95));
    for (n, r, g, b) in buckets.values() {
        let (r, g, b) = (r / *n as f32, g / *n as f32, b / *n as f32);
        let hi = r.max(g).max(b);
        let lo = r.min(g).min(b);
        let sat = if hi == 0.0 { 0.0 } else { (hi - lo) / hi };
        let lum = 0.3 * r + 0.59 * g + 0.11 * b;
        let score = *n as f32 * (0.25 + sat) * if lum > 0.12 && lum < 0.85 { 1.0 } else { 0.15 };
        if score > best {
            best = score;
            c = (r, g, b);
        }
    }
    let (mut r, mut g, mut b) = c;
    let hi = r.max(g).max(b);
    let lo = r.min(g).min(b);
    if hi - lo < 0.08 {
        (r, g, b) = (0.42, 0.45, 0.5);
    } else if hi > 0.82 {
        let k = 0.82 / hi;
        (r, g, b) = (r * k, g * k, b * k);
    }
    ((r * 255.0) as u32).min(255) << 16 | ((g * 255.0) as u32).min(255) << 8 | ((b * 255.0) as u32).min(255)
}

pub fn make(path: &std::path::Path) -> Result<Made, String> {
    let img = image::open(path).map_err(|_| tr("That file isn't a picture Waypointer can read.").to_string())?;
    if !img.color().has_alpha() {
        return Err(tr("The picture needs a transparent background. Save it as a PNG with transparency.").into());
    }
    let p = img.to_rgba8();
    if p.width() < 64 || p.height() < 64 {
        return Err(tr("The picture is too small. Use one at least 512 pixels tall.").into());
    }
    let b = check(&p)?;
    Ok(Made { image: compose(&p, b), ink: ink(&p) })
}

/// The text to give an image tool such as ChatGPT. Kept in English: they understand it best.
pub fn prompt(idea: &str) -> String {
    let idea = idea.trim();
    let style = if idea.is_empty() { "a soft vinyl toy in a bright colour" } else { idea };
    format!(
        "Make a pointer image for TinyHand, a desktop app that shows a toy hand on screen.\n\n\
         Draw one cartoon right hand in this style: {style}\n\n\
         Rules:\n\
         - We see the BACK of the hand. The palm faces away from us. The knuckles and the smooth back of the hand face the viewer.\n\
         - The index finger points straight up. The other three fingers are folded down, so we only see their knuckles, never their fingertips or fingernails.\n\
         - The raised index finger sits at the far left of the knuckle row, right next to the thumb. Three folded knuckles sit to its right. It must never look like a middle finger.\n\
         - The thumb is tucked against the left side of the hand. Nothing crosses the front of the fist: no thumb, no fingertips, no fingernails, no palm.\n\
         - The fingertip is the highest point of the image. Nothing above or beside it.\n\
         - The hand ends at the bottom in a wrist cuff or a clean flat wrist, facing straight down, centred.\n\
         - Upright, not tilted. Whole hand visible, nothing cropped.\n\
         - Transparent background. No shadow, no floor, no outline glow, no text, no stick, no arm.\n\
         - Soft 3D toy look, bright and readable at small sizes.\n\
         - Portrait image, hand filling most of the height with a little empty space around it."
    )
}

/// A name from the file's: "knit-mustard.png" → "Knit mustard".
pub fn suggested_name(path: &std::path::Path, count: usize) -> String {
    let base = path.file_stem().map(|s| s.to_string_lossy().replace(['_', '-'], " ")).unwrap_or_default();
    let words: Vec<&str> = base.split_whitespace().take(2).collect();
    let name = words.join(" ");
    if name.is_empty() || name.len() > 16 || name.to_lowercase().starts_with("chatgpt image") {
        return trf("My hand {}", &[&(count + 1)]);
    }
    let mut c = name.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hands::Picture;

    #[test]
    fn a_built_in_hand_without_its_stick_is_taken() {
        for id in ["lavender", "coral", "robot", "glove"] {
            let p = Picture::load(id).unwrap();
            let m = p.metrics;
            let h = p.image.height() as f32;
            let (top, bottom) = ((m.hand_top * h) as u32, (m.wrist * h) as u32);
            let hand = imageops::crop_imm(&p.image, 0, top, p.image.width(), bottom - top).to_image();
            let mut canvas = RgbaImage::new(p.image.width(), hand.height() + 160);
            imageops::overlay(&mut canvas, &hand, 0, 80);
            let dir = std::env::temp_dir().join(format!("waypointer-test-{id}.png"));
            canvas.save(&dir).unwrap();
            let made = make(&dir).unwrap_or_else(|e| panic!("{id}: {e}"));
            assert_eq!(made.image.dimensions(), (1024, 1536));
            assert!(crate::hands::Metrics::measure(&made.image).is_some(), "{id}: the result measures");
            let _ = std::fs::remove_file(dir);
        }
    }

    #[test]
    fn a_picture_without_transparency_is_turned_down() {
        let dir = std::env::temp_dir().join("waypointer-test-opaque.png");
        image::RgbImage::from_pixel(600, 900, image::Rgb([255, 255, 255])).save(&dir).unwrap();
        assert!(make(&dir).is_err());
        let _ = std::fs::remove_file(dir);
    }

    #[test]
    fn two_things_are_turned_down() {
        let mut img = RgbaImage::new(600, 900);
        for y in 100..700 {
            for x in (60..260).chain(340..540) {
                img.put_pixel(x, y, Rgba([255, 0, 0, 255]));
            }
        }
        assert!(check(&img).is_err());
    }

    #[test]
    fn names_come_from_the_file() {
        assert_eq!(suggested_name(std::path::Path::new("/x/knit-mustard.png"), 0), "Knit mustard");
        assert_eq!(suggested_name(std::path::Path::new("/x/ChatGPT Image 3.png"), 2), "My hand 3");
    }
}
