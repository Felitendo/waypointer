//! What all parts share: the settings, whether the pointer is on, where clicks go, and the
//! handles of the overlays and windows. Any thread may call it: the tray, D-Bus, the shortcut
//! listener and the overlays themselves.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use rune::{AppHandle, OverlayHandle, OverlayInput};

use crate::art::Cache;
use crate::hands::{self, Custom};
use crate::i18n::{tr, trf};
use crate::overlay::Screen;
use crate::settings::{InteractKey, Settings};
use crate::sound::Sound;

/// What the platform side does for the hub. Each part may be missing (not KDE, no KWin).
pub trait Services: Send + Sync {
    /// Esc turns the pointer off, but only while it is on.
    fn set_esc(&self, _on: bool) {}
    /// Follow the cursor while clicks pass through to the apps.
    fn set_tracking(&self, _on: bool) {}
    /// Ask where the panels are and where the cursor is; answered with `Hub::areas`.
    fn request_areas(&self) {}
    fn refresh_tray(&self) {}
    fn can_track(&self) -> bool {
        false
    }
}

pub struct NoServices;
impl Services for NoServices {}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Mode {
    pub active: bool,
    /// Latched with a double tap on Shift or the ink button: clicks reach the apps, the hand
    /// stays, and the toolbar still takes its own.
    pub click_through: bool,
    pub interact_held: bool,
    pub drawing: bool,
    pub ctrl: bool,
    /// Clicks go to the apps right now.
    pub pass: bool,
    /// They pass but the toolbar still takes its own: not while Alt is held.
    pub bar_only: bool,
    /// The real cursor shows instead of the hand.
    pub hand_hidden: bool,
    pub tracking: bool,
}

pub struct OverlayEntry {
    pub handle: OverlayHandle<Screen>,
    /// The output's name (DP-1), to match KWin's answer.
    pub name: Option<String>,
    /// Its top-left in compositor coordinates.
    pub origin: (f32, f32),
}

pub struct Hub {
    settings: Mutex<Settings>,
    custom: Mutex<Vec<Custom>>,
    mode: Mutex<Mode>,
    overlays: Mutex<Vec<OverlayEntry>>,
    pub art: Arc<Cache>,
    pub sound: Sound,
    services: OnceLock<Box<dyn Services>>,
    settings_window: OnceLock<AppHandle<crate::ui::Ui>>,
    quitting: AtomicBool,
    shift: Mutex<ShiftTaps>,
    clock: std::time::Instant,
    pub can_hold: AtomicBool,
}

#[derive(Default)]
struct ShiftTaps {
    down_at: Option<f64>,
    last_tap: Option<f64>,
}

impl Hub {
    pub fn new(settings: Settings) -> Arc<Hub> {
        Arc::new(Hub {
            settings: Mutex::new(settings),
            custom: Mutex::new(hands::load_custom()),
            mode: Mutex::new(Mode::default()),
            overlays: Mutex::new(Vec::new()),
            art: Arc::new(Cache::default()),
            sound: Sound::new(),
            services: OnceLock::new(),
            settings_window: OnceLock::new(),
            quitting: AtomicBool::new(false),
            shift: Mutex::new(ShiftTaps::default()),
            clock: std::time::Instant::now(),
            can_hold: AtomicBool::new(false),
        })
    }

    pub fn set_services(&self, services: Box<dyn Services>) {
        let _ = self.services.set(services);
    }

    fn services(&self) -> &dyn Services {
        static NONE: NoServices = NoServices;
        self.services.get().map(|s| s.as_ref()).unwrap_or(&NONE)
    }

    pub fn set_overlays(&self, overlays: Vec<OverlayEntry>) {
        *self.overlays.lock().unwrap() = overlays;
    }

    pub fn set_settings_window(&self, handle: AppHandle<crate::ui::Ui>) {
        let _ = self.settings_window.set(handle);
    }

    fn now(&self) -> f64 {
        self.clock.elapsed().as_secs_f64()
    }

    // --- settings -----------------------------------------------------------------------

    pub fn settings(&self) -> Settings {
        self.settings.lock().unwrap().clone()
    }

    pub fn with_settings<R>(&self, f: impl FnOnce(&Settings) -> R) -> R {
        f(&self.settings.lock().unwrap())
    }

    /// Changes the settings, saves them and tells everything that shows them.
    pub fn change(&self, f: impl FnOnce(&mut Settings)) {
        let (before, after) = {
            let mut s = self.settings.lock().unwrap();
            let before = s.clone();
            f(&mut s);
            s.size = s.size.clamp(crate::settings::SIZE_RANGE.0, crate::settings::SIZE_RANGE.1);
            s.volume = s.volume.clamp(0.0, 1.0);
            (before, s.clone())
        };
        if before == after {
            return;
        }
        after.save();
        if before.tap_sound != after.tap_sound {
            self.sound.load(&after.tap_sound);
        }
        if before.language != after.language {
            crate::i18n::set(&after.language);
        }
        if before.volume != after.volume {
            self.sound.set_volume(after.volume);
        }
        // Turned off while latched: let go, as nothing else might.
        if before.double_shift && !after.double_shift && self.mode().click_through {
            self.set_click_through(false);
        }
        self.update_mode();
        self.refresh();
        self.services().refresh_tray();
    }

    pub fn custom_hands(&self) -> Vec<Custom> {
        self.custom.lock().unwrap().clone()
    }

    pub fn set_custom_hands(&self, list: Vec<Custom>) {
        let _ = hands::save_custom(&list);
        *self.custom.lock().unwrap() = list;
        self.refresh();
    }

    /// Every pointer id: built-in hands, the user's own, the laser.
    pub fn styles(&self) -> Vec<String> {
        let mut ids: Vec<String> = hands::BUILT_IN.iter().map(|b| b.id.to_string()).collect();
        ids.extend(self.custom.lock().unwrap().iter().map(|c| c.id.clone()));
        ids.push(hands::LASER.to_string());
        ids
    }

    pub fn is_style(&self, id: &str) -> bool {
        self.styles().iter().any(|s| s == id)
    }

    pub fn valid_style(&self, id: &str) -> String {
        if self.is_style(id) { id.to_string() } else { "lavender".to_string() }
    }

    pub fn pinned(&self) -> Vec<String> {
        let raw = self.with_settings(|s| s.pinned.clone());
        let mut out: Vec<String> = Vec::new();
        for id in raw {
            if self.is_style(&id) && !out.contains(&id) {
                out.push(id);
            }
        }
        if out.is_empty() {
            out = hands::DEFAULT_PINNED.iter().map(|s| s.to_string()).collect();
        }
        out.truncate(hands::MAX_PINNED);
        out
    }

    pub fn title(&self, id: &str) -> String {
        if id == hands::LASER {
            return tr("Laser dot").into();
        }
        if let Some(c) = self.custom.lock().unwrap().iter().find(|c| c.id == id) {
            return c.name.clone();
        }
        hands::BUILT_IN.iter().find(|b| b.id == id).map(|b| tr(b.title).to_string()).unwrap_or_else(|| id.to_string())
    }

    /// The colour a pointer draws in: the one the user picked for it, or its own.
    pub fn ink_of(&self, id: &str) -> rune::Color {
        let picked = self.with_settings(|s| s.inks.get(id).copied());
        hands::rgb(picked.unwrap_or_else(|| self.own_ink(id)))
    }

    /// A pointer's own colour, as `0xRRGGBB`.
    pub fn own_ink(&self, id: &str) -> u32 {
        if id == hands::LASER {
            return hands::RED;
        }
        if let Some(c) = self.custom.lock().unwrap().iter().find(|c| c.id == id) {
            return c.ink;
        }
        hands::BUILT_IN.iter().find(|b| b.id == id).map(|b| b.ink).unwrap_or(0x8754F2)
    }

    /// Picks the colour a pointer draws in, or `None` for its own. The ink follows the pointer
    /// again, or the pick wouldn't show.
    pub fn set_ink_of(&self, id: &str, ink: Option<u32>) {
        self.change(|s| {
            match ink {
                Some(c) => s.inks.insert(id.to_string(), c),
                None => s.inks.remove(id),
            };
            s.ink = "match".into();
        });
    }

    /// The colour lines are drawn in: the user's pick, or the pointer's own.
    pub fn ink(&self) -> rune::Color {
        let (choice, style) = self.with_settings(|s| (s.ink.clone(), s.pointer.clone()));
        Settings::ink_color(&choice).unwrap_or_else(|| self.ink_of(&style))
    }

    pub fn style(&self) -> String {
        let id = self.with_settings(|s| s.pointer.clone());
        self.valid_style(&id)
    }

    pub fn sound_on(&self) -> bool {
        let laser = self.style() == hands::LASER;
        self.with_settings(|s| if laser { s.laser_sound } else { s.hand_sound })
    }

    pub fn set_sound_on(&self, on: bool) {
        let laser = self.style() == hands::LASER;
        self.change(|s| if laser { s.laser_sound = on } else { s.hand_sound = on });
    }

    /// Plays the tap sound once, at the volume set, to hear a change.
    pub fn preview(&self) {
        let name = self.with_settings(|s| s.tap_sound.clone());
        self.sound.preview(&name);
    }

    pub fn pick(&self, id: &str) {
        let id = self.valid_style(id);
        self.change(|s| s.pointer = id);
    }

    // --- on and off ---------------------------------------------------------------------

    pub fn mode(&self) -> Mode {
        *self.mode.lock().unwrap()
    }

    pub fn active(&self) -> bool {
        self.mode().active
    }

    pub fn toggle(&self) {
        if self.active() { self.deactivate("toggled") } else { self.activate() }
    }

    pub fn activate(&self) {
        {
            let mut m = self.mode.lock().unwrap();
            if m.active {
                return;
            }
            *m = Mode { active: true, ctrl: m.ctrl, ..Mode::default() };
        }
        let first = !self.with_settings(|s| s.hint_shown);
        let hint = first.then(|| self.first_hint());
        if first {
            self.change(|s| s.hint_shown = true);
        }
        let now = self.now();
        for o in self.overlays.lock().unwrap().iter() {
            let hint = hint.clone();
            o.handle.update(move |s: &mut Screen, _| s.turned_on(now, hint));
            o.handle.set_input(OverlayInput::Regions);
            o.handle.show();
        }
        self.services().request_areas();
        self.update_mode();
        self.services().refresh_tray();
    }

    pub fn deactivate(&self, reason: &str) {
        {
            let mut m = self.mode.lock().unwrap();
            if !m.active {
                return;
            }
            *m = Mode { ctrl: m.ctrl, ..Mode::default() };
        }
        eprintln!("waypointer: pointer off ({reason})");
        for o in self.overlays.lock().unwrap().iter() {
            o.handle.update(|s: &mut Screen, _| s.turned_off());
            o.handle.hide();
        }
        self.update_mode();
        self.services().refresh_tray();
    }

    pub fn clear(&self) {
        for o in self.overlays.lock().unwrap().iter() {
            o.handle.update(|s: &mut Screen, _| s.strokes.clear());
        }
    }

    /// Esc, or the toolbar's close button.
    pub fn exit(&self) {
        self.clear();
        self.deactivate("exit");
    }

    fn first_hint(&self) -> Vec<(String, String)> {
        let s = self.settings();
        let mut parts = vec![(tr("Click").to_string(), tr("to tap").to_string()), (tr("Drag").into(), tr("to draw").into())];
        if self.can_hold.load(Ordering::Relaxed) && s.interact_key != InteractKey::Off {
            let key = if s.interact_key == InteractKey::Ctrl { tr("Ctrl") } else { tr("Alt") };
            parts.push((trf("Hold {}", &[&key]), tr("to click for real").into()));
        }
        if self.can_hold.load(Ordering::Relaxed) && s.double_shift {
            parts.push((tr("Double-tap ⇧").into(), tr("to click through").into()));
        }
        parts.push((tr("Esc").into(), tr("to exit").into()));
        parts
    }

    /// Redraws every overlay and the settings window with the latest settings.
    pub fn refresh(&self) {
        for o in self.overlays.lock().unwrap().iter() {
            o.handle.update(|_: &mut Screen, _| {});
        }
        if let Some(window) = self.settings_window.get() {
            window.update(|_, _| {});
        }
    }

    pub fn refresh_tray(&self) {
        self.services().refresh_tray();
    }

    pub fn can_track(&self) -> bool {
        self.services().can_track()
    }

    // --- where clicks go ----------------------------------------------------------------

    pub fn set_drawing(&self, on: bool) {
        let changed = {
            let mut m = self.mode.lock().unwrap();
            let changed = m.drawing != on;
            m.drawing = on;
            changed
        };
        if changed {
            self.update_mode();
        }
    }

    fn update_mode(&self) {
        let s = self.settings();
        let can_track = self.services().can_track();
        let (before, after) = {
            let mut m = self.mode.lock().unwrap();
            let before = *m;
            let interact = m.interact_held && !m.drawing && s.interact_key != InteractKey::Off;
            m.pass = m.active && (m.click_through || interact);
            m.bar_only = m.pass && !interact;
            let keep = m.pass && can_track && (m.click_through || s.interact_keeps_hand);
            m.hand_hidden = m.pass && !keep;
            m.tracking = keep;
            (before, *m)
        };
        // All clicks pass (Alt held), or the overlay takes those on its `hit` parts: everything,
        // or while clicks go through just the toolbar.
        let all = |m: &Mode| m.pass && !m.bar_only;
        if all(&before) != all(&after) && after.active {
            let input = if all(&after) { OverlayInput::None } else { OverlayInput::Regions };
            for o in self.overlays.lock().unwrap().iter() {
                o.handle.set_input(input);
            }
            // Back from passing every click on: show the hand where the cursor is now.
            if !all(&after) {
                self.services().request_areas();
            }
        }
        if before.tracking != after.tracking {
            self.services().set_tracking(after.tracking);
        }
        self.services().set_esc(after.active && !after.click_through);
        if before != after {
            for o in self.overlays.lock().unwrap().iter() {
                o.handle.update(move |s: &mut Screen, _| s.mode(&after));
            }
            if before.click_through != after.click_through {
                self.services().refresh_tray();
            }
        }
    }

    /// Clicks go through to the apps, or the hand taps and draws again: a double tap on Shift
    /// or the toolbar's ink button.
    pub fn set_click_through(&self, on: bool) {
        {
            let mut m = self.mode.lock().unwrap();
            if !m.active || m.click_through == on {
                return;
            }
            m.click_through = on;
        }
        let keep = self.services().can_track();
        let shift = self.can_hold.load(Ordering::Relaxed) && self.with_settings(|s| s.double_shift);
        let mut hint: Vec<(String, String)> = if on {
            vec![(tr("Clicks go through").into(), if keep { tr("the hand stays").into() } else { tr("to the apps").into() })]
        } else {
            vec![(tr("Taps").into(), tr("clicks don't reach apps").into())]
        };
        if shift {
            hint.push((tr("Double-tap ⇧").into(), if on { tr("to tap again") } else { tr("to click through") }.into()));
        }
        let now = self.now();
        for o in self.overlays.lock().unwrap().iter() {
            let hint = hint.clone();
            o.handle.update(move |s: &mut Screen, _| s.show_hint(hint, now, 1.6));
        }
        self.update_mode();
    }

    /// Alt, Ctrl, Shift or Meta went down or up anywhere.
    pub fn modifier(&self, key: Modifier, pressed: bool) {
        let s = self.settings();
        if key == Modifier::Ctrl {
            self.mode.lock().unwrap().ctrl = pressed;
            for o in self.overlays.lock().unwrap().iter() {
                o.handle.update(move |s: &mut Screen, _| s.set_arrow(pressed));
            }
        }
        let interact = match s.interact_key {
            InteractKey::Alt => Some(Modifier::Alt),
            InteractKey::Ctrl => Some(Modifier::Ctrl),
            InteractKey::Off => None,
        };
        if Some(key) == interact {
            self.mode.lock().unwrap().interact_held = pressed;
            self.update_mode();
        }

        // A clean double tap on Shift: two short presses, nothing else in between.
        let now = self.now();
        let mut taps = self.shift.lock().unwrap();
        if key != Modifier::Shift {
            if pressed {
                *taps = ShiftTaps::default();
            }
            return;
        }
        if !self.active() || !s.double_shift {
            *taps = ShiftTaps::default();
            return;
        }
        if pressed {
            taps.down_at = Some(now);
            return;
        }
        let Some(down) = taps.down_at.take() else { return };
        if now - down > 0.3 {
            taps.last_tap = None;
            return;
        }
        if taps.last_tap.is_some_and(|last| down - last < 0.35) {
            taps.last_tap = None;
            drop(taps);
            let on = !self.mode().click_through;
            self.set_click_through(on);
            return;
        }
        taps.last_tap = Some(now);
    }

    /// A click on an overlay spoils a double tap on Shift.
    pub fn tapped(&self) {
        *self.shift.lock().unwrap() = ShiftTaps::default();
        if self.sound_on() {
            self.sound.play();
        }
    }

    /// From KWin: where the panels leave room on each screen, and where the cursor is.
    pub fn areas(&self, areas: HashMap<String, [f32; 4]>, cursor: Option<(f32, f32)>) {
        if !self.active() {
            return;
        }
        for o in self.overlays.lock().unwrap().iter() {
            let area = o.name.as_ref().and_then(|n| areas.get(n)).map(|a| [a[0] - o.origin.0, a[1] - o.origin.1, a[2], a[3]]);
            let local = cursor.map(|c| [c.0 - o.origin.0, c.1 - o.origin.1]);
            o.handle.update(move |s: &mut Screen, _| s.areas(area, local));
        }
    }

    /// From KWin, while clicks pass through: where the cursor went.
    pub fn tracked(&self, x: f32, y: f32) {
        if !self.mode().tracking {
            return;
        }
        for o in self.overlays.lock().unwrap().iter() {
            let local = [x - o.origin.0, y - o.origin.1];
            o.handle.update(move |s: &mut Screen, tx| s.tracked(local, tx));
        }
    }

    /// One overlay got the pointer: it shows the toolbar, the others don't.
    pub fn pointer_on(&self, index: usize) {
        for (i, o) in self.overlays.lock().unwrap().iter().enumerate() {
            if i != index {
                o.handle.update(|s: &mut Screen, _| s.current = false);
            }
        }
    }

    // --- windows ------------------------------------------------------------------------

    pub fn show_settings(&self) {
        // The overlay would sit over the window.
        self.exit();
        if let Some(window) = self.settings_window.get() {
            window.open();
        }
    }

    pub fn quit(&self) {
        if self.quitting.swap(true, Ordering::SeqCst) {
            return;
        }
        self.deactivate("quit");
        if let Some(window) = self.settings_window.get() {
            window.quit();
        }
    }

    /// The state for scripts, as `$XDG_RUNTIME_DIR/waypointer/state.json` and over D-Bus.
    pub fn state_json(&self) -> String {
        let styles: Vec<serde_json::Value> = self
            .styles()
            .iter()
            .map(|id| {
                let kind = if id == hands::LASER { "laser" } else if id.starts_with("custom-") { "custom" } else { "hand" };
                serde_json::json!({ "id": id, "name": self.title(id), "type": kind })
            })
            .collect();
        let m = self.mode();
        serde_json::to_string_pretty(&serde_json::json!({
            "pid": std::process::id(),
            "active": m.active,
            "clickThrough": m.click_through,
            "style": self.style(),
            "soundOn": self.sound_on(),
            "styles": styles,
        }))
        .unwrap_or_default()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Modifier {
    Alt,
    Ctrl,
    Shift,
    Meta,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tap_shift(hub: &Hub) {
        hub.modifier(Modifier::Shift, true);
        hub.modifier(Modifier::Shift, false);
    }

    #[test]
    fn double_shift_and_the_ink_button_are_one_switch() {
        crate::settings::test_config();
        let hub = Hub::new(Settings { hint_shown: true, ..Settings::default() });
        hub.activate();
        assert!(!hub.mode().pass, "the hand taps and draws at first");

        hub.set_click_through(true);
        let m = hub.mode();
        assert!(m.click_through && m.pass && m.bar_only, "clicks go to the apps, the toolbar takes its own");
        tap_shift(&hub);
        tap_shift(&hub);
        assert!(!hub.mode().pass, "a double tap on Shift turns it off again");
        tap_shift(&hub);
        tap_shift(&hub);
        assert!(hub.mode().click_through && hub.mode().bar_only, "and on, with the toolbar still working");

        hub.modifier(Modifier::Alt, true);
        let m = hub.mode();
        assert!(m.pass && !m.bar_only, "holding Alt lets every click through");
        hub.modifier(Modifier::Alt, false);
        hub.set_click_through(false);
        assert!(!hub.mode().pass);
    }
}
