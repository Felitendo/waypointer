//! Alt, Ctrl, Shift and Meta going down or up anywhere, also while another app has the
//! keyboard. KWin tells this through its keystate protocol (version 5 and up), the same way
//! KDE's own sticky keys notice does. Elsewhere holding a key to click for real can't work.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use wayland_client::protocol::wl_registry::{self, WlRegistry};
use wayland_client::{Connection, Dispatch, QueueHandle};

use crate::hub::{Hub, Modifier};

#[allow(dead_code, non_upper_case_globals, non_camel_case_types, clippy::all)]
mod protocol {
    use wayland_client;
    #[allow(unused_imports)]
    use wayland_client::protocol::*;
    pub mod __interfaces {
        #[allow(unused_imports)]
        use wayland_client::protocol::__interfaces::*;
        wayland_scanner::generate_interfaces!("protocols/keystate.xml");
    }
    use self::__interfaces::*;
    wayland_scanner::generate_client_code!("protocols/keystate.xml");
}

use protocol::org_kde_kwin_keystate::{self, OrgKdeKwinKeystate};

struct State {
    hub: Arc<Hub>,
    keystate: Option<OrgKdeKwinKeystate>,
    pressed: [bool; 8],
}

impl Dispatch<WlRegistry, ()> for State {
    fn event(state: &mut Self, registry: &WlRegistry, event: wl_registry::Event, _: &(), _: &Connection, qh: &QueueHandle<Self>) {
        if let wl_registry::Event::Global { name, interface, version } = event
            && interface == "org_kde_kwin_keystate"
            && version >= 5
        {
            let keystate: OrgKdeKwinKeystate = registry.bind(name, 5, qh, ());
            keystate.fetchStates();
            state.keystate = Some(keystate);
            state.hub.can_hold.store(true, Ordering::Relaxed);
        }
    }
}

impl Dispatch<OrgKdeKwinKeystate, ()> for State {
    fn event(state: &mut Self, _: &OrgKdeKwinKeystate, event: org_kde_kwin_keystate::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        #[allow(irrefutable_let_patterns)]
        let org_kde_kwin_keystate::Event::StateChanged { key, state: key_state } = event else { return };
        let modifier = match key {
            3 => Modifier::Alt,
            4 => Modifier::Ctrl,
            5 => Modifier::Shift,
            6 => Modifier::Meta,
            _ => return,
        };
        // 3 is "pressed"; latched and locked (sticky keys) don't count as held.
        let pressed = key_state == 3;
        let slot = key as usize;
        if state.pressed[slot] != pressed {
            state.pressed[slot] = pressed;
            state.hub.modifier(modifier, pressed);
        }
    }
}

/// Listens on a thread of its own. Does nothing outside Wayland or without the protocol.
pub fn start(hub: Arc<Hub>) {
    std::thread::Builder::new()
        .name("waypointer keys".into())
        .spawn(move || {
            let Ok(conn) = Connection::connect_to_env() else { return };
            let mut queue = conn.new_event_queue();
            let qh = queue.handle();
            conn.display().get_registry(&qh, ());
            let mut state = State { hub, keystate: None, pressed: [false; 8] };
            if queue.roundtrip(&mut state).is_err() || state.keystate.is_none() {
                return;
            }
            while queue.blocking_dispatch(&mut state).is_ok() {}
        })
        .ok();
}
