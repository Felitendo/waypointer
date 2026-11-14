//! Waypointer: a tiny hand on a stick that points at things on your screen. The Linux version
//! of TinyHand for macOS, drawn with Rune.

#[macro_use]
mod i18n;
mod arm;
mod art;
mod handmaker;
mod hands;
mod hub;
mod icons;
mod ink;
mod keystate;
mod overlay;
mod platform;
mod settings;
mod sound;
mod tray;
mod ui;

use rune::{App, DisplayTarget, KeyPolicy, OverlayInput, OverlayOptions, OverlayPlacement, OverlaySpec, RunOptions, WindowSpec};

use crate::hub::{Hub, OverlayEntry};
use crate::overlay::Screen;
use crate::settings::Settings;

const USAGE: &str = "Usage: waypointer [command]

Without a command, Waypointer starts in the system tray.

Commands (they start Waypointer when it isn't running):
  on, off, toggle      turn the pointer on, off, or flip it
  clear                clear the drawing
  style <id>           switch to a hand (or laser) and turn the pointer on
  sound on|off|toggle  the tap sound of the current pointer
  volume <0-100>       the tap sound's volume in percent
  settings             open the settings
  quit                 quit Waypointer
  waypointer://...     the same as a link, for example waypointer://style/robot

Options:
  --autostart          start quietly, as at login
  --version            print the version
";

/// Text drawn with macOS's font smoothing: fuller and softer than Chromium's thin Linux text,
/// which is Rune's default here. WP_FONT_SMOOTHING picks another mode, for comparing.
fn font_smoothing() -> rune::FontSmoothing {
    let wanted = std::env::var("WP_FONT_SMOOTHING").unwrap_or_default().to_lowercase();
    rune::FontSmoothing::ALL
        .iter()
        .copied()
        .find(|m| format!("{m:?}").to_lowercase() == wanted)
        .unwrap_or(rune::FontSmoothing::Windfoil)
}

/// Rune's own flags (headless snapshots and the like), which go to Rune untouched.
fn is_rune_flag(arg: &str) -> bool {
    arg.starts_with("--") && !matches!(arg, "--autostart" | "--version" | "--help")
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print!("{USAGE}");
        return;
    }
    if args.iter().any(|a| a == "--version") {
        println!("waypointer {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    // Rune's tooling (`--snap out.png --scene general`) takes every argument, values too.
    let headless = args.iter().any(|a| is_rune_flag(a));
    let rune_args: Vec<String> = if headless { args.clone() } else { Vec::new() };
    let autostart = args.iter().any(|a| a == "--autostart");
    let command: Vec<String> = if headless { Vec::new() } else { args.iter().filter(|a| !a.starts_with("--")).cloned().collect() };

    let settings = Settings::load();
    i18n::set(&settings.language);
    let hub = Hub::new(settings.clone());

    // One Waypointer at a time: a second start hands its command over.
    let bus = if headless { None } else {
        match platform::serve(&hub) {
            Ok(Some(conn)) => Some(conn),
            Ok(None) => {
                if autostart {
                    return;
                }
                let command = if command.is_empty() { vec!["settings".to_string()] } else { command };
                if let Err(e) = platform::send(&command) {
                    eprintln!("waypointer: {e}");
                    std::process::exit(1);
                }
                return;
            }
            Err(e) => {
                eprintln!("waypointer: no D-Bus session ({e}), running without it");
                None
            }
        }
    };

    if !headless {
        hub.sound.start(&settings.tap_sound, settings.volume);
        keystate::start(hub.clone());
        let tray = tray::start(hub.clone());
        platform::start(&hub, tray);
    }

    // KWin finds the window's icon and name through this, in de.felitendo.waypointer.desktop.
    rune::set_app_id("de.felitendo.waypointer");

    let h = hub.clone();
    let first_run = !settings.welcome_shown && !autostart && !headless;
    rune::run_windows_with(RunOptions { accessory: true }, move |displays| {
        let hub = h.clone();
        let ui_hub = hub.clone();
        let mut app = App::new(ui::spec()).init(move || ui::Ui::new(ui_hub.clone())).args(rune_args.clone())
            .font_smoothing(font_smoothing());
        if !first_run && !headless {
            app = app.windowless();
        }
        let mut entries = Vec::new();
        for (index, display) in displays.iter().enumerate() {
            let target = display.id.clone().map_or(DisplayTarget::Primary, DisplayTarget::Id);
            let spec = OverlaySpec::new("waypointer", (800.0, 600.0), overlay::view)
                .options(OverlayOptions {
                    display: target,
                    placement: Some(OverlayPlacement::FullScreen),
                    input: OverlayInput::Regions,
                    keys: KeyPolicy::None,
                    ..Default::default()
                })
                .poll(overlay::poll);
            let handle = app.overlay(spec, Screen::new(hub.clone(), index));
            let b = display.native_bounds();
            if std::env::var_os("WP_DEBUG").is_some() {
                eprintln!("waypointer: display {:?} position {:?} size {:?} scale {} bounds {b:?}", display.name, display.position, display.size, display.scale);
            }
            entries.push(OverlayEntry { handle, name: display.name.clone(), origin: (b[0] as f32, b[1] as f32) });
        }
        hub.set_overlays(entries);
        hub.set_settings_window(app.handle());
        let thumbs = hub.clone();
        hub.art.load_thumbs(hub.styles(), move || thumbs.refresh());

        if first_run {
            hub.change(|s| s.welcome_shown = true);
        }
        if !hub.with_settings(|s| s.login_set_up) {
            // Like on the Mac: an installed Waypointer opens at login from the start.
            let installed = std::env::current_exe().is_ok_and(|p| p.starts_with("/usr/"));
            if installed {
                settings::set_launch_at_login(true);
            }
            hub.change(|s| s.login_set_up = true);
        }
        if !command.is_empty() {
            let hub = hub.clone();
            let command = command.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(300));
                if let Err(e) = platform::run_command(&hub, &command) {
                    eprintln!("waypointer: {e}");
                }
            });
        }
        hub.refresh_tray();
        vec![WindowSpec::from_app("settings", app)]
    });
    drop(bus);
    platform::clean_up();
}
