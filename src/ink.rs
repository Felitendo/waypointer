//! Lines drawn with the pointer: smoothed through the midpoints, with an arrowhead while Ctrl
//! is held, fading after a while unless drawings are kept.

pub type Point = [f32; 2];

/// A drag shorter than this is a tap, not a line.
pub const MIN_DRAG: f32 = 4.0;

#[derive(Clone, Debug)]
pub struct Stroke {
    pub points: Vec<Point>,
    pub color: rune::Color,
    pub width: f32,
    pub arrow: bool,
    /// When it was finished, on the app clock. `None` while it is drawn.
    pub finished: Option<f64>,
}

impl Stroke {
    pub fn new(start: Point, color: rune::Color, width: f32) -> Self {
        Self { points: vec![start], color, width, arrow: false, finished: None }
    }

    pub fn add(&mut self, p: Point) {
        if let Some(last) = self.points.last()
            && (p[0] - last[0]).hypot(p[1] - last[1]) < 1.5
        {
            return;
        }
        self.points.push(p);
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

    /// The line and, with an arrow, the two strokes of its head.
    pub fn lines(&self) -> Vec<Vec<Point>> {
        let mut lines = vec![smoothed(&self.points)];
        if self.arrow {
            lines.extend(head(&self.points, (self.width * 3.2).max(14.0)));
        }
        lines
    }
}

/// Quadratic curves through the midpoints of the points, like TinyHand for macOS, cut into
/// short straight pieces.
pub fn smoothed(p: &[Point]) -> Vec<Point> {
    match p {
        [] => vec![],
        [only] => vec![*only, *only],
        _ => {
            let mut out = vec![p[0]];
            let mut from = p[0];
            for i in 1..p.len() {
                let control = p[i - 1];
                let to = [(p[i - 1][0] + p[i][0]) / 2.0, (p[i - 1][1] + p[i][1]) / 2.0];
                for step in 1..=4 {
                    let t = step as f32 / 4.0;
                    let u = 1.0 - t;
                    out.push([
                        u * u * from[0] + 2.0 * u * t * control[0] + t * t * to[0],
                        u * u * from[1] + 2.0 * u * t * control[1] + t * t * to[1],
                    ]);
                }
                from = to;
            }
            out.push(p[p.len() - 1]);
            out
        }
    }
}

/// An arrowhead aimed along the last stretch, so a wobble at the very end doesn't twist it.
pub fn head(p: &[Point], size: f32) -> Vec<Vec<Point>> {
    let Some(&end) = p.last() else { return vec![] };
    let back = p.iter().rev().find(|q| (end[0] - q[0]).hypot(end[1] - q[1]) >= size * 1.2).copied().unwrap_or(p[0]);
    let length = (end[0] - back[0]).hypot(end[1] - back[1]);
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
            vec![end, [end[0] + a.cos() * len, end[1] + a.sin() * len]]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoothing_keeps_the_ends() {
        let line = smoothed(&[[0.0, 0.0], [10.0, 0.0], [20.0, 10.0]]);
        assert_eq!(line.first(), Some(&[0.0, 0.0]));
        assert_eq!(line.last(), Some(&[20.0, 10.0]));
        assert!(line.len() > 3);
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
