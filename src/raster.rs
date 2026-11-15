//! The drawing's own rasterizer. Rune draws a line as one capsule per segment, and a
//! hand-drawn line has hundreds: their soft edges add up into hard, stepped ones, and anything
//! see-through doubles where they overlap. Here a line is drawn as real curves with tiny-skia,
//! into tiles of a fixed size that Rune shows as pictures. While a line grows, only the tiles
//! at its end are drawn again.

use std::collections::BTreeMap;

use rune::{Color, Pixels};
use tiny_skia::{LineCap, LineJoin, Paint, PathBuilder, Pixmap, Transform};

use crate::ink::{self, Cubic, Point, Stroke};

/// A tile's side in device pixels. Tiles keep their size, so Rune keeps their place in its
/// atlas.
pub const TILE: u32 = 256;

/// The soft shadow under a line: its offset in points, then extra width and alpha per pass,
/// widest first.
const SHADOW_OFFSET: Point = [1.0, 1.6];
const SHADOW: [(f32, f32); 3] = [(3.2, 0.045), (1.6, 0.06), (0.4, 0.08)];

/// `[x0, y0, x1, y1]` in points.
type Rect = [f32; 4];

/// One stroke's pictures, by tile.
#[derive(Default)]
pub struct StrokeTiles {
    pub tiles: BTreeMap<(i32, i32), Pixels>,
    revision: Option<u64>,
    scale: f32,
    /// How many curves the line had when last drawn.
    curves: usize,
    /// Around the part that could still change then.
    tail: Option<Rect>,
}

impl StrokeTiles {
    /// Draws the tiles that changed since the last call, at `scale` device pixels per point.
    pub fn update(&mut self, stroke: &Stroke, scale: f32) {
        if self.revision == Some(stroke.revision) && self.scale == scale {
            return;
        }
        let curve = stroke.curve();
        let head = stroke.head();
        let end = stroke.end();
        let dirty = if self.revision.is_none() || self.scale != scale {
            self.tiles.clear();
            bounds(&curve, &head, end)
        } else {
            let from = self.curves.saturating_sub(ink::UNSTABLE).min(curve.len());
            union(self.tail, bounds(&curve[from..], &head, end))
        };
        self.tail = bounds(&curve[curve.len().saturating_sub(ink::UNSTABLE)..], &head, end);
        self.curves = curve.len();
        self.revision = Some(stroke.revision);
        self.scale = scale;

        let Some(dirty) = dirty else { return };
        let reach = reach(stroke.width);
        let side = TILE as f32 / scale;
        let tile = |v: f32| (v / side).floor() as i32;
        for ty in tile(dirty[1] - reach)..=tile(dirty[3] + reach) {
            for tx in tile(dirty[0] - reach)..=tile(dirty[2] + reach) {
                match draw_tile(stroke, &curve, &head, scale, tx, ty) {
                    Some(rgba) => self.tiles.entry((tx, ty)).or_default().set(TILE, TILE, rgba),
                    None => {
                        self.tiles.remove(&(tx, ty));
                    }
                }
            }
        }
    }
}

/// How far a line's paint reaches past its middle, shadow included.
fn reach(width: f32) -> f32 {
    width * 0.5 + SHADOW[0].0 * 0.5 + SHADOW_OFFSET[1] + 2.0
}

/// One tile of a stroke, as straight-alpha RGBA, or `None` when nothing of it shows there.
fn draw_tile(stroke: &Stroke, curve: &[Cubic], head: &[[Point; 2]], scale: f32, tx: i32, ty: i32) -> Option<Vec<u8>> {
    let side = TILE as f32 / scale;
    let reach = reach(stroke.width);
    let area = [tx as f32 * side - reach, ty as f32 * side - reach, (tx + 1) as f32 * side + reach, (ty + 1) as f32 * side + reach];

    // The runs of curves that come near the tile. A run is cut only where it is out of reach,
    // so its caps never show here.
    let mut pb = PathBuilder::new();
    let mut open = false;
    for c in curve {
        if overlaps(cubic_bounds(c), area) {
            if !open {
                pb.move_to(c[0][0], c[0][1]);
                open = true;
            }
            pb.cubic_to(c[1][0], c[1][1], c[2][0], c[2][1], c[3][0], c[3][1]);
        } else {
            open = false;
        }
    }
    let end = stroke.end();
    if curve.is_empty() && overlaps([end[0], end[1], end[0], end[1]], area) {
        // A single point: a dot.
        pb.move_to(end[0], end[1]);
        pb.line_to(end[0] + 0.01, end[1]);
    }
    for [a, b] in head {
        if overlaps([a[0].min(b[0]), a[1].min(b[1]), a[0].max(b[0]), a[1].max(b[1])], area) {
            pb.move_to(a[0], a[1]);
            pb.line_to(b[0], b[1]);
        }
    }
    let path = pb.finish()?;

    let mut pixmap = Pixmap::new(TILE, TILE)?;
    let at = Transform::from_row(scale, 0.0, 0.0, scale, -(tx as f32) * TILE as f32, -(ty as f32) * TILE as f32);
    let mut paint = Paint { anti_alias: true, ..Paint::default() };
    let mut pen = tiny_skia::Stroke { line_cap: LineCap::Round, line_join: LineJoin::Round, ..tiny_skia::Stroke::default() };
    let shadow_at = at.pre_translate(SHADOW_OFFSET[0], SHADOW_OFFSET[1]);
    for (extra, alpha) in SHADOW {
        paint.set_color(tiny_skia::Color::from_rgba(0.0, 0.0, 0.0, alpha)?);
        pen.width = stroke.width + extra;
        pixmap.stroke_path(&path, &paint, &pen, shadow_at, None);
    }
    paint.set_color(skia_color(stroke.color, 1.0)?);
    pen.width = stroke.width;
    pixmap.stroke_path(&path, &paint, &pen, at, None);

    if pixmap.data().as_chunks::<4>().0.iter().all(|px| px[3] == 0) {
        return None;
    }
    Some(straight(pixmap.take()))
}

/// A curve lies inside the box of its four points.
fn cubic_bounds(c: &Cubic) -> Rect {
    let xs = c.map(|p| p[0]);
    let ys = c.map(|p| p[1]);
    [xs.iter().copied().fold(f32::MAX, f32::min), ys.iter().copied().fold(f32::MAX, f32::min), xs.iter().copied().fold(f32::MIN, f32::max), ys.iter().copied().fold(f32::MIN, f32::max)]
}

fn bounds(curve: &[Cubic], head: &[[Point; 2]], end: Point) -> Option<Rect> {
    let mut r = [end[0], end[1], end[0], end[1]];
    for b in curve.iter().map(cubic_bounds).chain(head.iter().map(|[a, b]| [a[0].min(b[0]), a[1].min(b[1]), a[0].max(b[0]), a[1].max(b[1])])) {
        r = [r[0].min(b[0]), r[1].min(b[1]), r[2].max(b[2]), r[3].max(b[3])];
    }
    Some(r)
}

fn union(a: Option<Rect>, b: Option<Rect>) -> Option<Rect> {
    match (a, b) {
        (Some(a), Some(b)) => Some([a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])]),
        (a, b) => a.or(b),
    }
}

fn overlaps(a: Rect, b: Rect) -> bool {
    a[0] <= b[2] && a[2] >= b[0] && a[1] <= b[3] && a[3] >= b[1]
}

fn skia_color(c: Color, alpha: f32) -> Option<tiny_skia::Color> {
    tiny_skia::Color::from_rgba(c.r.clamp(0.0, 1.0), c.g.clamp(0.0, 1.0), c.b.clamp(0.0, 1.0), alpha.clamp(0.0, 1.0))
}

/// tiny-skia keeps premultiplied pixels; Rune takes straight ones.
fn straight(mut rgba: Vec<u8>) -> Vec<u8> {
    for px in rgba.as_chunks_mut::<4>().0 {
        let a = px[3] as u32;
        if a != 0 && a != 255 {
            for c in &mut px[..3] {
                *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    rgba
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(points: &[Point]) -> Stroke {
        let mut s = Stroke::new(points[0], Color::WHITE, 6.0);
        for p in &points[1..] {
            s.add(*p);
        }
        s
    }

    #[test]
    fn a_line_shows_in_the_tiles_it_crosses() {
        let s = line(&[[10.0, 10.0], [200.0, 10.0], [400.0, 10.0]]);
        let mut tiles = StrokeTiles::default();
        tiles.update(&s, 1.0);
        let keys: Vec<_> = tiles.tiles.keys().copied().collect();
        assert_eq!(keys, vec![(0, 0), (1, 0)]);
    }

    #[test]
    fn growing_draws_only_the_end_again() {
        let points: Vec<Point> = (0..200).map(|i| [10.0 + i as f32 * 3.0, 100.0]).collect();
        let mut s = line(&points[..150]);
        let mut tiles = StrokeTiles::default();
        tiles.update(&s, 1.0);
        let first = tiles.tiles[&(0, 0)].clone();
        let generation = format!("{first:?}");
        for p in &points[150..] {
            s.add(*p);
        }
        tiles.update(&s, 1.0);
        assert_eq!(format!("{:?}", tiles.tiles[&(0, 0)]), generation, "the start's tile stays as it was");
        assert!(tiles.tiles.contains_key(&(2, 0)), "the new end got its tile");
    }
}
