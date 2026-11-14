//! Everything the user can set, kept in ~/.config/waypointer/settings.json. Nobody ever has to
//! open that file: every setting is in the settings window. WP_CONFIG points it elsewhere.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::hands;

pub const TAP_SOUNDS: &[(&str, &str)] = &[
    ("thock", n!("Thock")), ("click", n!("Click")), ("tock", n!("Tock")), ("clack", n!("Clack")),
    ("typewriter", n!("Typewriter")), ("snap", n!("Snap")), ("wood", n!("Wood block")), ("pop", n!("Pop")),
    ("boop", n!("Boop")), ("blip", n!("Blip")), ("coin", n!("Coin")), ("pew", n!("Pew")),
    ("squeak", n!("Squeak")), ("quack", n!("Quack")), ("chirp", n!("Chirp")),
];

/// The ink colours, `match` first: the pointer's own colour.
pub const INKS: &[(&str, &str, u32)] = &[
    ("match", n!("Match the pointer"), 0),
    ("red", n!("Red"), hands::RED),
    ("orange", n!("Orange"), 0xFF8A00),
    ("yellow", n!("Yellow"), 0xFFCC00),
    ("green", n!("Green"), 0x30C25A),
    ("blue", n!("Blue"), 0x0A84FF),
    ("purple", n!("Purple"), 0x8754F2),
    ("white", n!("White"), 0xFFFFFF),
    ("black", n!("Black"), 0x1D1D21),
];

/// Drawings fade after this many seconds; 0 keeps them.
pub const FADES: &[(f64, &str)] = &[(1.0, n!("1 s")), (2.0, n!("2 s")), (3.0, n!("3 s")), (5.0, n!("5 s")), (10.0, n!("10 s")), (0.0, n!("Keep"))];

pub const SIZE_RANGE: (f32, f32) = (0.08, 0.20);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InteractKey {
    #[default]
    Alt,
    Ctrl,
    Off,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub pointer: String,
    pub size: f32,
    pub shadow: bool,
    pub fade_seconds: f64,
    pub hand_sound: bool,
    pub laser_sound: bool,
    pub tap_sound: String,
    /// The tap sound's volume, 0 to 1.
    pub volume: f32,
    pub interact_key: InteractKey,
    pub interact_keeps_hand: bool,
    pub double_shift: bool,
    pub ink: String,
    /// A language code, or "system".
    pub language: String,
    pub show_toolbar: bool,
    pub show_ink_button: bool,
    pub pinned: Vec<String>,
    /// Shown once each.
    pub welcome_shown: bool,
    pub hint_shown: bool,
    pub login_set_up: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            pointer: "lavender".into(),
            size: crate::arm::DEFAULT_SIZE,
            shadow: true,
            fade_seconds: 3.0,
            hand_sound: true,
            laser_sound: false,
            tap_sound: "thock".into(),
            volume: 0.5,
            interact_key: InteractKey::Alt,
            interact_keeps_hand: false,
            double_shift: true,
            ink: "match".into(),
            language: "system".into(),
            show_toolbar: true,
            show_ink_button: true,
            pinned: hands::DEFAULT_PINNED.iter().map(|s| s.to_string()).collect(),
            welcome_shown: false,
            hint_shown: false,
            login_set_up: false,
        }
    }
}

impl Settings {
    pub fn path() -> PathBuf {
        if let Some(path) = std::env::var_os("WP_CONFIG") {
            return PathBuf::from(path);
        }
        dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("waypointer/settings.json")
    }

    pub fn load() -> Settings {
        let mut s: Settings = std::fs::read(Self::path())
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        s.size = s.size.clamp(SIZE_RANGE.0, SIZE_RANGE.1);
        s.volume = s.volume.clamp(0.0, 1.0);
        if !TAP_SOUNDS.iter().any(|t| t.0 == s.tap_sound) {
            s.tap_sound = "thock".into();
        }
        if !INKS.iter().any(|i| i.0 == s.ink) {
            s.ink = "match".into();
        }
        s
    }

    pub fn save(&self) {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let tmp = path.with_extension("json.tmp");
        if let Ok(bytes) = serde_json::to_vec_pretty(self)
            && std::fs::write(&tmp, bytes).is_ok()
        {
            let _ = std::fs::rename(tmp, path);
        }
    }

    /// Back to how it was at first. The one-time flags stay.
    pub fn reset(&mut self) {
        *self = Settings {
            language: self.language.clone(),
            welcome_shown: self.welcome_shown,
            hint_shown: self.hint_shown,
            login_set_up: self.login_set_up,
            ..Settings::default()
        };
    }

    /// The ink colour of a choice, or `None` for "match the pointer".
    pub fn ink_color(choice: &str) -> Option<rune::Color> {
        INKS.iter().find(|i| i.0 == choice && i.0 != "match").map(|i| hands::rgb(i.2))
    }
}

pub fn autostart_file() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("autostart/de.felitendo.waypointer.desktop")
}

pub fn launch_at_login() -> bool {
    autostart_file().exists()
}

pub fn set_launch_at_login(on: bool) {
    let path = autostart_file();
    if !on {
        let _ = std::fs::remove_file(path);
        return;
    }
    let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_else(|_| "waypointer".into());
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(
        path,
        format!(
            "[Desktop Entry]\nType=Application\nName=Waypointer\nComment=A tiny hand on a stick that points at things on your screen\n\
             Exec={exe} --autostart\nIcon=de.felitendo.waypointer\nTerminal=false\nNoDisplay=true\n\
             X-GNOME-Autostart-enabled=true\nX-KDE-autostart-phase=2\n"
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_fields_take_their_defaults() {
        let s: Settings = serde_json::from_str(r#"{"pointer": "robot", "size": 5.0}"#).unwrap();
        assert_eq!(s.pointer, "robot");
        assert!(s.shadow);
        assert_eq!(s.volume, 0.5, "half volume until the user picks one");
        assert_eq!(s.pinned.len(), hands::DEFAULT_PINNED.len());
    }

    #[test]
    fn reset_keeps_the_one_time_flags() {
        let mut s = Settings { pointer: "robot".into(), welcome_shown: true, ..Settings::default() };
        s.reset();
        assert_eq!(s.pointer, "lavender");
        assert!(s.welcome_shown);
    }
}
