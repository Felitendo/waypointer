//! The icon in the system tray: click to turn the pointer on or off, right-click for the menu.

use std::sync::Arc;

use ksni::blocking::{Handle, TrayMethods};
use ksni::menu::StandardItem;
use ksni::{MenuItem, ToolTip};

use crate::hub::Hub;
use crate::i18n::tr;
use crate::platform;

struct Tray {
    hub: Arc<Hub>,
}

impl ksni::Tray for Tray {
    fn id(&self) -> String {
        "waypointer".into()
    }

    fn title(&self) -> String {
        "Waypointer".into()
    }

    fn icon_theme_path(&self) -> String {
        // Always name the folder the icon is in. Without it Plasma looks the name up in the icon
        // theme, which it reads only at its own start (so a fresh install isn't there yet) and
        // where it doesn't colour the symbolic icon for the panel: dark on a dark panel.
        let candidates = [
            dirs::data_dir().map(|d| d.join("icons")),
            Some("/usr/local/share/icons".into()),
            Some("/usr/share/icons".into()),
            Some(concat!(env!("CARGO_MANIFEST_DIR"), "/res/icons").into()),
        ];
        candidates
            .into_iter()
            .flatten()
            .find(|d: &std::path::PathBuf| d.join("hicolor/scalable/status/waypointer-off-symbolic.svg").exists())
            .map(|d| d.display().to_string())
            .unwrap_or_default()
    }

    fn icon_name(&self) -> String {
        let m = self.hub.mode();
        if m.click_through {
            "waypointer-through".into()
        } else if m.active {
            "waypointer-on-symbolic".into()
        } else {
            "waypointer-off-symbolic".into()
        }
    }

    fn tool_tip(&self) -> ToolTip {
        let m = self.hub.mode();
        let text = if m.click_through {
            tr("Clicks go through. Double-tap Shift to tap again, click to turn off.")
        } else if m.active {
            tr("The pointer is on. Click to turn it off, right-click for the menu.")
        } else {
            tr("Click to turn the pointer on, right-click for the menu.")
        };
        ToolTip { title: "Waypointer".into(), description: text.into(), ..Default::default() }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.hub.toggle();
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let shortcuts = platform::SHORTCUTS.lock().unwrap();
        let with_key = |label: &str, key: i32| {
            if key == 0 { label.to_string() } else { format!("{label}\t{}", platform::key_text(key)) }
        };
        let on = self.hub.active();
        let sound = self.hub.sound_on();
        vec![
            StandardItem {
                label: with_key(if on { tr("Turn Pointer Off") } else { tr("Turn Pointer On") }, shortcuts.toggle),
                icon_name: if on { "process-stop".into() } else { "tool-pointer".into() },
                activate: Box::new(|t: &mut Self| t.hub.toggle()),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: with_key(tr("Clear Drawing"), shortcuts.clear),
                icon_name: "edit-clear-all".into(),
                activate: Box::new(|t: &mut Self| t.hub.clear()),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: if sound { tr("Mute Tap Sound") } else { tr("Turn Tap Sound On") }.into(),
                icon_name: if sound { "audio-volume-muted".into() } else { "audio-volume-high".into() },
                activate: Box::new(|t: &mut Self| {
                    let on = !t.hub.sound_on();
                    t.hub.set_sound_on(on);
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: tr("Settings…").into(),
                icon_name: "configure".into(),
                activate: Box::new(|t: &mut Self| t.hub.show_settings()),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: tr("Quit Waypointer").into(),
                icon_name: "application-exit".into(),
                activate: Box::new(|t: &mut Self| t.hub.quit()),
                ..Default::default()
            }
            .into(),
        ]
    }
}

#[derive(Clone)]
pub struct TrayHandle(Handle<Tray>);

impl TrayHandle {
    pub fn refresh(&self) {
        self.0.update(|_| {});
    }
}

pub fn start(hub: Arc<Hub>) -> Option<TrayHandle> {
    Tray { hub }.spawn().map(TrayHandle).map_err(|e| eprintln!("waypointer: no tray: {e}")).ok()
}
