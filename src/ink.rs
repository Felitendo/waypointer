//! Lines drawn with the pointer, with an arrowhead while Ctrl is held, fading after a while
//! unless drawings are kept.
//!
//! The pointer comes in whole pixels, so a slow line is a staircase. A stroke walks its points
//! every [`STEP`] points, evens that out over a few points around each (a Gaussian), and draws
//! smooth curves through every [`EVERY`]th one. Both ends stay put, so the line always ends
//! under the fingertip. Only the last few curves change while it grows: see [`UNSTABLE`].

use std::sync::atomic::{AtomicU64, Ordering};

pub type Point = [f32; 2];
/// One curve of a line: start, two control points, end.
pub type Cubic = [Point; 4];

/// A drag shorter than this is a tap, not a line.
pub const MIN_DRAG: f32 = 4.0;
/// The line is walked in steps this long, in points.
const STEP: f32 = 1.5;
/// How far the evening out reaches, in steps: its sigma, and where it stops.
const SIGMA: f32 = 3.0;
const RADIUS: usize = 7;
/// Every this many steps, the curve gets a point.
const EVERY: usize = 3;
/// How many curves at a line's end can still change when a point comes in.
pub const UNSTABLE: usize = (RADIUS + 1).div_ceil(EVERY) + 4;

#[derive(Clone, Debug)]
pub struct Stroke {
    /// Names its pictures.
    pub id: u64,
    /// The points as they came in. The last one is where the line ends.
    raw: Vec<Point>,
    pub color: rune::Color,
    pub width: f32,
    pub arrow: bool,
    /// When it was finished, on the app clock. `None` while it is drawn.
    pub finished: Option<f64>,
    /// Goes up with every change, so its pictures know to draw again.
    pub revision: u64,
}

impl Stroke {
    pub fn new(start: Point, color: rune::Color, width: f32) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self { id: NEXT.fetch_add(1, Ordering::Relaxed), raw: vec![start], color, width, arrow: false, finished: None, revision: 0 }
    }

    pub fn add(&mut self, p: Point) {
        if distance(p, self.end()) < 0.5 {
            return;
        }
        self.raw.push(p);
        self.revision += 1;
    }

    pub fn set_arrow(&mut self, on: bool) {
        if self.arrow != on {
            self.arrow = on;
            self.revision += 1;
        }
    }

    /// Points every `STEP` along the line, and its end.
    fn walked(&self) -> Vec<Point> {
        let mut out = vec![self.raw[0]];
        let mut left = STEP;
        for pair in self.raw.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let len = distance(a, b);
            let mut at = left;
            while at <= len {
                let t = at / len;
                out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
                at += STEP;
            }
            left = at - len;
        }
        let end = self.end();
        if out.last().is_some_and(|last| distance(*last, end) > 0.01) {
            out.push(end);
        }
        out
    }

    /// The points the curve goes through: the walked line evened out, every `EVERY`th point
    /// and the end. Near the ends the evening out reaches less far, so the ends stay put.
    pub fn smoothed(&self) -> Vec<Point> {
        let p = self.walked();
        let n = p.len();
        let weights: Vec<f32> = (0..=RADIUS).map(|j| (-((j * j) as f32) / (2.0 * SIGMA * SIGMA)).exp()).collect();
        let even = |i: usize| {
            let r = RADIUS.min(i).min(n - 1 - i);
            let (mut x, mut y, mut total) = (0.0, 0.0, 0.0);
            for k in i - r..=i + r {
                let w = weights[k.abs_diff(i)];
                (x, y, total) = (x + p[k][0] * w, y + p[k][1] * w, total + w);
            }
            [x / total, y / total]
        };
        let mut out: Vec<Point> = (0..n).step_by(EVERY).map(even).collect();
        if !(n - 1).is_multiple_of(EVERY) {
            out.push(p[n - 1]);
        }
        out
    }

    /// The line as curves through the smoothed points (Catmull-Rom). Empty for a single point.
    pub fn curve(&self) -> Vec<Cubic> {
        curve_through(&self.smoothed())
    }

    /// Where the line ends.
    pub fn end(&self) -> Point {
        self.raw[self.raw.len() - 1]
    }

    /// The two strokes of the arrowhead, from the end back, when the line has one.
    pub fn head(&self) -> Vec<[Point; 2]> {
        if !self.arrow {
            return vec![];
        }
        head(&self.smoothed(), (self.width * 3.2).max(14.0))
    }

    /// How visible it is at `now`: 1 until `fade` seconds after it was finished, then down to 0
    /// over 0.6 s. Kept drawings (`fade` 0) stay.
    pub fn opacity(&self, now: f64, fade: f64) -> f32 {
        let Some(done) = self.finished else { return 1.0 };
        if fade <= 0.0 {
            return 1.0;
        }
        let t = ((now - done - fade.max(0.5)) / 0.6).clamp(0.0, 1.0) as f32;
        1.0 - t * t
    }

    pub fn gone(&self, now: f64, fade: f64) -> bool {
        fade > 0.0 && self.finished.is_some_and(|done| now > done + fade.max(0.5) + 0.6)
    }
}

pub fn distance(a: Point, b: Point) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// Catmull-Rom curves through `p`, as cubic Béziers.
pub fn curve_through(p: &[Point]) -> Vec<Cubic> {
    let n = p.len();
    (0..n.saturating_sub(1))
        .map(|i| {
            let (a, b) = (p[i], p[i + 1]);
            let before = if i > 0 { p[i - 1] } else { a };
            let after = if i + 2 < n { p[i + 2] } else { b };
            [
                a,
                [a[0] + (b[0] - before[0]) / 6.0, a[1] + (b[1] - before[1]) / 6.0],
                [b[0] - (after[0] - a[0]) / 6.0, b[1] - (after[1] - a[1]) / 6.0],
                b,
            ]
        })
        .collect()
}

/// An arrowhead aimed along the last stretch, so a wobble at the very end doesn't twist it.
pub fn head(p: &[Point], size: f32) -> Vec<[Point; 2]> {
    let Some(&end) = p.last() else { return vec![] };
    let back = p.iter().rev().find(|q| distance(end, **q) >= size * 1.2).copied().unwrap_or(p[0]);
    let length = distance(end, back);
    if length <= 1.0 {
        return vec![];
    }
    let len = size.min(length * 0.8);
    let angle = (end[1] - back[1]).atan2(end[0] - back[0]);
    let spread = std::f32::consts::PI / 7.0;
    [-1.0f32, 1.0]
        .iter()
        .map(|side| {
            let a = angle + std::f32::consts::PI + side * spread;
            [end, [end[0] + a.cos() * len, end[1] + a.sin() * len]]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stroke(points: &[Point]) -> Stroke {
        let mut s = Stroke::new(points[0], rune::Color::WHITE, 4.0);
        for p in &points[1..] {
            s.add(*p);
        }
        s
    }

    #[test]
    fn the_line_keeps_its_ends() {
        let s = stroke(&[[0.0, 0.0], [10.0, 0.0], [20.0, 10.0], [21.0, 10.5]]);
        let curve = s.curve();
        assert_eq!(curve.first().unwrap()[0], [0.0, 0.0]);
        assert_eq!(curve.last().unwrap()[3], [21.0, 10.5], "the line ends under the fingertip");
    }

    #[test]
    fn stairs_come_out_smooth() {
        // A slow diagonal in whole pixels: right, down, right, down.
        let stairs: Vec<Point> = (0..40).map(|i| [((i + 1) / 2) as f32, (i / 2) as f32]).collect();
        let s = stroke(&stairs);
        let line = s.smoothed();
        // The steps' middle runs half a pixel right of the diagonal.
        for p in &line[3..line.len() - 3] {
            assert!((p[0] - p[1] - 0.5).abs() < 0.15, "{p:?} is off the line");
        }
    }

    #[test]
    fn only_the_end_changes_while_drawing() {
        let points: Vec<Point> = (0..400).map(|i| [i as f32 * 0.7, ((i as f32 * 0.05).sin() * 20.0).round()]).collect();
        let full = stroke(&points).curve();
        for cut in [120, 121, 122, 200, 333] {
            let before = stroke(&points[..cut]).curve();
            let stable = before.len().saturating_sub(UNSTABLE);
            assert_eq!(before[..stable], full[..stable], "cut at {cut}");
        }
    }

    #[test]
    fn the_head_points_back_along_the_line() {
        let points: Vec<Point> = (0..20).map(|i| [i as f32 * 5.0, 0.0]).collect();
        let head = head(&points, 14.0);
        assert_eq!(head.len(), 2);
        for side in head {
            assert!(side[1][0] < side[0][0], "both strokes go back left");
        }
    }

    #[test]
    fn a_finished_stroke_fades_and_goes() {
        let mut s = Stroke::new([0.0, 0.0], rune::Color::WHITE, 4.0);
        s.finished = Some(10.0);
        assert_eq!(s.opacity(11.0, 3.0), 1.0);
        assert!(s.opacity(13.3, 3.0) < 1.0);
        assert!(s.gone(13.7, 3.0));
        assert!(!s.gone(100.0, 0.0), "kept drawings stay");
    }
}
