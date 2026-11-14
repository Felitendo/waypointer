//! Where the hand on its stick goes. The stick comes up from below the screen, to the right of
//! the cursor, and leans so the fingertip is right on the cursor. The maths is TinyHand's for
//! macOS, worked out with y going up, and handed back in screen points with y going down.

use crate::hands::Metrics;

/// The hand's length as a share of the screen's height, at rest.
pub const DEFAULT_SIZE: f32 = 0.13;
const MIN_ROD: f32 = 0.05;
const GRIP_LEAN: f32 = 0.30;
const GRIP_DEPTH: f32 = 0.12;
const LIFT: f32 = 0.06;

/// The grip slides 8% of the way to its target every 1/120 s.
const GRIP_FOLLOW: f32 = 0.08;
/// A tap springs back 18% of the way every 1/120 s.
const PRESS_RETURN: f32 = 0.18;

/// What moves on its own between pointer events: the grip catching up, a tap springing back.
#[derive(Clone, Copy, Debug, Default)]
pub struct Motion {
    pub grip_x: Option<f32>,
    pub press: f32,
    pub held: bool,
}

impl Motion {
    /// Where the grip wants to be for a cursor at `cursor_x` on a screen `width` wide.
    pub fn grip_target(cursor_x: f32, width: f32) -> f32 {
        (cursor_x + GRIP_LEAN * width).clamp(width * 0.15, width * 1.05)
    }

    pub fn press(&mut self) {
        self.press = 1.0;
        self.held = true;
    }

    /// Advances by `dt` seconds. Returns whether anything is still moving.
    pub fn step(&mut self, dt: f32, target: f32) -> bool {
        let ticks = (dt * 120.0).max(0.0);
        let grip = self.grip_x.get_or_insert(target);
        *grip += (target - *grip) * (1.0 - (1.0 - GRIP_FOLLOW).powf(ticks));
        if (target - *grip).abs() < 0.3 {
            *grip = target;
        }
        if !self.held {
            self.press *= (1.0 - PRESS_RETURN).powf(ticks);
            if self.press < 0.002 {
                self.press = 0.0;
            }
        }
        self.settling(target)
    }

    pub fn settling(&self, target: f32) -> bool {
        self.press > 0.0 || self.grip_x.is_some_and(|g| (g - target).abs() > 0.0)
    }
}

/// The stick and hand, ready to draw: a box with the hand on top and the stick below it, turned
/// clockwise by `turn` about its `origin` (fractions of the box), which sits on the grip.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub hand_height: f32,
    pub rod_length: f32,
    pub origin: (f32, f32),
    /// Radians, clockwise on screen.
    pub turn: f32,
    /// The whole picture's height in points, for picking a picture size.
    pub picture_height: f32,
    /// How far the shadow falls, down and to the right on screen, and how soft it is.
    pub shadow: (f32, f32),
    pub shadow_blur: f32,
    pub shadow_alpha: f32,
}

impl Pose {
    pub fn height(&self) -> f32 {
        self.hand_height + self.rod_length
    }
}

/// The pose for a cursor at (`cx`, `cy`) on a screen of `width` × `height` points.
pub fn pose(m: &Metrics, size: f32, width: f32, height: f32, cx: f32, cy: f32, motion: &Motion) -> Pose {
    let press = motion.press;
    let grip_x = motion.grip_x.unwrap_or_else(|| Motion::grip_target(cx, width));
    // y up from here on.
    let grip_y = -GRIP_DEPTH * height;
    let dx = cx - grip_x;
    let dy = (height - cy) - grip_y;
    let reach = dx.hypot(dy);
    let aim = dy.atan2(dx);

    let hand = size * height * (1.0 - 0.06 * press);
    let s = hand / (m.wrist - m.hand_top);
    let w = s * m.aspect;
    let hand_tip = (m.wrist - m.tip) * s;
    let tip_off = (m.tip_x - m.rod_x) * w;
    let along = (reach * reach - tip_off * tip_off).max(0.0).sqrt();
    let rod = (along - hand_tip).max(MIN_ROD * height);
    let tip_angle = (rod + hand_tip).atan2(tip_off);
    // Counterclockwise with y up is clockwise on screen.
    let turn = -(aim - tip_angle);

    let lift = LIFT * height * (1.0 - 0.8 * press);
    let total = hand + rod;
    Pose {
        x: grip_x - m.rod_x * w,
        y: height * (1.0 + GRIP_DEPTH) - total,
        width: w,
        hand_height: hand,
        rod_length: rod,
        origin: (m.rod_x, 1.0),
        turn,
        picture_height: size * height / (m.wrist - m.hand_top),
        shadow: (0.6 * lift, lift),
        shadow_blur: (lift * 0.35).max(1.0) + 5.0 * press,
        shadow_alpha: 0.26 + 0.02 * press,
    }
}

/// Where the fingertip of a pose lands on screen.
#[cfg(test)]
pub fn fingertip(m: &Metrics, p: &Pose) -> (f32, f32) {
    let s = p.hand_height / (m.wrist - m.hand_top);
    let local = (m.tip_x * p.width, (m.tip - m.hand_top) * s);
    let pivot = (p.x + p.origin.0 * p.width, p.y + p.origin.1 * p.height());
    let (rx, ry) = (local.0 + p.x - pivot.0, local.1 + p.y - pivot.1);
    let (sin, cos) = p.turn.sin_cos();
    (pivot.0 + rx * cos - ry * sin, pivot.1 + rx * sin + ry * cos)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metrics() -> Metrics {
        Metrics { aspect: 1024.0 / 1536.0, hand_top: 0.08, tip: 0.084, wrist: 0.42, rod_end: 0.95, tip_x: 0.38, rod_x: 0.5, hand_min_x: 0.25, hand_max_x: 0.75 }
    }

    #[test]
    fn the_fingertip_lands_on_the_cursor() {
        let m = metrics();
        for &(cx, cy) in &[(700.0, 450.0), (100.0, 80.0), (1500.0, 900.0), (900.0, 200.0)] {
            let motion = Motion { grip_x: Some(Motion::grip_target(cx, 1600.0)), ..Default::default() };
            let p = pose(&m, DEFAULT_SIZE, 1600.0, 1000.0, cx, cy, &motion);
            let (tx, ty) = fingertip(&m, &p);
            assert!((tx - cx).abs() < 1.0 && (ty - cy).abs() < 1.0, "cursor {cx},{cy} tip {tx},{ty}");
        }
    }

    #[test]
    fn the_stick_leans_in_from_the_right() {
        let m = metrics();
        let motion = Motion { grip_x: Some(Motion::grip_target(700.0, 1600.0)), ..Default::default() };
        let p = pose(&m, DEFAULT_SIZE, 1600.0, 1000.0, 700.0, 450.0, &motion);
        assert!(p.turn < 0.0, "leans left at the top: {}", p.turn);
        assert!(p.y + p.height() > 1000.0, "comes from below the screen");
    }

    #[test]
    fn a_tap_springs_back() {
        let mut motion = Motion::default();
        motion.press();
        motion.held = false;
        let mut frames = 0;
        while motion.step(1.0 / 60.0, 100.0) {
            frames += 1;
            assert!(frames < 120);
        }
        assert_eq!(motion.press, 0.0);
    }

    #[test]
    fn the_grip_catches_up() {
        let mut motion = Motion { grip_x: Some(0.0), ..Default::default() };
        for _ in 0..240 {
            motion.step(1.0 / 120.0, 500.0);
        }
        assert_eq!(motion.grip_x, Some(500.0));
    }
}
