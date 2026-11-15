//! The see-through layer over one screen while the pointer is on: the hand (or the laser dot),
//! the drawing, the tap ripples and the toolbar. On Wayland it is a layer-shell surface above
//! everything, full screen windows too. Only the area without panels takes the pointer, so the
//! panels stay usable with the normal cursor, like the menu bar on the Mac.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use rune::tok::motion;
use rune::{Align, Color, Cursor, Cx, Drag, El, Phase, Side, Transition, Tx, col, el, row, text};

use crate::arm::{self, Motion};
use crate::hands;
use crate::hub::Hub;
use crate::ink::{self, Stroke};
use crate::raster::{self, StrokeTiles};

const BAR_GAP: f32 = 14.0;
/// How long a tray stays after the pointer left it and its button, to cross the gap.
const TRAY_GRACE: f64 = 0.35;
const GLASS: Color = Color { r: 0.075, g: 0.075, b: 0.09, a: 0.88 };
const EDGE: Color = Color { r: 1.0, g: 1.0, b: 1.0, a: 0.09 };

#[derive(Clone, Debug)]
struct Ripple {
    at: [f32; 2],
    born: f64,
    color: Color,
}

/// What opens above the toolbar while the pointer is on its button.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Tray {
    Ink,
    Volume,
}

pub struct Screen {
    pub hub: Arc<Hub>,
    pub index: usize,
    /// The surface's size in points.
    pub size: [f32; 2],
    /// The part without panels, in the surface's points.
    pub work: Option<[f32; 4]>,
    pub pointer: Option<[f32; 2]>,
    /// The pointer is on this screen: it shows the toolbar and the hint.
    pub current: bool,
    pub hand_hidden: bool,
    pub tracking: bool,
    /// Clicks go to the apps: the screen doesn't catch them.
    passing: bool,
    /// But the toolbar still takes its own.
    bar_only: bool,
    /// Latched by the ink button or a double tap on Shift: the ink button shows it.
    through: bool,
    pub motion: Motion,
    pub strokes: Vec<Stroke>,
    drag_start: Option<[f32; 2]>,
    drawing: bool,
    arrow: bool,
    tray: Option<Tray>,
    tray_close_at: Option<f64>,
    /// On the volume tray: the hand steps aside, the real cursor shows.
    over_volume: bool,
    bar: Option<[f32; 4]>,
    reveal_until: f64,
    hint: Vec<(String, String)>,
    hint_until: f64,
    ripples: Vec<Ripple>,
    now: f64,
    last_poll: Option<f64>,
    /// The drawing's pictures, by stroke. Drawn in `view`, which knows the scale.
    pictures: RefCell<HashMap<u64, StrokeTiles>>,
}

impl Screen {
    pub fn new(hub: Arc<Hub>, index: usize) -> Screen {
        Screen {
            hub,
            index,
            size: [1920.0, 1080.0],
            work: None,
            pointer: None,
            current: index == 0,
            hand_hidden: false,
            tracking: false,
            passing: false,
            bar_only: false,
            through: false,
            motion: Motion::default(),
            strokes: Vec::new(),
            drag_start: None,
            drawing: false,
            arrow: false,
            tray: None,
            tray_close_at: None,
            over_volume: false,
            bar: None,
            reveal_until: 0.0,
            hint: Vec::new(),
            hint_until: 0.0,
            ripples: Vec::new(),
            now: 0.0,
            last_poll: None,
            pictures: RefCell::new(HashMap::new()),
        }
    }

    fn work(&self) -> [f32; 4] {
        self.work.unwrap_or([0.0, 0.0, self.size[0], self.size[1]])
    }

    pub fn turned_on(&mut self, now: f64, hint: Option<Vec<(String, String)>>) {
        self.pointer = None;
        self.work = None;
        self.motion = Motion::default();
        self.drag_start = None;
        self.drawing = false;
        self.close_tray();
        self.reveal_until = now + 3.0;
        self.ripples.clear();
        if let Some(hint) = hint {
            self.show_hint(hint, now, 10.0);
            self.reveal_until = now + 10.0;
        }
    }

    pub fn turned_off(&mut self) {
        self.strokes.clear();
        self.pointer = None;
        self.drag_start = None;
        self.drawing = false;
        self.hint.clear();
        self.ripples.clear();
    }

    /// Where clicks go now, from the hub.
    pub fn mode(&mut self, m: &crate::hub::Mode) {
        self.hand_hidden = m.hand_hidden;
        self.tracking = m.tracking;
        self.passing = m.pass;
        self.bar_only = m.bar_only;
        self.through = m.click_through;
    }

    pub fn show_hint(&mut self, hint: Vec<(String, String)>, now: f64, seconds: f64) {
        self.hint = hint;
        self.hint_until = now + seconds;
    }

    pub fn set_arrow(&mut self, on: bool) {
        self.arrow = on;
        if let Some(stroke) = self.strokes.last_mut()
            && stroke.finished.is_none()
        {
            stroke.set_arrow(on);
        }
    }

    pub fn areas(&mut self, area: Option<[f32; 4]>, cursor: Option<[f32; 2]>) {
        if std::env::var_os("WP_DEBUG").is_some() {
            eprintln!("waypointer: screen {} size {:?} area {area:?} cursor {cursor:?}", self.index, self.size);
        }
        self.work = area;
        if let Some(c) = cursor {
            let [x, y, w, h] = self.work();
            if c[0] >= x && c[1] >= y && c[0] < x + w && c[1] < y + h {
                if self.pointer.is_none() {
                    self.pointer = Some(c);
                }
                self.current = true;
                self.hub.pointer_on(self.index);
            }
        }
    }

    pub fn tracked(&mut self, at: [f32; 2], tx: &mut Tx) {
        let inside = at[0] >= 0.0 && at[1] >= 0.0 && at[0] < self.size[0] && at[1] < self.size[1];
        self.pointer = inside.then_some(at);
        if inside && !self.current {
            self.current = true;
            self.hub.pointer_on(self.index);
        }
        tx.wake_after(Duration::ZERO);
    }

    fn moved(&mut self, at: [f32; 2], tx: &mut Tx) {
        if !self.current && std::env::var_os("WP_DEBUG").is_some() {
            eprintln!("waypointer: pointer on screen {} at {at:?}", self.index);
        }
        if !self.current {
            self.current = true;
            self.hub.pointer_on(self.index);
            self.reveal_until = self.reveal_until.max(tx.clock() + 1.5);
        }
        self.pointer = Some(at);
        tx.wake_after(Duration::ZERO);
    }

    fn left(&mut self) {
        if !self.tracking && self.drag_start.is_none() {
            self.pointer = None;
        }
    }

    fn tap(&mut self, at: [f32; 2], tx: &mut Tx) {
        self.moved(at, tx);
        self.motion.press();
        self.hub.tapped();
        self.hint.clear();
        self.close_tray();
        self.ripples.push(Ripple { at, born: tx.clock(), color: self.hub.ink() });
    }

    fn close_tray(&mut self) {
        self.tray = None;
        self.tray_close_at = None;
        self.over_volume = false;
    }

    /// The pointer went onto or off a tray or its button. The tray opens at once and goes a
    /// moment after the pointer left both.
    fn hover_tray(&mut self, tray: Tray, on: bool, tx: &mut Tx) {
        if on {
            self.tray = Some(tray);
            self.tray_close_at = None;
        } else if self.tray == Some(tray) {
            self.tray_close_at = Some(tx.clock() + TRAY_GRACE);
            tx.wake_after(Duration::from_secs_f64(TRAY_GRACE));
        }
    }

    fn drag(&mut self, d: &Drag, tx: &mut Tx) {
        let [ox, oy, _, _] = self.work();
        let at = [d.x + ox, d.y + oy];
        match d.phase {
            Phase::Down => {
                self.tap(at, tx);
                self.drag_start = Some(at);
            }
            Phase::Up => {
                self.moved(at, tx);
                self.drag_start = None;
                self.motion.held = false;
                if self.drawing {
                    self.drawing = false;
                    if let Some(stroke) = self.strokes.last_mut() {
                        stroke.finished = Some(tx.clock());
                    }
                    self.hub.set_drawing(false);
                }
            }
            _ => {
                self.moved(at, tx);
                let Some(start) = self.drag_start else { return };
                if !self.drawing {
                    if (at[0] - start[0]).hypot(at[1] - start[1]) < ink::MIN_DRAG {
                        return;
                    }
                    self.drawing = true;
                    self.hub.set_drawing(true);
                    let width = (self.size[1] * 0.0055).max(3.0);
                    self.strokes.push(Stroke::new(start, self.hub.ink(), width));
                }
                let arrow = self.arrow;
                if let Some(stroke) = self.strokes.last_mut() {
                    stroke.set_arrow(arrow);
                    stroke.add(at);
                }
            }
        }
    }

    fn revealed(&self, show_toolbar: bool) -> bool {
        if !show_toolbar || !self.current || self.drawing {
            return false;
        }
        // While clicks go through and no cursor comes from KWin, the toolbar is the way back.
        if self.now < self.reveal_until || self.tray.is_some() || (self.bar_only && !self.tracking) {
            return true;
        }
        let (Some(p), Some([x, y, w, h])) = (self.pointer, self.bar) else { return false };
        p[0] >= x - 80.0 && p[0] <= x + w + 80.0 && p[1] >= y - 100.0 && p[1] <= y + h + 10.0
    }
}

/// Runs every frame while something moves on its own, and at deadlines otherwise.
pub fn poll(s: &mut Screen, t: f64, tx: &mut Tx) -> bool {
    let dt = s.last_poll.map_or(0.0, |last| (t - last) as f32).clamp(0.0, 0.1);
    s.last_poll = Some(t);
    let before_reveal = s.revealed(true);
    s.now = t;
    let mut changed = false;
    let mut moving = false;

    if let Some(p) = s.pointer {
        let target = Motion::grip_target(p[0], s.size[0]);
        let before = (s.motion.grip_x, s.motion.press);
        moving |= s.motion.step(dt, target);
        changed |= before != (s.motion.grip_x, s.motion.press);
    }
    let fade = s.hub.with_settings(|st| st.fade_seconds);
    let count = s.strokes.len();
    s.strokes.retain(|st| !st.gone(t, fade));
    changed |= count != s.strokes.len();
    if fade > 0.0 && s.strokes.iter().any(|st| st.finished.is_some_and(|f| t > f + fade.max(0.5))) {
        changed = true;
        moving = true;
    }
    if !s.ripples.is_empty() {
        s.ripples.retain(|r| t - r.born < 0.45);
        changed = true;
        moving = true;
    }
    if !s.hint.is_empty() && t > s.hint_until {
        s.hint.clear();
        changed = true;
    }
    if s.tray_close_at.is_some_and(|at| t >= at) {
        s.close_tray();
        changed = true;
    }
    changed |= before_reveal != s.revealed(true);

    if moving {
        tx.wake_after(Duration::ZERO);
    } else {
        let next = [s.reveal_until, s.hint_until, s.tray_close_at.unwrap_or(0.0)].into_iter().filter(|d| *d > t).fold(f64::INFINITY, f64::min);
        if next.is_finite() {
            tx.wake_after(Duration::from_secs_f64(next - t));
        }
    }
    changed
}

pub fn view(s: &Screen, cx: &mut Cx<Screen>) -> El {
    let settings = s.hub.settings();
    let style = s.hub.style();
    let [wx, wy, ww, wh] = s.work();
    let scale = cx.env().scale;

    let mut root = el().key("root").w_full().h_full().on_layout(|s: &mut Screen, r| s.size = [r[2], r[3]]);
    // The drawing, under everything else.
    root = drawing(root, s, settings.fade_seconds, scale);

    // Where the pointer is caught: everything but the panels, unless clicks go to the apps.
    let mut catcher = el().key("catcher").size(ww, wh).pin(Some(wx), Some(wy), None, None)
        .on_pointer_move_local(|s: &mut Screen, p, tx| s.moved([p.window_x, p.window_y], tx))
        .on_hover_change(|s: &mut Screen, on, _| if !on { s.left() });
    if !s.passing {
        catcher = catcher.hit().cursor(Cursor::Hidden)
            .on_drag(|s: &mut Screen, d, tx| s.drag(d, tx))
            .on_secondary_press_local(|s: &mut Screen, p, tx| {
                s.tap([p.window_x, p.window_y], tx);
                s.motion.held = false;
            });
    }
    root = root.child(catcher);

    // Tap ripples.
    for r in &s.ripples {
        let t = ((s.now - r.born) / 0.45).clamp(0.0, 1.0) as f32;
        let ease = 1.0 - (1.0 - t).powi(3);
        let radius = 6.0 + 26.0 * ease;
        root = root.child(
            el().size(radius * 2.0, radius * 2.0).radius(radius).border(2.5 * (1.0 - t) + 0.5, r.color.alpha(0.85 * (1.0 - t)))
                .pin(Some(r.at[0] - radius), Some(r.at[1] - radius), None, None).motion(rune_motion_none()),
        );
    }

    // The toolbar and the hint, at the bottom of the screen without its panels.
    let revealed = s.revealed(settings.show_toolbar);
    let mut stack = col().key("bar-stack").w(ww).pin(Some(wx), None, None, Some(s.size[1] - (wy + wh) + BAR_GAP)).center().gap(10.0);
    if s.current && !s.hint.is_empty() {
        stack = stack.child(hint_view(&s.hint));
        // Room for an open tray, so the hint sits above it.
        if revealed && s.tray.is_some() {
            stack = stack.child(el().h(46.0));
        }
    }
    if revealed {
        stack = stack.child(toolbar(s, &style, &settings));
    }
    root = root.child(stack);
    // An open tray, right above its button. The gap clears the toolbar's padding.
    if revealed && s.tray == Some(Tray::Ink) && settings.show_ink_button {
        root = root.child(ink_tray(&settings.ink).anchored("ink", Side::Top, Align::Center, 18.0));
    }
    if revealed && s.tray == Some(Tray::Volume) {
        root = root.child(volume_tray(s.hub.sound_on(), settings.volume).anchored("sound", Side::Top, Align::Center, 18.0));
    }

    // The pointer, on top of everything.
    if let Some(p) = s.pointer
        && s.current
        && !s.hand_hidden
        && !s.over_volume
    {
        if style == hands::LASER {
            root = laser(root, s, p, settings.size);
        } else {
            root = hand(root, s, &style, p, settings.size, settings.shadow, scale);
        }
    }
    root
}

fn rune_motion_none() -> rune::Spring {
    rune::Spring::NONE
}

/// The drawing as tile pictures, drawn again where it changed.
fn drawing(root: El, s: &Screen, fade: f64, scale: f32) -> El {
    let mut pictures = s.pictures.borrow_mut();
    pictures.retain(|id, _| s.strokes.iter().any(|st| st.id == *id));
    let side = raster::TILE as f32 / scale;
    let none = rune_motion_none();
    let mut root = root;
    for stroke in &s.strokes {
        let tiles = pictures.entry(stroke.id).or_default();
        tiles.update(stroke, scale);
        let alpha = stroke.opacity(s.now, fade);
        if alpha <= 0.0 {
            continue;
        }
        for ((tx, ty), pixels) in &tiles.tiles {
            root = root.child(
                el().key(format!("ink-{}-{tx}-{ty}", stroke.id)).size(side, side)
                    .pin(Some(*tx as f32 * side), Some(*ty as f32 * side), None, None)
                    .pixels(pixels).opacity(alpha).motion(none),
            );
        }
    }
    root
}

fn hand(root: El, s: &Screen, style: &str, p: [f32; 2], size: f32, shadow: bool, scale: f32) -> El {
    // The picture's height at rest decides its pixel size, so a tap doesn't swap it. The hand
    // is about a third of its picture.
    let rest_points = size * s.size[1] / 0.34;
    let px = crate::art::picture_px(rest_points, scale);
    let hub = s.hub.clone();
    let Some(art) = s.hub.art.art(style, px, move || hub.refresh()) else { return root };
    let pose = arm::pose(&art.metrics, size, s.size[0], s.size[1], p[0], p[1], &s.motion);
    let (w, hh, rod) = (pose.width, pose.hand_height, pose.rod_length);
    let none = rune_motion_none();
    let mut root = root;

    if shadow {
        let pad = art.pad * w;
        let shadow_el = el().key("hand-shadow").size(w, pose.height())
            .pin(Some(pose.x + pose.shadow.0), Some(pose.y + pose.shadow.1), None, None)
            .origin(pose.origin.0, pose.origin.1).rotate(pose.turn).rotate_motion(none).motion(none)
            .opacity(pose.shadow_alpha)
            .child(el().size(w + 2.0 * pad, hh + 2.0 * pad).pixels(&art.hand_shadow).pin(Some(-pad), Some(-pad), None, None).motion(none))
            .child(el().size(w + 2.0 * pad, rod + 2.0 * pad).pixels(&art.rod_shadow).pin(Some(-pad), Some(hh - pad), None, None).motion(none));
        root = root.child(shadow_el);
    }
    root.child(
        el().key("hand").size(w, pose.height()).pin(Some(pose.x), Some(pose.y), None, None)
            .origin(pose.origin.0, pose.origin.1).rotate(pose.turn).rotate_motion(none).motion(none)
            .child(el().size(w, hh).pixels(&art.hand).pin(Some(0.0), Some(0.0), None, None).motion(none))
            .child(el().size(w, rod).pixels(&art.rod).pin(Some(0.0), Some(hh), None, None).motion(none)),
    )
}

fn laser(root: El, s: &Screen, p: [f32; 2], size: f32) -> El {
    let red = hands::rgb(hands::RED);
    let r = (s.size[1] * 0.0038 * size / 0.13).max(4.0) * (1.0 + 0.2 * s.motion.press);
    let none = rune_motion_none();
    root.child(
        el().key("laser").size(2.0 * r, 2.0 * r).radius(r).fill(red)
            .shadow(0.0, r * (3.0 + 1.5 * s.motion.press), red.alpha(0.95))
            .shadow(0.0, r * (8.0 + 3.0 * s.motion.press), red.alpha(0.5))
            .pin(Some(p[0] - r), Some(p[1] - r), None, None).motion(none)
            .child(el().size(r * 0.9, r * 0.9).radius(r * 0.45).fill(Color { r: 1.0, g: 0.86, b: 0.82, a: 0.95 })
                .pin(Some(r * 0.55), Some(r * 0.55), None, None).motion(none)),
    )
}

fn hint_view(parts: &[(String, String)]) -> El {
    let mut r = row().key("hint").px(16.0).py(9.0).gap(14.0).radius(12.0).fill(GLASS).border(1.0, EDGE)
        .shadow(6.0, 18.0, Color { r: 0.0, g: 0.0, b: 0.0, a: 0.3 })
        .transition(Transition::offset(0.0, 10.0).combined(Transition::opacity()));
    for (key, what) in parts {
        r = r.child(row().gap(5.0)
            .child(text(key.as_str()).font(rune::tok::TextStyle::new(13.0, 600.0)).color(Color::WHITE))
            .child(text(what.as_str()).font(rune::tok::TextStyle::new(13.0, 400.0)).color(Color::WHITE.alpha(0.68))));
    }
    r
}

fn toolbar(s: &Screen, style: &str, settings: &crate::settings::Settings) -> El {
    let mut styles = s.hub.pinned();
    if !styles.iter().any(|id| id == style) {
        styles.push(style.to_string());
    }
    let mut bar = row().key("toolbar").hit().cursor(Cursor::Hidden).padding(8.0).gap(6.0).radius(18.0)
        .fill(GLASS).border(1.0, EDGE).shadow(10.0, 30.0, Color { r: 0.0, g: 0.0, b: 0.0, a: 0.35 })
        .transition(Transition::offset(0.0, 18.0).combined(Transition::scale(0.96)).combined(Transition::opacity()))
        .on_layout(|s: &mut Screen, r| s.bar = Some(r))
        .on_pointer_move_local(|s: &mut Screen, p, tx| s.moved([p.window_x, p.window_y], tx));
    for id in styles {
        let selected = id == style;
        let mut b = tool_button(&format!("style-{id}"), selected);
        if id == hands::LASER {
            b = b.child(el().size(12.0, 12.0).radius(6.0).fill(hands::rgb(hands::RED))
                .shadow(0.0, 10.0, hands::rgb(hands::RED).alpha(0.8)));
        } else if let Some(thumb) = s.hub.art.thumb(&id) {
            b = b.child(el().size(36.0, 36.0).pixels(&thumb));
        }
        let pick = id.clone();
        bar = bar.child(b.on_press(move |s: &mut Screen| s.hub.pick(&pick)));
    }
    bar = bar.child(el().size(1.0, 24.0).fill(Color::WHITE.alpha(0.16)));
    // A click lets clicks through (no drawing) or mutes the sound; the pointer on the button
    // opens its tray.
    if settings.show_ink_button {
        let mut b = tool_button("ink", false).child(ink_dot(s.hub.ink(), 24.0).opacity(if s.through { 0.4 } else { 1.0 }));
        if s.through {
            b = b.child(crate::icons::slash(30.0, Color::WHITE.alpha(0.9)).pin(Some(6.0), Some(6.0), None, None));
        }
        bar = bar.child(b
            .on_hover_change(|s: &mut Screen, on, tx| s.hover_tray(Tray::Ink, on, tx))
            .on_press(|s: &mut Screen| {
                let on = !s.hub.mode().click_through;
                s.hub.set_click_through(on);
            }));
    }
    let sound = s.hub.sound_on() && settings.volume > 0.0;
    bar = bar.child(tool_button("sound", false).child(crate::icons::speaker(sound, 18.0, Color::WHITE.alpha(if sound { 0.88 } else { 0.45 })))
        .on_hover_change(|s: &mut Screen, on, tx| s.hover_tray(Tray::Volume, on, tx))
        .on_press(move |s: &mut Screen| {
            if sound {
                s.hub.set_sound_on(false);
                return;
            }
            if s.hub.with_settings(|st| st.volume) <= 0.0 {
                s.hub.change(|st| st.volume = 0.5);
            }
            s.hub.set_sound_on(true);
            s.hub.preview();
        }));
    bar.child(tool_button("exit", false).child(crate::icons::close(16.0, Color::WHITE.alpha(0.88)))
        .on_press(|s: &mut Screen| s.hub.exit()))
}

fn tool_button(key: &str, selected: bool) -> El {
    let mut b = el().key(key).size(42.0, 42.0).radius(12.0).center().hit().cursor(Cursor::Hidden)
        .hover(Color::WHITE.alpha(0.08)).press_scale(0.88);
    if selected {
        // One highlight that glides from the old choice to the new one.
        b = b.child(el().key("selection").matched("toolbar-selection").size(42.0, 42.0).radius(12.0)
            .fill(Color::WHITE.alpha(0.17)).pin(Some(0.0), Some(0.0), None, None).motion(motion::SNAPPY));
    }
    b
}

fn ink_dot(color: Color, size: f32) -> El {
    el().size(size, size).radius(size / 2.0).fill(color).border(1.5, Color::WHITE.alpha(0.25))
        .shadow(1.5, 4.0, Color { r: 0.0, g: 0.0, b: 0.0, a: 0.4 })
}

fn ink_tray(current: &str) -> El {
    let mut r = row().key("ink-tray").hit().cursor(Cursor::Hidden).padding(6.0).gap(2.0).radius(15.0)
        .fill(GLASS).border(1.0, EDGE).shadow(8.0, 22.0, Color { r: 0.0, g: 0.0, b: 0.0, a: 0.3 })
        .transition(Transition::offset(0.0, 8.0).combined(Transition::scale(0.9)).combined(Transition::opacity()))
        .on_pointer_move_local(|s: &mut Screen, p, tx| s.moved([p.window_x, p.window_y], tx))
        .on_hover_change(|s: &mut Screen, on, tx| s.hover_tray(Tray::Ink, on, tx));
    for (id, _, rgb) in crate::settings::INKS {
        let selected = *id == current;
        let dot = if *id == "match" { crate::icons::rainbow(20.0) } else { ink_dot(hands::rgb(*rgb), 20.0) };
        let mut b = el().key(format!("ink-{id}")).size(32.0, 32.0).radius(16.0).center().hit().cursor(Cursor::Hidden)
            .hover(Color::WHITE.alpha(0.08)).press_scale(0.85);
        if selected {
            b = b.child(el().matched("ink-selection").size(32.0, 32.0).radius(16.0).fill(Color::WHITE.alpha(0.2))
                .pin(Some(0.0), Some(0.0), None, None));
        }
        let choice = id.to_string();
        // Picking a colour means drawing.
        r = r.child(b.child(dot).on_press(move |s: &mut Screen| {
            s.hub.change(|st| st.ink = choice.clone());
            s.hub.set_click_through(false);
            s.close_tray();
        }));
    }
    r
}

/// Above the speaker button: a slider for the volume. The hand steps aside here, so it hides
/// nothing, and the real cursor shows.
fn volume_tray(on: bool, volume: f32) -> El {
    let (w, h, k, bar) = (150.0, 26.0, 16.0, 4.0);
    let travel = w - k;
    let value = move |x: f32| ((x - k / 2.0) / travel).clamp(0.0, 1.0);
    let shown = if on { volume } else { 0.0 };
    let slider = el().key("volume-slider").hit().cursor(Cursor::Default).size(w, h)
        .on_drag(move |s: &mut Screen, d: &mut Drag, tx| {
            let v = value(d.x);
            tx.instant();
            s.hub.change(|st| st.volume = v);
            if !s.hub.sound_on() && v > 0.0 {
                s.hub.set_sound_on(true);
            }
            if d.phase == Phase::Up {
                s.hub.preview();
            }
        })
        .child(el().size(w, bar).pin(Some(0.0), Some((h - bar) / 2.0), None, None).radius(bar / 2.0).fill(Color::WHITE.alpha(0.18)))
        .child(el().size(k / 2.0 + shown * travel, bar).pin(Some(0.0), Some((h - bar) / 2.0), None, None).radius(bar / 2.0).fill(Color::WHITE.alpha(0.9)))
        .child(el().size(k, k).pin(Some(shown * travel), Some((h - k) / 2.0), None, None).radius(k / 2.0).fill(Color::WHITE)
            .shadow(1.0, 3.0, Color::BLACK.alpha(0.35)));
    row().key("volume-tray").hit().cursor(Cursor::Default).padding(6.0).px(12.0).gap(10.0).center_y().radius(15.0)
        .fill(GLASS).border(1.0, EDGE).shadow(8.0, 22.0, Color { r: 0.0, g: 0.0, b: 0.0, a: 0.3 })
        .transition(Transition::offset(0.0, 8.0).combined(Transition::scale(0.9)).combined(Transition::opacity()))
        .on_pointer_move_local(|s: &mut Screen, p, tx| s.moved([p.window_x, p.window_y], tx))
        .on_hover_change(|s: &mut Screen, on, tx| {
            s.over_volume = on;
            s.hover_tray(Tray::Volume, on, tx);
        })
        .child(slider)
        .child(text(format!("{:.0} %", shown * 100.0)).w(40.0).font(rune::tok::TextStyle::new(12.5, 600.0)).color(Color::WHITE.alpha(0.75)))
}

/// Scenes for checking the drawing and the laser without a desktop:
/// `waypointer --snap out.png --scene ink` (or `laser`), with `--scale 2` for a HiDPI screen.
pub fn preview_spec() -> rune::AppSpec<Screen> {
    rune::AppSpec {
        scenes: &[("ink", preview_ink), ("laser", preview_laser), ("toolbar", preview_toolbar)],
        ..rune::AppSpec::new("Waypointer", (900.0, 560.0), || Screen::new(Hub::new(crate::settings::Settings::default()), 0), preview_view)
    }
}

/// The overlay over a light and a dark half, as over a slide and a dark app. The toolbar's
/// scene shows it over a dark desktop.
fn preview_view(s: &Screen, cx: &mut Cx<Screen>) -> El {
    let backdrop = if s.reveal_until.is_infinite() {
        el().w_full().h_full().fill(hands::rgb(0x1C1C21))
    } else {
        row().w_full().h_full()
            .child(el().grow(1.0).h_full().fill(hands::rgb(0xEDEDF0)))
            .child(el().grow(1.0).h_full().fill(hands::rgb(0x1C1C21)))
    };
    el().w_full().h_full()
        .child(backdrop.pin(Some(0.0), Some(0.0), None, None))
        .child(view(s, cx).pin(Some(0.0), Some(0.0), None, None))
}

pub fn is_preview(scene: &str) -> bool {
    matches!(scene, "ink" | "laser" | "toolbar")
}

/// Like a mouse: whole pixels.
fn mouse(points: impl Iterator<Item = [f32; 2]>) -> Vec<[f32; 2]> {
    points.map(|p| [p[0].round(), p[1].round()]).collect()
}

fn preview_stroke(s: &mut Screen, points: Vec<[f32; 2]>, color: u32, arrow: bool) {
    let mut stroke = Stroke::new(points[0], hands::rgb(color), 6.0);
    for p in &points[1..] {
        stroke.add(*p);
    }
    stroke.set_arrow(arrow);
    stroke.finished = Some(0.0);
    s.strokes.push(stroke);
}

fn preview_ink(s: &mut Screen) {
    // Loops, a ring around something, an arrow and a slow diagonal.
    let loops = mouse((0..520).map(|i| {
        let a = i as f32 * 0.03;
        [90.0 + a * 22.0 + 34.0 * a.cos(), 130.0 + 30.0 * a.sin()]
    }));
    preview_stroke(s, loops, hands::RED, false);
    let ring = mouse((0..400).map(|i| {
        let a = 0.4 + i as f32 * 0.0155;
        [660.0 + 150.0 * a.cos(), 330.0 + 85.0 * a.sin()]
    }));
    preview_stroke(s, ring, 0x0A84FF, false);
    let arrow = mouse((0..300).map(|i| {
        let t = i as f32 / 299.0;
        [110.0 + 380.0 * t, 470.0 - 160.0 * t - 70.0 * (t * std::f32::consts::PI).sin()]
    }));
    preview_stroke(s, arrow, 0x30C25A, true);
    let stairs = mouse((0..180).map(|i| [560.0 + i as f32 * 1.3, 470.0 + i as f32 * 0.35]));
    preview_stroke(s, stairs, 0xFFCC00, false);
}

/// The toolbar with the colours open, as the pointer on the ink button opens them.
fn preview_toolbar(s: &mut Screen) {
    // Its pictures first, so they show.
    let (done, wait) = std::sync::mpsc::channel();
    s.hub.art.load_thumbs(s.hub.pinned(), move || {
        let _ = done.send(());
    });
    let _ = wait.recv_timeout(Duration::from_secs(10));
    // A snap is one frame, before layout tells the size.
    s.size = [900.0, 560.0];
    s.reveal_until = f64::INFINITY;
    s.tray = Some(Tray::Ink);
}

fn preview_laser(s: &mut Screen) {
    s.hub = Hub::new(crate::settings::Settings { pointer: hands::LASER.into(), ..Default::default() });
    // On the edge of the two halves.
    s.pointer = Some([450.0, 280.0]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune::{Env, Runtime};

    /// One overlay with its toolbar showing and settled, its settings in a file of its own.
    fn overlay(settings: crate::settings::Settings) -> (Arc<Hub>, Runtime<Screen>) {
        crate::settings::test_config();
        let hub = Hub::new(settings);
        let mut screen = Screen::new(hub.clone(), 0);
        screen.pointer = Some([800.0, 500.0]);
        screen.reveal_until = f64::INFINITY;
        let mut rt = Runtime::new(screen, view, (1600.0, 1000.0), Env { system_dark: true, scale: 1.0 });
        frames(&mut rt, 90);
        (hub, rt)
    }

    fn frames(rt: &mut Runtime<Screen>, n: usize) {
        for _ in 0..n {
            rt.frame(1.0 / 60.0);
        }
    }

    fn hover(rt: &mut Runtime<Screen>, key: &str) {
        let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} shows"));
        rt.pointer_move(x, y);
        frames(rt, 10);
    }

    fn click(rt: &mut Runtime<Screen>, key: &str) {
        hover(rt, key);
        let (x, y) = rt.find(key).unwrap();
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        frames(rt, 10);
    }

    /// The pointer goes off the toolbar, and the tray's moment to cross the gap runs out.
    fn away(rt: &mut Runtime<Screen>) {
        rt.pointer_move(800.0, 300.0);
        frames(rt, 2);
        rt.update(|s, tx| {
            poll(s, tx.clock() + 1.0, tx);
        });
        frames(rt, 40);
    }

    /// What the hub's overlay handles do in the app.
    fn sync(rt: &mut Runtime<Screen>, hub: &Hub) {
        let m = hub.mode();
        rt.update(move |s, _| s.mode(&m));
        frames(rt, 2);
    }

    fn catches(rt: &Runtime<Screen>, (x, y): (f32, f32)) -> bool {
        rt.hit_regions().iter().any(|r| x >= r[0] && x <= r[2] && y >= r[1] && y <= r[3])
    }

    fn strokes(rt: &mut Runtime<Screen>) -> usize {
        let mut n = 0;
        rt.update(|s, _| n = s.strokes.len());
        n
    }

    fn drag(rt: &mut Runtime<Screen>) {
        rt.pointer_down(300.0, 300.0);
        for i in 1..=10 {
            rt.pointer_move(300.0 + i as f32 * 20.0, 300.0 + i as f32 * 8.0);
        }
        rt.pointer_up(500.0, 380.0);
        frames(rt, 2);
    }

    #[test]
    fn the_speaker_mutes_on_a_click_and_shows_the_volume_on_hover() {
        // The laser dot: the hands' pictures load in the background.
        let (hub, mut rt) = overlay(crate::settings::Settings { pointer: hands::LASER.into(), laser_sound: true, ..Default::default() });
        hover(&mut rt, "sound");
        assert!(rt.find("volume-tray").is_some(), "the pointer on the speaker opens the volume");
        assert!(rt.find("laser").is_some(), "the pointer shows on the toolbar");

        assert!(hub.sound_on());
        click(&mut rt, "sound");
        assert!(!hub.sound_on(), "a click mutes");
        click(&mut rt, "sound");
        assert!(hub.sound_on(), "a second click turns the sound back on");

        let [sx, sy, sw, _] = rt.presented("volume-slider").expect("the slider shows").0;
        let y = sy + 13.0;
        rt.pointer_move(sx + sw * 0.9, y);
        frames(&mut rt, 10);
        assert!(rt.find("laser").is_none(), "the pointer steps aside on the volume");
        rt.pointer_down(sx + sw * 0.9, y);
        rt.pointer_move(sx + sw * 0.3, y);
        rt.pointer_up(sx + sw * 0.3, y);
        frames(&mut rt, 2);
        let volume = hub.settings().volume;
        assert!(volume > 0.2 && volume < 0.4, "the drag set the volume: {volume}");

        away(&mut rt);
        assert!(rt.find("volume-tray").is_none(), "the volume goes once the pointer left");
        assert!(rt.find("laser").is_some(), "and the pointer is back");
    }

    #[test]
    fn with_drawing_off_clicks_reach_the_apps_but_the_toolbar_works() {
        let (hub, mut rt) = overlay(crate::settings::Settings { hint_shown: true, ..Default::default() });
        hub.activate();
        sync(&mut rt, &hub);
        assert!(catches(&rt, (300.0, 300.0)), "with drawing on, the screen takes the clicks");
        hover(&mut rt, "ink");
        assert!(rt.find("ink-tray").is_some(), "the pointer on the ink button opens the colours");

        click(&mut rt, "ink");
        assert!(hub.mode().click_through, "a click lets clicks through");
        sync(&mut rt, &hub);
        away(&mut rt);
        assert!(rt.find("ink-tray").is_none(), "the colours go once the pointer left");
        assert!(!catches(&rt, (300.0, 300.0)), "clicks on the screen reach the apps");
        let ink = rt.find("ink").expect("the toolbar stays");
        assert!(catches(&rt, ink), "the toolbar still takes its clicks");
        drag(&mut rt);
        assert_eq!(strokes(&mut rt), 0, "a drag draws nothing");

        hover(&mut rt, "ink");
        click(&mut rt, "ink-red");
        assert!(!hub.mode().click_through, "picking a colour means drawing again");
        assert_eq!(hub.settings().ink, "red");
        sync(&mut rt, &hub);
        away(&mut rt);
        assert!(catches(&rt, (300.0, 300.0)));
        drag(&mut rt);
        assert_eq!(strokes(&mut rt), 1, "now a drag draws");
    }

}
