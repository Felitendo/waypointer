//! Everything Waypointer asks of the desktop over D-Bus: being the one running instance and
//! taking commands, global shortcuts through KGlobalAccel (KDE), small KWin scripts that say
//! where the panels and the cursor are, and the screen locking or the computer sleeping.
//!
//! Calls go out on a worker thread with a connection of its own: a blocking call made from
//! inside a D-Bus handler on the same connection would wait for itself.

use std::collections::HashMap;
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};

use zbus::blocking::{Connection, MessageIterator, Proxy};
use zbus::interface;

use crate::hub::{Hub, Services};
use crate::i18n::tr;

pub const BUS_NAME: &str = "de.felitendo.Waypointer";
pub const PATH: &str = "/Waypointer";

// --- the running instance's D-Bus API --------------------------------------------------------

struct Api {
    hub: Arc<Hub>,
}

#[interface(name = "de.felitendo.Waypointer")]
impl Api {
    /// A command as typed after `waypointer`. Returns an empty string when it worked.
    fn run(&self, command: Vec<String>) -> String {
        run_command(&self.hub, &command).err().unwrap_or_default()
    }
    fn state(&self) -> String {
        self.hub.state_json()
    }
    fn on(&self) {
        self.hub.activate();
    }
    fn off(&self) {
        self.hub.deactivate("D-Bus");
    }
    fn toggle(&self) {
        self.hub.toggle();
    }
    fn clear(&self) {
        self.hub.clear();
    }
}

/// The free area of each screen by output name, and the cursor.
type Areas = (HashMap<String, [f32; 4]>, Option<(f32, f32)>);

/// What KWin scripts call back with.
struct KWinHelper {
    hub: Arc<Hub>,
    areas: Mutex<Areas>,
}

fn numbers(text: &str) -> Vec<f32> {
    text.split(',').filter_map(|v| v.trim().parse().ok()).collect()
}

#[interface(name = "de.felitendo.Waypointer.KWinHelper")]
impl KWinHelper {
    // Numbers come as text: KWin hands script numbers over as int or double as it likes, and
    // D-Bus only finds a method whose types match.
    fn area(&self, output: String, rect: String) {
        if let [x, y, w, h] = numbers(&rect)[..] {
            self.areas.lock().unwrap().0.insert(output, [x, y, w, h]);
        }
    }
    fn cursor(&self, point: String) {
        if let [x, y] = numbers(&point)[..] {
            self.areas.lock().unwrap().1 = Some((x, y));
            self.hub.tracked(x, y);
        }
    }
    fn done(&self) {
        let (areas, cursor) = std::mem::take(&mut *self.areas.lock().unwrap());
        self.hub.areas(areas, cursor);
    }
}

/// Runs a command for the CLI, D-Bus and waypointer:// links.
pub fn run_command(hub: &Hub, args: &[String]) -> Result<(), String> {
    let args: Vec<String> = match args {
        [one] if one.starts_with("waypointer:") || one.starts_with("tinyhand:") => from_url(one),
        _ => args.to_vec(),
    };
    let verb = args.first().map(|s| s.to_lowercase()).unwrap_or_default();
    let arg = args.get(1).map(|s| s.to_lowercase()).unwrap_or_default();
    match verb.as_str() {
        "on" => hub.activate(),
        "off" => hub.deactivate("command"),
        "toggle" => hub.toggle(),
        "clear" => hub.clear(),
        "style" => {
            let id = args.get(1).cloned().unwrap_or_default();
            if !hub.is_style(&id) {
                return Err(format!("unknown pointer style: {id}"));
            }
            hub.pick(&id);
            hub.activate();
        }
        "sound" => match arg.as_str() {
            "on" => hub.set_sound_on(true),
            "off" => hub.set_sound_on(false),
            "" | "toggle" => hub.set_sound_on(!hub.sound_on()),
            other => return Err(format!("unknown sound value: {other}")),
        },
        "volume" => {
            let percent: f32 = arg.trim_end_matches('%').parse().map_err(|_| format!("volume needs a number from 0 to 100, not {arg:?}"))?;
            hub.change(|s| s.volume = (percent / 100.0).clamp(0.0, 1.0));
        }
        "settings" | "" => hub.show_settings(),
        "quit" => hub.quit(),
        other => return Err(format!("unknown command: {other}")),
    }
    Ok(())
}

/// `waypointer://style/robot` → ["style", "robot"].
pub fn from_url(url: &str) -> Vec<String> {
    let rest = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    rest.split('/').filter(|p| !p.is_empty()).map(|p| p.to_string()).collect()
}

/// Takes the bus name and serves the API. `Ok(None)` when another Waypointer has the name.
pub fn serve(hub: &Arc<Hub>) -> zbus::Result<Option<Connection>> {
    let conn = Connection::session()?;
    conn.object_server().at(PATH, Api { hub: hub.clone() })?;
    conn.object_server().at("/KWinHelper", KWinHelper { hub: hub.clone(), areas: Mutex::default() })?;
    match conn.request_name_with_flags(BUS_NAME, zbus::fdo::RequestNameFlags::DoNotQueue.into()) {
        Ok(zbus::fdo::RequestNameReply::PrimaryOwner) | Ok(zbus::fdo::RequestNameReply::AlreadyOwner) => Ok(Some(conn)),
        Ok(_) | Err(zbus::Error::NameTaken) => Ok(None),
        Err(e) => Err(e),
    }
}

/// Hands a command to the running Waypointer.
pub fn send(command: &[String]) -> Result<(), String> {
    let conn = Connection::session().map_err(|e| e.to_string())?;
    let proxy = Proxy::new(&conn, BUS_NAME, PATH, "de.felitendo.Waypointer").map_err(|e| e.to_string())?;
    let answer: String = proxy.call("Run", &(command.to_vec(),)).map_err(|e| e.to_string())?;
    if answer.is_empty() { Ok(()) } else { Err(answer) }
}

// --- the worker: shortcuts, KWin scripts, the tray, the state file ---------------------------

enum Job {
    Esc(bool),
    Tracking(bool),
    Areas,
    Tray,
}

pub struct Desktop {
    jobs: Sender<Job>,
    can_track: bool,
}

impl Services for Desktop {
    fn set_esc(&self, on: bool) {
        let _ = self.jobs.send(Job::Esc(on));
    }
    fn set_tracking(&self, on: bool) {
        let _ = self.jobs.send(Job::Tracking(on));
    }
    fn request_areas(&self) {
        let _ = self.jobs.send(Job::Areas);
    }
    fn refresh_tray(&self) {
        let _ = self.jobs.send(Job::Tray);
    }
    fn can_track(&self) -> bool {
        self.can_track
    }
}

const COMPONENT: &str = "waypointer";
const TOGGLE: &str = "toggle-pointer";
const CLEAR: &str = "clear-drawing";
const EXIT: &str = "exit-pointer";

// Qt's key codes, as KGlobalAccel stores them.
const CTRL: i32 = 0x0400_0000;
const ALT: i32 = 0x0800_0000;
const SHIFT: i32 = 0x0200_0000;
const META: i32 = 0x1000_0000;
const ESCAPE: i32 = 0x0100_0000;
pub const TOGGLE_DEFAULT: i32 = CTRL | ALT | 0x48; // H
pub const CLEAR_DEFAULT: i32 = CTRL | ALT | 0x58; // X

fn action_id(action: &str) -> Vec<String> {
    let friendly = match action {
        TOGGLE => "Turn the pointer on or off",
        CLEAR => "Clear the drawing",
        _ => "Turn the pointer off (only while it is on)",
    };
    vec![COMPONENT.into(), action.into(), "Waypointer".into(), friendly.into()]
}

/// A key as readable text: Ctrl+Alt+H.
pub fn key_text(key: i32) -> String {
    if key == 0 {
        return String::new();
    }
    let mut parts = Vec::new();
    // Named as the keyboard says it in the language in use, like "Strg" in German.
    for (bit, name) in [(META, "Meta"), (CTRL, tr("Ctrl")), (ALT, "Alt"), (SHIFT, tr("Shift"))] {
        if key & bit != 0 {
            parts.push(name.to_string());
        }
    }
    let code = key & 0x01FF_FFFF;
    parts.push(match code {
        ESCAPE => "Esc".into(),
        0x0100_0001 => "Tab".into(),
        0x0100_0004 | 0x0100_0005 => "Enter".into(),
        0x20 => "Space".into(),
        0x0100_0030..=0x0100_004F => format!("F{}", code - 0x0100_0030 + 1),
        c if (0x21..0x7F).contains(&c) => char::from_u32(c as u32).unwrap_or('?').to_string(),
        c => format!("0x{c:x}"),
    });
    parts.join("+")
}

pub struct Shortcuts {
    pub available: bool,
    pub toggle: i32,
    pub clear: i32,
}

pub static SHORTCUTS: Mutex<Shortcuts> = Mutex::new(Shortcuts { available: false, toggle: 0, clear: 0 });

fn kglobalaccel(conn: &Connection) -> zbus::Result<Proxy<'static>> {
    Proxy::new(conn, "org.kde.kglobalaccel", "/kglobalaccel", "org.kde.KGlobalAccel")
}

/// Registers an action; returns the key it ends up with (the user's own, if they changed it).
fn register(conn: &Connection, action: &str, default: i32) -> zbus::Result<i32> {
    let p = kglobalaccel(conn)?;
    let id = action_id(action);
    p.call::<_, _, ()>("doRegister", &(id.clone(),))?;
    p.call::<_, _, Vec<i32>>("setShortcut", &(id.clone(), vec![default], 1u32))?;
    let keys: Vec<i32> = p.call("setShortcut", &(id, vec![default], 2u32))?;
    Ok(keys.first().copied().unwrap_or(0))
}

/// Gives an action a new key. Returns why not, when it can't.
pub fn assign(action: &str, key: i32) -> Result<(), String> {
    let conn = Connection::session().map_err(|e| e.to_string())?;
    let p = kglobalaccel(&conn).map_err(|e| e.to_string())?;
    let free: bool = p.call("isGlobalShortcutAvailable", &(key, COMPONENT)).map_err(|e| e.to_string())?;
    let current = { let s = SHORTCUTS.lock().unwrap(); if action == TOGGLE { s.toggle } else { s.clear } };
    let other = { let s = SHORTCUTS.lock().unwrap(); if action == TOGGLE { s.clear } else { s.toggle } };
    if key == other {
        return Err(crate::i18n::trf("{} is already used by Waypointer.", &[&key_text(key)]));
    }
    if !free && key != current {
        return Err(crate::i18n::trf("{} is already in use.", &[&key_text(key)]));
    }
    let keys: Vec<i32> = p.call("setShortcut", &(action_id(action), vec![key], 6u32)).map_err(|e| e.to_string())?;
    let got = keys.first().copied().unwrap_or(0);
    let mut s = SHORTCUTS.lock().unwrap();
    if action == TOGGLE { s.toggle = got } else { s.clear = got }
    Ok(())
}

pub fn set_toggle(key: i32) -> Result<(), String> {
    assign(TOGGLE, key)
}

pub fn set_clear(key: i32) -> Result<(), String> {
    assign(CLEAR, key)
}

/// Stops all global shortcuts while the user records a new one.
pub fn block_shortcuts(on: bool) {
    if let Ok(conn) = Connection::session()
        && let Ok(p) = kglobalaccel(&conn)
    {
        let _ = p.call::<_, _, ()>("blockGlobalShortcuts", &(on,));
    }
}

fn has_name(conn: &Connection, name: &str) -> bool {
    let Ok(name) = zbus::names::BusName::try_from(name) else { return false };
    zbus::blocking::fdo::DBusProxy::new(conn).ok().and_then(|p| p.name_has_owner(name).ok()).unwrap_or(false)
}

fn wayland() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some()
}

/// Starts the desktop side and hands it to the hub.
pub fn start(hub: &Arc<Hub>, tray: Option<crate::tray::TrayHandle>) {
    let (jobs, rx) = channel::<Job>();
    let Ok(conn) = Connection::session() else {
        eprintln!("waypointer: no D-Bus session");
        return;
    };
    let kwin = wayland() && has_name(&conn, "org.kde.KWin");
    let kde_keys = has_name(&conn, "org.kde.kglobalaccel");
    hub.set_services(Box::new(Desktop { jobs, can_track: kwin }));

    if kde_keys {
        let toggle = register(&conn, TOGGLE, TOGGLE_DEFAULT).unwrap_or(TOGGLE_DEFAULT);
        let clear = register(&conn, CLEAR, CLEAR_DEFAULT).unwrap_or(CLEAR_DEFAULT);
        *SHORTCUTS.lock().unwrap() = Shortcuts { available: true, toggle, clear };
        listen_shortcuts(hub.clone());
    }
    watch_session(hub.clone());

    let hub = hub.clone();
    std::thread::Builder::new()
        .name("waypointer desktop".into())
        .spawn(move || {
            let mut esc = false;
            for job in rx {
                match job {
                    Job::Esc(on) if kde_keys && on != esc => {
                        esc = on;
                        let _ = if on { register_esc(&conn) } else { kglobalaccel(&conn).and_then(|p| p.call::<_, _, ()>("setInactive", &(action_id(EXIT),))) };
                    }
                    Job::Esc(_) => {}
                    Job::Tracking(on) if kwin => {
                        if on {
                            let _ = load_script(&conn, "waypointer-track", TRACK_SCRIPT);
                        } else {
                            unload_script(&conn, "waypointer-track");
                        }
                    }
                    Job::Tracking(_) => {}
                    Job::Areas if kwin => {
                        if load_script(&conn, "waypointer-areas", AREAS_SCRIPT).is_err() {
                            hub.areas(HashMap::new(), None);
                        }
                    }
                    Job::Areas => hub.areas(HashMap::new(), None),
                    Job::Tray => {
                        if let Some(tray) = &tray {
                            tray.refresh();
                        }
                        write_state(&hub);
                    }
                }
            }
        })
        .ok();
}

fn register_esc(conn: &Connection) -> zbus::Result<()> {
    let p = kglobalaccel(conn)?;
    let id = action_id(EXIT);
    p.call::<_, _, ()>("doRegister", &(id.clone(),))?;
    p.call::<_, _, Vec<i32>>("setShortcut", &(id.clone(), vec![ESCAPE], 1u32))?;
    p.call::<_, _, Vec<i32>>("setShortcut", &(id, vec![ESCAPE], 6u32))?;
    Ok(())
}

fn listen_shortcuts(hub: Arc<Hub>) {
    std::thread::spawn(move || {
        let Ok(conn) = Connection::session() else { return };
        let Ok(rule) = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .interface("org.kde.kglobalaccel.Component")
            .and_then(|r| r.member("globalShortcutPressed"))
            .map(|r| r.build())
        else {
            return;
        };
        let Ok(messages) = MessageIterator::for_match_rule(rule, &conn, None) else { return };
        for msg in messages.flatten() {
            let Ok((component, action, _time)) = msg.body().deserialize::<(String, String, i64)>() else { continue };
            if component != COMPONENT {
                continue;
            }
            match action.as_str() {
                TOGGLE => hub.toggle(),
                CLEAR => hub.clear(),
                EXIT => hub.exit(),
                _ => {}
            }
        }
    });
}

fn watch_session(hub: Arc<Hub>) {
    let h = hub.clone();
    std::thread::spawn(move || {
        let Ok(conn) = Connection::session() else { return };
        let Ok(rule) = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .interface("org.freedesktop.ScreenSaver")
            .and_then(|r| r.member("ActiveChanged"))
            .map(|r| r.build())
        else {
            return;
        };
        let Ok(messages) = MessageIterator::for_match_rule(rule, &conn, None) else { return };
        for msg in messages.flatten() {
            if msg.body().deserialize::<bool>().unwrap_or(false) {
                h.deactivate("screen locked");
            }
        }
    });
    std::thread::spawn(move || {
        let Ok(conn) = Connection::system() else { return };
        let Ok(rule) = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .interface("org.freedesktop.login1.Manager")
            .and_then(|r| r.member("PrepareForSleep"))
            .map(|r| r.build())
        else {
            return;
        };
        let Ok(messages) = MessageIterator::for_match_rule(rule, &conn, None) else { return };
        for msg in messages.flatten() {
            if msg.body().deserialize::<bool>().unwrap_or(false) {
                hub.deactivate("going to sleep");
            }
        }
    });
}

// KWin scripts call back with numbers as text, see KWinHelper.
const SCRIPT_HEAD: &str = "var S = 'de.felitendo.Waypointer', P = '/KWinHelper', I = 'de.felitendo.Waypointer.KWinHelper';\n\
                           function point(p) { return p.x + ',' + p.y; }\n";
const AREAS_SCRIPT: &str = "var screens = workspace.screens;\n\
    for (var i = 0; i < screens.length; i++) {\n\
        var a = workspace.clientArea(KWin.PlacementArea, screens[i], workspace.currentDesktop);\n\
        callDBus(S, P, I, 'Area', screens[i].name, a.x + ',' + a.y + ',' + a.width + ',' + a.height);\n\
    }\n\
    callDBus(S, P, I, 'Cursor', point(workspace.cursorPos));\n\
    callDBus(S, P, I, 'Done');\n";
const TRACK_SCRIPT: &str = "callDBus(S, P, I, 'Cursor', point(workspace.cursorPos));\n\
    workspace.cursorPosChanged.connect(function () { callDBus(S, P, I, 'Cursor', point(workspace.cursorPos)); });\n";

fn runtime_dir() -> std::path::PathBuf {
    dirs::runtime_dir().unwrap_or_else(std::env::temp_dir).join("waypointer")
}

fn kwin_scripting(conn: &Connection) -> zbus::Result<Proxy<'static>> {
    Proxy::new(conn, "org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting")
}

fn unload_script(conn: &Connection, name: &str) {
    if let Ok(p) = kwin_scripting(conn) {
        let _ = p.call::<_, _, bool>("unloadScript", &(name,));
    }
}

fn load_script(conn: &Connection, name: &str, body: &str) -> zbus::Result<()> {
    let dir = runtime_dir();
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(format!("{name}.js"));
    std::fs::write(&path, format!("{SCRIPT_HEAD}{body}")).map_err(|e| zbus::Error::Failure(e.to_string()))?;
    unload_script(conn, name);
    let id: i32 = kwin_scripting(conn)?.call("loadScript", &(path.display().to_string(), name))?;
    if id < 0 {
        return Err(zbus::Error::Failure("KWin did not load the script".into()));
    }
    let script = Proxy::new(conn, "org.kde.KWin", format!("/Scripting/Script{id}"), "org.kde.kwin.Script")?;
    script.call::<_, _, ()>("run", &())?;
    Ok(())
}

pub fn state_file() -> std::path::PathBuf {
    runtime_dir().join("state.json")
}

fn write_state(hub: &Hub) {
    let _ = std::fs::create_dir_all(runtime_dir());
    let tmp = runtime_dir().join("state.json.tmp");
    if std::fs::write(&tmp, hub.state_json()).is_ok() {
        let _ = std::fs::rename(tmp, state_file());
    }
}

/// When Waypointer quits: no script left in KWin, no state file left behind.
pub fn clean_up() {
    if let Ok(conn) = Connection::session() {
        unload_script(&conn, "waypointer-areas");
        unload_script(&conn, "waypointer-track");
        if let Ok(p) = kglobalaccel(&conn) {
            let _ = p.call::<_, _, ()>("setInactive", &(action_id(EXIT),));
        }
    }
    let _ = std::fs::remove_file(state_file());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_become_commands() {
        assert_eq!(from_url("waypointer://style/robot"), vec!["style", "robot"]);
        assert_eq!(from_url("waypointer://toggle"), vec!["toggle"]);
    }

    #[test]
    fn keys_read_like_keys() {
        assert_eq!(key_text(TOGGLE_DEFAULT), "Ctrl+Alt+H");
        assert_eq!(key_text(ESCAPE), "Esc");
    }
}
