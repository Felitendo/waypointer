//! The settings window, laid out like TinyHand's on the Mac: a sidebar with coloured icon
//! tiles, and cards of settings that change at once. Every setting lives here; nobody has to
//! open the settings file.

use std::sync::Arc;

use rune::tok::{self, Theme, motion, radius, space, type_};
use rune::{AppSpec, Color, Cx, Drag, El, FileDialogOptions, Icon, Phase, Pixels, ScrollSpec, Spring, SpringPair, TextEdit, Transition, Tx, col, el, icon, row, text, text_input};

use crate::hands;
use crate::hub::Hub;
use crate::i18n::{tr, trf};
use crate::icons;
use crate::platform;
use crate::settings::{self, FADES, INKS, InteractKey, TAP_SOUNDS};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    General,
    Pointers,
    Appearance,
    Drawing,
    Sound,
    About,
    /// Reached from Pointers, not in the sidebar.
    MakeHand,
}

pub struct Draft {
    image: image::RgbaImage,
    ink: u32,
    preview: Pixels,
}

static PAGES: [(Page, &str, &Icon); 6] = [
    (Page::General, n!("General"), &icons::GEAR),
    (Page::Pointers, n!("Pointers"), &icons::POINTER),
    (Page::Appearance, n!("Appearance"), &icons::SPARKLE),
    (Page::Drawing, n!("Drawing"), &icons::PENCIL),
    (Page::Sound, n!("Sound"), &icons::SPEAKER_LINE),
    (Page::About, n!("About"), &icons::INFO),
];

/// Sidebar rows, as in Hex: 30 pt tall with a 4 pt gap.
const NAV_ROW: f32 = 30.0;
const NAV_GAP: f32 = 4.0;
const NAV_WIDTH: f32 = 220.0;

pub struct Ui {
    pub hub: Arc<Hub>,
    pub page: Page,
    recording: Option<&'static str>,
    shortcut_error: Option<String>,
    confirm_reset: bool,
    pin_message: Option<String>,
    /// A pointer being dragged into, within or out of the toolbar row.
    pin_drag: Option<PinDrag>,
    /// Where each pointer in the toolbar row sits, from layout, by place.
    pin_slots: Vec<[f32; 4]>,
    /// Where the toolbar row is, from layout.
    pin_area: Option<[f32; 4]>,
    /// The last press on a pointer was a drag, so it takes nothing out.
    pin_dragged: bool,
    idea: TextEdit,
    hand_name: TextEdit,
    draft: Option<Draft>,
    draft_error: Option<String>,
    hand_message: Option<String>,
    copied: bool,
}

impl Ui {
    pub fn new(hub: Arc<Hub>) -> Ui {
        Ui {
            hub,
            page: Page::General,
            recording: None,
            shortcut_error: None,
            confirm_reset: false,
            pin_message: None,
            pin_drag: None,
            pin_slots: Vec::new(),
            pin_area: None,
            pin_dragged: false,
            idea: TextEdit::new(""),
            hand_name: TextEdit::new(""),
            draft: None,
            draft_error: None,
            hand_message: None,
            copied: false,
        }
    }
}

pub fn spec() -> AppSpec<Ui> {
    AppSpec {
        scenes: &[
            ("general", |s| s.page = Page::General),
            ("pointers", |s| s.page = Page::Pointers),
            ("appearance", |s| s.page = Page::Appearance),
            ("drawing", |s| s.page = Page::Drawing),
            ("sound", |s| s.page = Page::Sound),
            ("about", |s| s.page = Page::About),
            ("make-hand", |s| s.page = Page::MakeHand),
        ],
        on_key: Some(on_key),
        ..AppSpec::new("Waypointer", (860.0, 600.0), || Ui::new(Hub::new(crate::settings::Settings::default())), view)
    }
}

/// Everything a component needs to paint itself.
#[derive(Clone, Copy)]
struct Paint {
    t: Theme,
    accent: Color,
    hair: f32,
    dark: bool,
}

fn view(s: &Ui, cx: &mut Cx<Ui>) -> El {
    let dark = cx.system_dark();
    let p = Paint { t: tok::theme(dark), accent: tok::accent(0, dark), hair: 1.0 / cx.env().scale, dark };
    let st = s.hub.settings();
    let page = match s.page {
        Page::General => general(p, s, &st),
        Page::Pointers => pointers(p, s, &st),
        Page::Appearance => appearance(p, s, &st),
        Page::Drawing => drawing(p, &st),
        Page::Sound => sound(p, &st),
        Page::About => about(p),
        Page::MakeHand => make_hand(p, s),
    };
    row().key("window").w_full().h_full().fill(p.t.bg)
        .child(sidebar(p, s.page))
        .child(el().key("divider").w(p.hair).h_full().fill(p.t.separator))
        .child(
            col().key("content").grow(1.0).h_full().min_h(0.0).scroll(ScrollSpec { stick_bottom: false })
                // The old page goes at once and the new one comes up quickly, so the two never
                // show over each other.
                .child(page.key(format!("page-{:?}", s.page)).transition(
                    Transition::asymmetric(Transition::offset(0.0, 6.0).combined(Transition::opacity()), Transition::none())
                        // asymmetric() keeps the insertion's springs for both ways: give the
                        // removal one of its own that lands at once.
                        .springs(SpringPair::new(Spring::new(0.28, 0.0), Spring::NONE)),
                )),
        )
}

fn select(s: &mut Ui, page: Page) {
    s.page = page;
    s.confirm_reset = false;
    s.pin_message = None;
}

fn sidebar(p: Paint, current: Page) -> El {
    let current = if current == Page::MakeHand { Page::Pointers } else { current };
    let index = PAGES.iter().position(|e| e.0 == current).unwrap_or(0);
    let inner = NAV_WIDTH - 20.0;
    // One highlight under the rows that springs to the chosen one. The rows themselves never
    // change shape, so nothing in them jumps.
    let mut nav = col().key("nav").role("list").gap(NAV_GAP)
        .child(el().key("nav-highlight").size(inner, NAV_ROW).absolute(0.0, index as f32 * (NAV_ROW + NAV_GAP))
            .radius(7.0).fill(if p.dark { Color::WHITE.alpha(0.075) } else { Color::BLACK.alpha(0.06) })
            .motion(motion::SNAPPY).a11y_hidden());
    for &(page, title, glyph) in PAGES.iter() {
        let on = page == current;
        nav = nav.child(
            row().key(format!("nav-{title}")).role("tab").label(tr(title)).checked(on).focusable(true)
                .w(inner).h(NAV_ROW).px(8.0).gap(10.0).center_y().radius(7.0)
                .hover(if on { Color::TRANSPARENT } else { p.t.hover }).press(p.t.press)
                // Like a native sidebar: the page changes on the press, not on the release.
                .on_drag(move |s: &mut Ui, d: &mut Drag, _| if d.phase == Phase::Down { select(s, page) })
                .on_press(move |s: &mut Ui| select(s, page))
                .child(icon(glyph, 16.0).key("icon").color(if on { p.accent } else { p.t.text_secondary }).a11y_hidden())
                .child(text(tr(title)).key("label").font(rune::tok::TextStyle::new(13.0, if on { 600.0 } else { 500.0 }))
                    .color(if on { p.t.text } else { p.t.text_secondary }).a11y_hidden()),
        );
    }
    col().key("sidebar").role("navigation").w(NAV_WIDTH).h_full().pt(12.0).pb(12.0).px(10.0)
        .fill(if p.dark { Color::BLACK.alpha(0.12) } else { Color::BLACK.alpha(0.025) })
        .child(row().key("brand").h(44.0).gap(10.0).center_y().px(4.0).mb(10.0)
            .child(app_mark(28.0))
            .child(text("Waypointer").font(rune::tok::TextStyle::new(14.0, 650.0)).color(p.t.text)))
        .child(nav)
        .child(el().key("spacer").grow(1.0))
        .child(text(trf("Version {}", &[&env!("CARGO_PKG_VERSION")])).key("version").font(rune::tok::TextStyle::new(11.5, 400.0))
            .color(p.t.text_secondary.alpha(0.7)).px(8.0))
}

fn mark_pixels() -> &'static Pixels {
    static MARK: std::sync::OnceLock<Pixels> = std::sync::OnceLock::new();
    MARK.get_or_init(|| {
        let pixels = Pixels::new();
        if let Ok(img) = image::load_from_memory(include_bytes!("../res/icons/hicolor/256x256/apps/de.felitendo.waypointer.png")) {
            let img = img.to_rgba8();
            pixels.set(img.width(), img.height(), img.into_raw());
        }
        pixels
    })
}

/// The app icon.
pub fn app_mark(size: f32) -> El {
    el().size(size, size).pixels(mark_pixels())
}

fn page(p: Paint, title: &str) -> El {
    col().px(space::S6 + 4.0).pt(space::S6).pb(space::S6 + 8.0).gap(space::S6)
        .child(text(title).font(rune::tok::TextStyle::new(22.0, 700.0)).color(p.t.text))
}

fn card<const N: usize>(p: Paint, key: &str, title: Option<&str>, rows: [El; N]) -> El {
    let mut c = col().key("card").padding(space::S1).fill(p.t.card).radius(radius::LG)
        .border(p.hair, p.t.card_border).shadow(1.0, 2.0, p.t.shadow).shadow(8.0, 24.0, p.t.shadow_soft);
    for (i, r) in rows.into_iter().enumerate() {
        if i > 0 {
            c = c.child(el().h(p.hair).ml(space::S3).mr(space::S3).fill(p.t.separator).a11y_hidden());
        }
        c = c.child(r);
    }
    let mut section = col().key(key).gap(space::S2);
    if let Some(title) = title {
        section = section.child(text(title).font(type_::SECTION).color(p.t.text_secondary).ml(space::S3));
    }
    section.child(c)
}

fn setting(p: Paint, key: &str, label: &str, detail: Option<&str>, control: El) -> El {
    let mut words = col().gap(2.0).grow(1.0).min_w(0.0).child(text(label).font(type_::BODY).color(p.t.text));
    if let Some(detail) = detail {
        words = words.child(text(detail).font(rune::tok::TextStyle::new(11.5, 400.0)).color(p.t.text_secondary).wrap());
    }
    row().key(key).min_h(46.0).px(space::S3).py(8.0).gap(16.0).center_y().justify_between().label(label)
        .child(words)
        .child(control)
}

fn toggle_row(p: Paint, key: &str, label: &str, detail: Option<&str>, on: bool, enabled: bool, flip: fn(&mut Ui)) -> El {
    let mut r = setting(p, key, label, detail, switch(p, on, enabled).a11y_hidden()).role("switch").checked(on).radius(radius::MD);
    if enabled {
        r = r.hover(p.t.hover).press(p.t.press).focusable(true).on_press(flip);
    } else {
        r = r.opacity(0.5);
    }
    r
}

fn switch(p: Paint, on: bool, enabled: bool) -> El {
    let (w, h, k) = (40.0, 24.0, 20.0);
    let inset = (h - k) / 2.0;
    el().key("switch").size(w, h).radius(h / 2.0)
        .fill(if on && enabled { p.accent } else { p.t.control_off })
        .motion(Spring::BOUNCY)
        .child(el().key("knob").size(k, k).absolute(if on { w - k - inset } else { inset }, inset).radius(k / 2.0)
            .fill(p.t.knob).shadow(1.0, 3.0, p.t.knob_shadow).shadow(0.0, 0.5, p.t.edge))
}

/// A segmented control whose indicator springs to the chosen segment.
fn segmented(p: Paint, key: &str, labels: &[&str], selected: usize, width: f32, enabled: bool, pick: fn(&mut Ui, usize)) -> El {
    let (h, pad) = (28.0, 2.0);
    // `width` at least; wider when a label needs it, as some languages do.
    let widest = labels.iter().map(|l| rune::text::measure(l, &[], type_::LABEL, None).0).fold(0.0, f32::max);
    let seg = ((width - 2.0 * pad) / labels.len() as f32).max(widest + 24.0);
    let width = seg * labels.len() as f32 + 2.0 * pad;
    let mut segs = row().key("segments");
    for (i, name) in labels.iter().enumerate() {
        let is = i == selected;
        let mut s = el().key(format!("{key}-{i}")).role("segment").label(*name).checked(is).focusable(enabled)
            .size(seg, h - 2.0 * pad).center().radius(radius::SM)
            .child(text(*name).font(type_::LABEL).color(if is { p.t.text } else { p.t.text_secondary }).a11y_hidden());
        if enabled {
            s = s.hover(if is { Color::TRANSPARENT } else { p.t.hover }).on_press_tx(move |st: &mut Ui, tx: &mut Tx| {
                pick(st, i);
                tx.color_motion(motion::CALM);
            });
        }
        segs = segs.child(s);
    }
    let mut c = el().key(key).role("segmented").size(width, h).padding(pad).radius(radius::MD).fill(p.t.control_track)
        .motion(motion::SNAPPY)
        .child(el().key("indicator").size(seg, h - 2.0 * pad).absolute(pad + selected as f32 * seg, pad).radius(radius::SM)
            .fill(p.t.indicator).shadow(1.0, 3.0, p.t.shadow).shadow(0.0, 0.5, p.t.edge)
            .lead(motion::LEAD, motion::DEFAULT).a11y_hidden())
        .child(segs);
    if !enabled {
        c = c.opacity(0.5);
    }
    c
}

fn button(p: Paint, key: &str, label: &str, primary: bool) -> El {
    let fill = if primary { p.accent } else { p.t.control_off };
    el().key(key).role("button").label(label).focusable(true).h(30.0).px(14.0).center().radius(radius::MD)
        .fill(fill).hover(Color::WHITE.alpha(0.06)).press(Color::BLACK.alpha(0.08)).press_scale(0.97)
        .child(text(label).font(type_::LABEL).color(if primary { Color::WHITE } else { p.t.text }).a11y_hidden())
}

// --- General --------------------------------------------------------------------------------

fn shortcut_button(p: Paint, key: &'static str, current: i32, recording: bool) -> El {
    let label = if recording { tr("Press keys…").to_string() } else if current == 0 { tr("None").into() } else { platform::key_text(current) };
    let mut b = el().key(key).role("button").label(if key == "toggle-shortcut" { tr("Shortcut to turn the pointer on or off") } else { tr("Shortcut to clear the drawing") })
        .focusable(true).h(30.0).min_w(130.0).px(12.0).center().radius(radius::MD)
        .fill(if recording { p.accent.alpha(0.18) } else { p.t.control_off })
        .border(if recording { 1.5 } else { 0.0 }, p.accent)
        .hover(p.t.hover).motion(motion::SNAPPY)
        .child(text(label).font(rune::tok::TextStyle::new(12.5, 600.0)).color(p.t.text).a11y_hidden());
    b = b.on_press(move |s: &mut Ui| {
        let start = s.recording != Some(key);
        s.recording = if start { Some(key) } else { None };
        s.shortcut_error = None;
        platform::block_shortcuts(start);
    });
    b
}

fn on_key(s: &mut Ui, event: &rune::KeyEvent, _tx: &mut Tx) -> bool {
    let Some(which) = s.recording else { return false };
    if event.key == rune::Key::Escape && event.mods == rune::Mods::default() {
        s.recording = None;
        platform::block_shortcuts(false);
        return true;
    }
    let Some(code) = qt_key(&event.key) else { return true };
    let mods = event.mods;
    if !(mods.ctrl || mods.alt || mods.logo) {
        s.shortcut_error = Some(tr("Use Ctrl, Alt or Meta with the key.").into());
        return true;
    }
    let mut key = code;
    if mods.ctrl { key |= 0x0400_0000; }
    if mods.alt { key |= 0x0800_0000; }
    if mods.shift { key |= 0x0200_0000; }
    if mods.logo { key |= 0x1000_0000; }
    s.recording = None;
    platform::block_shortcuts(false);
    let result = if which == "toggle-shortcut" { platform::set_toggle(key) } else { platform::set_clear(key) };
    s.shortcut_error = result.err();
    s.hub.refresh_tray();
    true
}

/// A key as Qt numbers it, which is how KGlobalAccel stores shortcuts.
fn qt_key(key: &rune::Key) -> Option<i32> {
    use rune::Key;
    Some(match key {
        Key::Command(c) => {
            let c = c.to_ascii_uppercase();
            if c == ' ' { 0x20 } else if c.is_ascii_graphic() { c as i32 } else { return None }
        }
        Key::Text(t) => {
            let c = t.chars().next()?.to_ascii_uppercase();
            if c == ' ' { 0x20 } else if c.is_ascii_graphic() { c as i32 } else { return None }
        }
        Key::Tab => 0x0100_0001,
        Key::Enter => 0x0100_0004,
        Key::Function(n) => 0x0100_0030 + (*n as i32) - 1,
        _ => return None,
    })
}

fn general(p: Paint, s: &Ui, st: &settings::Settings) -> El {
    let shortcuts = { let k = platform::SHORTCUTS.lock().unwrap(); (k.available, k.toggle, k.clear) };
    let can_hold = s.hub.can_hold.load(std::sync::atomic::Ordering::Relaxed);
    let can_track = s.hub.can_track();
    let mut g = page(p, tr("General"));

    let shortcut_card = if shortcuts.0 {
        let mut c = card(p, "shortcuts", Some(tr("Shortcuts")), [
            setting(p, "toggle-row", tr("Turn the pointer on or off"), None, shortcut_button(p, "toggle-shortcut", shortcuts.1, s.recording == Some("toggle-shortcut"))),
            setting(p, "clear-row", tr("Clear the drawing"), None, shortcut_button(p, "clear-shortcut", shortcuts.2, s.recording == Some("clear-shortcut"))),
        ]);
        if let Some(e) = &s.shortcut_error {
            c = c.child(text(e.as_str()).font(type_::LABEL).color(Color { r: 0.95, g: 0.35, b: 0.3, a: 1.0 }).ml(space::S3));
        }
        c
    } else {
        card(p, "shortcuts", Some(tr("Shortcuts")), [setting(p, "no-shortcuts", tr("Global shortcuts need KDE Plasma"),
            Some(tr("Bind `waypointer toggle` and `waypointer clear` to keys in your desktop's settings.")), el())])
    };
    g = g.child(shortcut_card);

    let key_index = match st.interact_key { InteractKey::Alt => 0, InteractKey::Ctrl => 1, InteractKey::Off => 2 };
    let mut clicking = col().gap(0.0);
    clicking = clicking.child(card(p, "clicking", Some(tr("Clicking")), [
        setting(p, "interact", tr("Click for real while holding"), Some(tr("The hand steps aside and the click reaches the app.")),
            segmented(p, "interact-key", &[tr("Alt"), tr("Ctrl"), tr("Off")], key_index, 180.0, can_hold, |s, i| {
                s.hub.change(|st| st.interact_key = [InteractKey::Alt, InteractKey::Ctrl, InteractKey::Off][i]);
            })),
        setting(p, "keep", tr("While clicking for real"), None,
            segmented(p, "keep-hand", &[tr("Show cursor"), tr("Keep hand")], usize::from(st.interact_keeps_hand), 220.0, can_hold && can_track && st.interact_key != InteractKey::Off, |s, i| {
                s.hub.change(|st| st.interact_keeps_hand = i == 1);
            })),
        toggle_row(p, "double-shift", tr("Double-tap Shift to click through"), Some(tr("Clicks reach the apps and the hand stays, until you double-tap again.")),
            st.double_shift, can_hold, |s| s.hub.change(|st| st.double_shift = !st.double_shift)),
    ]));
    if !can_hold {
        clicking = clicking.child(text(tr("Your desktop doesn't tell apps when Alt or Shift is held, so these need KDE Plasma."))
            .font(type_::LABEL).color(p.t.text_secondary).ml(space::S3).mt(space::S2).wrap());
    }
    g = g.child(clicking);

    // The language: the system's, or one of the others, each in its own name.
    let mut languages = row().key("languages").gap(6.0).padding(space::S3).flex_wrap();
    let choices = std::iter::once(("system", tr("System"))).chain(crate::i18n::LANGUAGES.iter().copied());
    for (code, name) in choices {
        let on = st.language == code;
        languages = languages.child(
            el().key(format!("language-{code}")).role("radio").label(name).checked(on).focusable(true)
                .h(30.0).px(12.0).center().radius(15.0)
                .fill(if on { p.accent } else { p.t.control_track }).hover(p.t.hover).press_scale(0.95).motion(motion::SNAPPY)
                .on_press(move |s: &mut Ui| s.hub.change(|st| st.language = code.to_string()))
                .child(text(name).font(rune::tok::TextStyle::new(12.5, if on { 600.0 } else { 500.0 }))
                    .color(if on { Color::WHITE } else { p.t.text }).a11y_hidden()),
        );
    }
    g = g.child(col().key("language-section").gap(space::S2)
        .child(text(tr("Language")).font(type_::SECTION).color(p.t.text_secondary).ml(space::S3))
        .child(col().fill(p.t.card).radius(radius::LG).border(p.hair, p.t.card_border).shadow(1.0, 2.0, p.t.shadow).child(languages)));

    let login = settings::launch_at_login();
    g.child(card(p, "app", Some(tr("App")), [
        toggle_row(p, "login", tr("Open at login"), None, login, true, |_| settings::set_launch_at_login(!settings::launch_at_login())),
        setting(p, "reset-row", tr("Reset all settings"), Some(tr("Shortcuts, toolbar, size, drawing and sound go back to how they were.")),
            button(p, "reset", if s.confirm_reset { tr("Click again to reset") } else { tr("Reset…") }, s.confirm_reset)
                .on_press(|s: &mut Ui| {
                    if s.confirm_reset {
                        s.hub.change(|st| st.reset());
                        let _ = platform::set_toggle(platform::TOGGLE_DEFAULT);
                        let _ = platform::set_clear(platform::CLEAR_DEFAULT);
                        s.confirm_reset = false;
                    } else {
                        s.confirm_reset = true;
                    }
                })),
    ]))
}

// --- Pointers -------------------------------------------------------------------------------

fn thumb(s: &Ui, id: &str, size: f32) -> El {
    if id == hands::LASER {
        let red = hands::rgb(hands::RED);
        return el().size(size, size).center().child(el().size(size * 0.3, size * 0.3).radius(size * 0.15).fill(red).shadow(0.0, size * 0.3, red.alpha(0.7)));
    }
    match s.hub.art.thumb(id) {
        Some(pixels) => el().size(size, size).pixels(&pixels),
        None => el().size(size, size),
    }
}

/// Points the pointer travels before a press on a pointer becomes a drag.
const PIN_SLOP: f32 = 4.0;
/// How far past the toolbar row a pointer still drops into it.
const PIN_REACH: f32 = 16.0;

/// A pointer being dragged: within the toolbar row to a new place, out of it, or from all
/// pointers into it.
#[derive(Clone, Debug)]
struct PinDrag {
    id: String,
    /// Its place in the toolbar row, if it has one.
    from: Option<usize>,
    /// It was picked up in the row: dropped off it, it goes out of the toolbar.
    out: bool,
    /// Where it lands in the row if dropped now; `None` off the row.
    to: Option<usize>,
    /// How far the pointer went since the press.
    d: [f32; 2],
    /// Past the slop: a drag, not a click.
    moving: bool,
}

impl PinDrag {
    /// Where the pointer at place `i` in the row sits while this drag goes on: the others make
    /// room where it would land, and close up where it left.
    fn place(&self, i: usize) -> usize {
        match (self.from, self.to) {
            (Some(from), Some(to)) if i == from => to,
            (Some(from), Some(to)) if from < i && i <= to => i - 1,
            (Some(from), Some(to)) if to <= i && i < from => i + 1,
            (Some(from), None) if self.out && i > from => i - 1,
            (None, Some(to)) if i >= to => i + 1,
            _ => i,
        }
    }
}

/// Where place `k` of the toolbar row is, one past the last too.
fn slot(slots: &[[f32; 4]], k: usize) -> Option<[f32; 2]> {
    if let Some(r) = slots.get(k) {
        return Some([r[0], r[1]]);
    }
    let n = slots.len();
    let last = slots.get(n.checked_sub(1)?)?;
    let step = if n >= 2 { [last[0] - slots[n - 2][0], last[1] - slots[n - 2][1]] } else { [last[2] + 6.0, 0.0] };
    let more = (k + 1 - n) as f32;
    Some([last[0] + step[0] * more, last[1] + step[1] * more])
}

fn over_row(area: Option<[f32; 4]>, at: [f32; 2]) -> bool {
    area.is_some_and(|r| {
        at[0] >= r[0] - PIN_REACH && at[0] <= r[0] + r[2] + PIN_REACH && at[1] >= r[1] - PIN_REACH && at[1] <= r[1] + r[3] + PIN_REACH
    })
}

/// The place in the row a pointer dragged to `at` lands in, or `None` off the row (or when a
/// new one finds the toolbar full).
fn landing(slots: &[[f32; 4]], area: Option<[f32; 4]>, at: [f32; 2], from: Option<usize>, full: bool) -> Option<usize> {
    if !over_row(area, at) {
        return None;
    }
    let centre = |r: &[f32; 4]| [r[0] + r[2] / 2.0, r[1] + r[3] / 2.0];
    let distance = |r: &[f32; 4]| {
        let c = centre(r);
        (c[0] - at[0]).hypot(c[1] - at[1])
    };
    let near = (0..slots.len()).min_by(|a, b| distance(&slots[*a]).total_cmp(&distance(&slots[*b])));
    match from {
        Some(from) => Some(near.unwrap_or(from)),
        None if full => None,
        // A new one goes before or after the nearest, whichever side the pointer is on.
        None => Some(near.map_or(0, |j| if at[0] > centre(&slots[j])[0] { j + 1 } else { j })),
    }
}

/// A press, drag or drop on a pointer: `from` is its place when it was picked up in the row.
/// The others make room, and the drop puts it there, or takes it out.
fn drag_pin(s: &mut Ui, id: &str, from: Option<usize>, d: &Drag, tx: &mut Tx) {
    match d.phase {
        Phase::Down => {
            s.pin_dragged = false;
            let pinned = s.hub.pinned();
            s.pin_slots.truncate(pinned.len());
            let place = from.or_else(|| pinned.iter().position(|p| p == id));
            s.pin_drag = Some(PinDrag { id: id.to_string(), from: place, out: from.is_some(), to: place, d: [0.0, 0.0], moving: false });
        }
        Phase::Move => {
            let full = s.hub.pinned().len() >= hands::MAX_PINNED;
            let (slots, area) = (s.pin_slots.clone(), s.pin_area);
            let Some(g) = &mut s.pin_drag else { return };
            g.d = [d.dx, d.dy];
            if !g.moving && d.dx.hypot(d.dy) < PIN_SLOP {
                return;
            }
            g.moving = true;
            g.to = landing(&slots, area, d.window_point(), g.from, full);
            tx.animated();
        }
        Phase::Up => {
            let Some(g) = s.pin_drag.take() else { return };
            s.pin_dragged = g.moving;
            if g.moving && !d.cancelled() {
                drop_pin(s, &g, over_row(s.pin_area, d.window_point()));
            }
        }
    }
}

fn drop_pin(s: &mut Ui, g: &PinDrag, over: bool) {
    let mut pinned = s.hub.pinned();
    match (g.from, g.to) {
        (Some(from), Some(to)) if from != to && from < pinned.len() && to < pinned.len() => {
            let id = pinned.remove(from);
            pinned.insert(to, id);
        }
        // Off the row, or onto a full toolbar: as a click used to, with its message.
        (Some(_), None) if g.out => return pin(s, &g.id),
        (None, None) if over => return pin(s, &g.id),
        (None, Some(to)) => pinned.insert(to.min(pinned.len()), g.id.clone()),
        _ => return,
    }
    s.pin_message = None;
    s.hub.change(|st| st.pinned = pinned);
}

fn pin(s: &mut Ui, id: &str) {
    let mut pinned = s.hub.pinned();
    if let Some(i) = pinned.iter().position(|p| p == id) {
        if pinned.len() == 1 {
            s.pin_message = Some(tr("Keep at least one pointer in the toolbar.").into());
            return;
        }
        pinned.remove(i);
    } else {
        if pinned.len() >= hands::MAX_PINNED {
            s.pin_message = Some(trf("The toolbar is full. Take one out above to add {}.", &[&s.hub.title(id)]));
            return;
        }
        pinned.push(id.to_string());
    }
    s.pin_message = None;
    s.hub.change(|st| st.pinned = pinned);
}

fn pointers(p: Paint, s: &Ui, st: &settings::Settings) -> El {
    let pinned = s.hub.pinned();
    let drag = s.pin_drag.as_ref().filter(|g| g.moving);
    let mut bar = row().key("pinned").gap(6.0).padding(space::S3).flex_wrap().on_layout(|s: &mut Ui, r| s.pin_area = Some(r));
    for (i, id) in pinned.iter().enumerate() {
        let current = *id == st.pointer;
        let lifted = drag.is_some_and(|g| g.out && g.from == Some(i));
        let (pick, take) = (id.clone(), id.clone());
        let mut item = col().key(format!("pinned-{id}")).w(72.0).gap(4.0).center().padding(6.0).radius(radius::LG)
            .fill(if lifted { p.t.card } else if current { p.accent.alpha(0.14) } else { Color::TRANSPARENT })
            .hover(p.t.hover).press_scale(0.95).help(trf("Take {} out of the toolbar", &[&s.hub.title(id)]))
            .on_layout(move |s: &mut Ui, r| {
                if s.pin_slots.len() <= i {
                    s.pin_slots.resize(i + 1, [0.0; 4]);
                }
                s.pin_slots[i] = r;
            })
            .on_drag(move |s: &mut Ui, d: &mut Drag, tx| drag_pin(s, &pick.clone(), Some(i), d, tx))
            .on_press(move |s: &mut Ui| {
                if !std::mem::take(&mut s.pin_dragged) {
                    pin(s, &take);
                }
            })
            .child(thumb(s, id, 48.0))
            .child(text(s.hub.title(id)).font(rune::tok::TextStyle::new(11.0, 500.0)).color(p.t.text_secondary).truncate());
        if let Some(g) = drag {
            if lifted {
                // It follows the pointer exactly, above the others; off the row it fades, as
                // dropping it there takes it out.
                item = item.offset(g.d[0], g.d[1]).offset_motion(rune::Anim::INSTANT).raise().cursor(rune::Cursor::Grabbing)
                    .shadow(4.0, 14.0, p.t.shadow).opacity(if g.to.is_some() { 1.0 } else { 0.45 });
            } else if let (Some(home), Some(there)) = (slot(&s.pin_slots, i), slot(&s.pin_slots, g.place(i))) {
                item = item.offset(there[0] - home[0], there[1] - home[1]);
            }
        }
        bar = bar.child(item);
    }
    // A pointer from below over the row lights it up.
    let adding = drag.is_some_and(|g| !g.out && g.to.is_some());
    let mut g = page(p, tr("Pointers"));
    let mut toolbar = col().key("toolbar-section").gap(space::S2)
        .child(text(tr("In your toolbar")).font(type_::SECTION).color(p.t.text_secondary).ml(space::S3))
        .child(col().fill(if adding { p.t.card.mix(p.accent, 0.12) } else { p.t.card }).radius(radius::LG)
            .border(p.hair, if adding { p.accent } else { p.t.card_border }).shadow(1.0, 2.0, p.t.shadow)
            .child(bar)
            .child(el().h(p.hair).ml(space::S3).mr(space::S3).fill(p.t.separator))
            .child(row().px(space::S3).py(10.0).gap(16.0).center_y()
                .child(text(trf("{} of {} places. Drag to reorder, drag out to take one out.", &[&pinned.len(), &hands::MAX_PINNED])).font(type_::LABEL).color(p.t.text_secondary).grow(1.0))
                .child(button(p, "reset-toolbar", tr("Reset"), false).on_press(|s: &mut Ui| {
                    s.pin_message = None;
                    s.hub.change(|st| st.pinned = hands::DEFAULT_PINNED.iter().map(|x| x.to_string()).collect());
                }))));
    if let Some(m) = &s.pin_message {
        toolbar = toolbar.child(text(m.as_str()).font(type_::LABEL).color(Color { r: 0.95, g: 0.6, b: 0.2, a: 1.0 }).ml(space::S3)
            .transition(Transition::opacity()));
    }
    g = g.child(toolbar);

    let mut grid = row().key("all").gap(4.0).padding(space::S3).flex_wrap();
    for id in s.hub.styles() {
        let is_pinned = pinned.contains(&id);
        let lifted = drag.is_some_and(|g| !g.out && g.id == id);
        let (pick, take) = (id.clone(), id.clone());
        let mut tile = el().key(format!("tile-{id}")).size(64.0, 64.0).center().radius(radius::LG)
            .hover(p.t.hover).press_scale(0.92).help(s.hub.title(&id))
            .on_drag(move |s: &mut Ui, d: &mut Drag, tx| drag_pin(s, &pick, None, d, tx))
            .on_press(move |s: &mut Ui| {
                if !std::mem::take(&mut s.pin_dragged) {
                    pin(s, &take);
                }
            })
            .child(thumb(s, &id, 50.0));
        if lifted && let Some(g) = drag {
            tile = tile.offset(g.d[0], g.d[1]).offset_motion(rune::Anim::INSTANT).raise().cursor(rune::Cursor::Grabbing)
                .fill(p.t.card).shadow(4.0, 14.0, p.t.shadow);
        }
        if is_pinned {
            tile = tile.child(el().size(18.0, 18.0).radius(9.0).center().fill(p.accent).absolute(44.0, 2.0)
                .shadow(1.0, 3.0, Color::BLACK.alpha(0.3))
                .transition(Transition::scale(0.4).combined(Transition::opacity()))
                .child(icon(&icons::CHECK, 12.0).color(Color::WHITE)));
        }
        grid = grid.child(tile);
    }
    g = g.child(col().key("all-section").gap(space::S2)
        .child(text(tr("All pointers")).font(type_::SECTION).color(p.t.text_secondary).ml(space::S3))
        .child(col().fill(p.t.card).radius(radius::LG).border(p.hair, p.t.card_border).shadow(1.0, 2.0, p.t.shadow).child(grid))
        .child(text(tr("Click a pointer to add it to the toolbar or take it out.")).font(type_::LABEL).color(p.t.text_secondary).ml(space::S3)));

    let mut yours = row().key("yours").gap(6.0).padding(space::S3).flex_wrap();
    for c in s.hub.custom_hands() {
        let id = c.id.clone();
        yours = yours.child(
            col().key(format!("custom-{}", c.id)).w(84.0).gap(4.0).center().padding(6.0).radius(radius::LG).hover(p.t.hover)
                .help(trf("Remove {}", &[&c.name]))
                .on_press(move |s: &mut Ui| remove_hand(s, &id))
                .child(thumb(s, &c.id, 48.0))
                .child(text(c.name.clone()).font(rune::tok::TextStyle::new(11.0, 500.0)).color(p.t.text_secondary).truncate()),
        );
    }
    yours = yours.child(
        col().key("make-hand").w(84.0).h(78.0).gap(6.0).center().radius(radius::LG).border(1.5, p.t.separator)
            .hover(p.t.hover).press_scale(0.95).label(tr("Make your own hand")).focusable(true)
            .on_press(|s: &mut Ui| {
                s.page = Page::MakeHand;
                s.draft = None;
                s.draft_error = None;
                s.hand_message = None;
            })
            .child(el().size(28.0, 28.0).radius(14.0).center().fill(p.accent).child(icon(&icons::PLUS, 16.0).color(Color::WHITE)))
            .child(text(tr("Make one")).font(rune::tok::TextStyle::new(11.0, 500.0)).color(p.t.text_secondary)),
    );
    let mut yours_section = col().key("yours-section").gap(space::S2)
        .child(text(tr("Your hands")).font(type_::SECTION).color(p.t.text_secondary).ml(space::S3))
        .child(col().fill(p.t.card).radius(radius::LG).border(p.hair, p.t.card_border).shadow(1.0, 2.0, p.t.shadow).child(yours))
        .child(text(tr("Turn any idea into a hand with ChatGPT or another image tool. Click one of yours to remove it.")).font(type_::LABEL).color(p.t.text_secondary).ml(space::S3).wrap());
    if let Some(m) = &s.hand_message {
        yours_section = yours_section.child(text(m.as_str()).font(type_::LABEL).color(Color { r: 0.3, g: 0.8, b: 0.45, a: 1.0 }).ml(space::S3));
    }
    g = g.child(yours_section);

    g.child(card(p, "toolbar-options", Some(tr("Toolbar")), [
        toggle_row(p, "show-toolbar", tr("Show the toolbar"), Some(tr("It comes up when the pointer gets near the bottom of the screen.")), st.show_toolbar, true,
            |s| s.hub.change(|st| st.show_toolbar = !st.show_toolbar)),
        toggle_row(p, "show-ink", tr("Show the ink button"), None, st.show_ink_button, true, |s| s.hub.change(|st| st.show_ink_button = !st.show_ink_button)),
    ]))
}

// --- Appearance -----------------------------------------------------------------------------

/// A setting a slider changes.
#[derive(Clone, Copy)]
struct Slide {
    key: &'static str,
    label: &'static str,
    lo: f32,
    hi: f32,
    get: fn(&settings::Settings) -> f32,
    set: fn(&mut settings::Settings, f32),
    /// Runs when the knob is let go: the volume plays the sound once.
    done: Option<fn(&Ui)>,
}

const SIZE: Slide = Slide { key: "size-slider", label: n!("Pointer size"), lo: settings::SIZE_RANGE.0, hi: settings::SIZE_RANGE.1,
    get: |st| st.size, set: |st, v| st.size = v, done: None };
const VOLUME: Slide = Slide { key: "volume-slider", label: n!("Volume"), lo: 0.0, hi: 1.0,
    get: |st| st.volume, set: |st, v| st.volume = v, done: Some(|s| s.hub.preview()) };

fn slider(p: Paint, spec: Slide, value: f32) -> El {
    let Slide { lo, hi, get, set, done, .. } = spec;
    let v = (value - lo) / (hi - lo);
    let (w, h, k, bar) = (220.0, 24.0, 20.0, 4.0);
    let travel = w - k;
    let to_value = move |x: f32| lo + ((x - k / 2.0) / travel).clamp(0.0, 1.0) * (hi - lo);
    let step = (hi - lo) / 20.0;
    el().key(spec.key).role("slider").label(tr(spec.label)).range_value(f64::from(value), f64::from(lo), f64::from(hi))
        .range_step(f64::from(step))
        .on_step(move |s: &mut Ui, steps, _| s.hub.change(|st| { let v = get(st) + steps as f32 * step; set(st, v.clamp(lo, hi)) }))
        .size(w, h).motion(motion::SNAPPY)
        .on_drag(move |s: &mut Ui, d: &mut Drag, tx| match d.phase {
            Phase::Down => {
                d.data = s.hub.with_settings(get);
                let knob = k / 2.0 + v * travel;
                if (d.x - knob).abs() > k / 2.0 + 2.0 {
                    let to = to_value(d.x);
                    s.hub.change(|st| set(st, to));
                }
            }
            Phase::Move => {
                let start = (d.data - lo) / (hi - lo);
                let knob = k / 2.0 + start * travel;
                let grab = if (d.start_x - knob).abs() <= k / 2.0 + 2.0 { d.start_x - knob } else { 0.0 };
                let to = to_value(d.x - grab);
                s.hub.change(|st| set(st, to));
                tx.instant();
            }
            Phase::Up => {
                if d.cancelled() {
                    let back = d.data;
                    s.hub.change(|st| set(st, back));
                } else if let Some(done) = done {
                    done(s);
                }
            }
        })
        .child(el().size(w, bar).absolute(0.0, (h - bar) / 2.0).radius(bar / 2.0).fill(p.t.control_off))
        .child(el().size(k / 2.0 + v * travel, bar).absolute(0.0, (h - bar) / 2.0).radius(bar / 2.0).fill(p.accent))
        .child(el().key("knob").size(k, k).absolute(v * travel, (h - k) / 2.0).radius(k / 2.0).fill(p.t.knob)
            .shadow(1.0, 3.0, p.t.knob_shadow).shadow(0.0, 0.5, p.t.edge))
}

fn appearance(p: Paint, s: &Ui, st: &settings::Settings) -> El {
    // A live preview: the hand at the size it will have on a 1080 pt tall screen, scaled to fit.
    let style = s.hub.style();
    let preview_h = 190.0;
    let hand_h = st.size / settings::SIZE_RANGE.1 * 150.0;
    let preview = el().key("preview").w_full().h(preview_h).radius(radius::LG).clip_rounded(radius::LG)
        .linear_gradient(160.0, [(0.0, Color { r: 0.42, g: 0.33, b: 0.62, a: 1.0 }), (1.0, Color { r: 0.16, g: 0.12, b: 0.26, a: 1.0 })])
        .center()
        .child(col().center().gap(0.0)
            .child(el().key("preview-hand").size(hand_h, hand_h).motion(Spring::BOUNCY).child(thumb(s, &style, hand_h)))
            // A soft round shadow on the "floor" under the hand.
            .child(el().key("preview-shadow").size(hand_h * 0.55, 10.0).radius(5.0).motion(Spring::BOUNCY)
                .fill(Color::BLACK.alpha(if st.shadow { 0.22 } else { 0.0 }))
                .shadow(0.0, if st.shadow { 14.0 } else { 0.0 }, Color::BLACK.alpha(if st.shadow { 0.25 } else { 0.0 }))));
    page(p, tr("Appearance"))
        .child(col().fill(p.t.card).radius(radius::LG).border(p.hair, p.t.card_border).shadow(1.0, 2.0, p.t.shadow).padding(space::S1).child(preview))
        .child(card(p, "look", None, [
            setting(p, "size", tr("Pointer size"), Some(tr("How tall the hand is, as a share of the screen.")),
                row().gap(10.0).center_y()
                    .child(text("S").font(type_::LABEL).color(p.t.text_secondary))
                    .child(slider(p, SIZE, st.size))
                    .child(text("L").font(type_::LABEL).color(p.t.text_secondary))),
            toggle_row(p, "shadow", tr("Shadow under the hand"), None, st.shadow, true, |s| s.hub.change(|st| st.shadow = !st.shadow)),
        ]))
}

// --- Drawing --------------------------------------------------------------------------------

fn drawing(p: Paint, st: &settings::Settings) -> El {
    let (d, gap) = (26.0, 12.0);
    let ring = d + 8.0;
    let current = INKS.iter().position(|i| i.0 == st.ink).unwrap_or(0);
    let mut swatches = row().key("inks").gap(gap).motion(motion::SNAPPY)
        .child(el().key("ink-ring").size(ring, ring).absolute(current as f32 * (d + gap) - (ring - d) / 2.0, -(ring - d) / 2.0)
            .radius(ring / 2.0).border(2.0, p.accent).lead(Spring::new(0.22, 0.0), Spring::new(0.3, 0.0)).a11y_hidden());
    for (i, (id, title, rgb)) in INKS.iter().enumerate() {
        let dot = if *id == "match" { icons::rainbow(d) } else {
            el().size(d, d).radius(d / 2.0).fill(hands::rgb(*rgb)).border(p.hair, Color::BLACK.alpha(if p.dark { 0.0 } else { 0.12 }))
        };
        let choice = id.to_string();
        swatches = swatches.child(dot.key(format!("ink-{id}")).role("radio").label(tr(title)).checked(i == current).focusable(true)
            .help(tr(title)).press_scale(0.88).on_press(move |s: &mut Ui| s.hub.change(|st| st.ink = choice.clone())));
    }
    let fade = FADES.iter().position(|f| (f.0 - st.fade_seconds).abs() < 0.01).unwrap_or(2);
    let labels: Vec<&str> = FADES.iter().map(|f| tr(f.1)).collect();
    page(p, tr("Drawing"))
        .child(card(p, "ink", Some(tr("Ink")), [
            setting(p, "ink-row", tr("Ink colour"), Some(tr(INKS[current].1)), swatches.mr(4.0)),
            toggle_row(p, "ripples-row", tr("Ring where you click"), Some(tr("A ring spreads where you click or start a line.")), st.ripples, true,
                |s| s.hub.change(|st| st.ripples = !st.ripples)),
            setting(p, "fade-row", tr("Drawings fade after"), Some(tr("Or keep them until you clear them.")),
                segmented(p, "fade", &labels, fade, 300.0, true, |s, i| s.hub.change(|st| st.fade_seconds = FADES[i].0))),
        ]))
        .child(text(tr("Drag to draw. Hold Ctrl while drawing to end the line in an arrowhead.")).font(type_::LABEL).color(p.t.text_secondary).ml(space::S3).wrap())
}

// --- Sound ----------------------------------------------------------------------------------

fn sound(p: Paint, st: &settings::Settings) -> El {
    let mut grid = row().key("sounds").gap(8.0).padding(space::S3).flex_wrap();
    // One width for all, wide enough for the longest name in this language.
    let widest = TAP_SOUNDS.iter().map(|(_, t)| rune::text::measure(tr(t), &[], type_::BODY, None).0).fold(0.0, f32::max);
    let tile = (widest + 42.0).max(132.0).ceil();
    for (id, title) in TAP_SOUNDS {
        let selected = st.tap_sound == *id;
        let choice = id.to_string();
        grid = grid.child(
            row().key(format!("sound-{id}")).w(tile).h(40.0).px(10.0).gap(8.0).center_y().radius(radius::MD)
                .fill(if selected { p.accent.alpha(0.16) } else { p.t.control_track })
                .border(if selected { 1.5 } else { 0.0 }, p.accent).motion(motion::SNAPPY)
                .hover(p.t.hover).press_scale(0.95)
                .on_press(move |s: &mut Ui| {
                    s.hub.sound.preview(&choice);
                    s.hub.change(|st| st.tap_sound = choice.clone());
                })
                .child(icons::speaker(true, 14.0, if selected { p.accent } else { p.t.text_secondary }))
                .child(text(tr(title)).font(type_::BODY).color(p.t.text)),
        );
    }
    page(p, tr("Sound"))
        .child(col().key("tap-section").gap(space::S2)
            .child(text(tr("Tap sound")).font(type_::SECTION).color(p.t.text_secondary).ml(space::S3))
            .child(col().fill(p.t.card).radius(radius::LG).border(p.hair, p.t.card_border).shadow(1.0, 2.0, p.t.shadow).child(grid))
            .child(text(tr("Click a sound to hear it. It plays each time the hand taps.")).font(type_::LABEL).color(p.t.text_secondary).ml(space::S3)))
        .child(card(p, "sound-options", None, [
            setting(p, "volume", tr("Volume"), None,
                row().gap(10.0).center_y()
                    .child(icons::speaker(false, 15.0, p.t.text_secondary))
                    .child(slider(p, VOLUME, st.volume))
                    .child(icons::speaker(true, 15.0, p.t.text_secondary))
                    .child(text(format!("{:.0} %", st.volume * 100.0)).key("volume-value").w(42.0).font(type_::LABEL).color(p.t.text_secondary))),
            toggle_row(p, "hand-sound", tr("Tap sound for hands"), None, st.hand_sound, true, |s| s.hub.change(|st| st.hand_sound = !st.hand_sound)),
            toggle_row(p, "laser-sound", tr("Tap sound for the laser dot"), None, st.laser_sound, true, |s| s.hub.change(|st| st.laser_sound = !st.laser_sound)),
        ]))
}

// --- About ----------------------------------------------------------------------------------

fn about(p: Paint) -> El {
    page(p, tr("About"))
        .child(row().gap(space::S6).center_y()
            .child(app_mark(84.0))
            .child(col().gap(4.0).grow(1.0)
                .child(text("Waypointer").font(rune::tok::TextStyle::new(24.0, 700.0)).color(p.t.text))
                .child(text(trf("Version {}", &[&env!("CARGO_PKG_VERSION")])).font(type_::BODY).color(p.t.text_secondary))
                .child(text(tr("A tiny hand on a stick that points at things on your screen. Free and open source.")).font(type_::BODY).color(p.t.text_secondary).wrap())))
        .child(card(p, "credits", None, [
            setting(p, "upstream", tr("Based on TinyHand for macOS"), Some(tr("By nilbuild. The hands, the sounds and the idea are theirs.")),
                button(p, "upstream-link", tr("Open"), false).on_press(|_: &mut Ui| open("https://github.com/nilbuild/tinyhand"))),
            setting(p, "repo", tr("Waypointer on GitHub"), Some(tr("Code, releases and bug reports.")),
                button(p, "repo-link", tr("Open"), false).on_press(|_: &mut Ui| open("https://github.com/Felitendo/waypointer"))),
        ]))
}

fn open(url: &str) {
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

// --- Making a hand --------------------------------------------------------------------------

fn remove_hand(s: &mut Ui, id: &str) {
    let list: Vec<_> = s.hub.custom_hands().into_iter().filter(|c| c.id != id).collect();
    let _ = std::fs::remove_file(hands::custom_dir().join(format!("{id}.png")));
    s.hub.art.forget(id);
    let id = id.to_string();
    s.hub.change(|st| {
        st.pinned.retain(|p| *p != id);
        if st.pointer == id {
            st.pointer = "lavender".into();
        }
    });
    s.hub.set_custom_hands(list);
    s.hand_message = None;
}

fn encode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn step(p: Paint, number: u32, title: &str, body: El) -> El {
    row().gap(space::S3).padding(space::S3)
        .child(el().size(26.0, 26.0).radius(13.0).center().fill(p.accent)
            .child(text(number.to_string()).font(rune::tok::TextStyle::new(12.5, 700.0)).color(Color::WHITE)))
        .child(col().gap(10.0).grow(1.0).min_w(0.0).child(text(title).font(type_::BODY).color(p.t.text).wrap()).child(body))
}

fn field(p: Paint, edit: &TextEdit, placeholder: &str, access: fn(&mut Ui) -> &mut TextEdit) -> El {
    let mut f = el().w_full().h(32.0).px(10.0).center_y().radius(radius::MD).fill(p.t.control_track).border(p.hair, p.t.card_border)
        .child(text_input(edit, type_::BODY).single_line().on_edit(access).w_full().color(p.t.text));
    if edit.text.is_empty() {
        f = f.child(text(placeholder).font(type_::BODY).color(p.t.text_secondary).absolute(10.0, 7.0).a11y_hidden());
    }
    f
}

fn add_hand(s: &mut Ui) {
    let Some(draft) = s.draft.take() else { return };
    let dir = hands::custom_dir();
    let _ = std::fs::create_dir_all(&dir);
    let id = format!("custom-{:08x}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u32).unwrap_or(0));
    if draft.image.save(dir.join(format!("{id}.png"))).is_err() {
        s.draft_error = Some(tr("Waypointer couldn't save the hand.").into());
        return;
    }
    let mut list = s.hub.custom_hands();
    let name = s.hand_name.text.trim().chars().take(16).collect::<String>();
    let name = if name.is_empty() { trf("My hand {}", &[&(list.len() + 1)]) } else { name };
    list.push(hands::Custom { id: id.clone(), name: name.clone(), ink: draft.ink });
    s.hub.set_custom_hands(list);
    let mut pinned = s.hub.pinned();
    s.hand_message = Some(if pinned.len() < hands::MAX_PINNED {
        let at = if pinned.last().is_some_and(|l| l == hands::LASER) { pinned.len() - 1 } else { pinned.len() };
        pinned.insert(at, id.clone());
        trf("{} is in your toolbar.", &[&name])
    } else {
        trf("{} is ready. Add it to the toolbar above.", &[&name])
    });
    let new_id = id.clone();
    s.hub.change(|st| {
        st.pinned = pinned;
        st.pointer = new_id;
    });
    let hub = s.hub.clone();
    s.hub.art.load_thumbs(vec![id], move || hub.refresh());
    s.idea.replace_text("");
    s.page = Page::Pointers;
}

fn make_hand(p: Paint, s: &Ui) -> El {
    let back = row().key("back").gap(6.0).center_y().h(28.0).px(8.0).radius(radius::MD).hover(p.t.hover).focusable(true).label(tr("Back to Pointers"))
        .on_press(|s: &mut Ui| s.page = Page::Pointers)
        .child(text(tr("‹ Pointers")).font(type_::LABEL).color(p.accent));
    let ask = row().gap(8.0)
        .child(button(p, "copy-prompt", if s.copied { tr("Copied") } else { tr("Copy prompt") }, false).on_press_tx(|s: &mut Ui, tx: &mut Tx| {
            tx.copy(crate::handmaker::prompt(&s.idea.text));
            s.copied = true;
        }))
        .child(button(p, "open-chatgpt", tr("Open ChatGPT"), false).on_press(|s: &mut Ui| {
            open(&format!("https://chatgpt.com/?q={}", encode(&crate::handmaker::prompt(&s.idea.text))));
        }));
    let choose = button(p, "choose", tr("Choose picture…"), true).on_press_tx(|_: &mut Ui, tx: &mut Tx| {
        let options = FileDialogOptions { title: tr("Pick the PNG of your hand").into(), ..Default::default() }.filter(tr("Pictures"), &["png", "webp"]);
        tx.open_file(options, |s: &mut Ui, result, _| match result {
            Ok(Some(path)) => match crate::handmaker::make(&path) {
                Ok(made) => {
                    let preview = Pixels::new();
                    if let Some(m) = hands::Metrics::measure(&made.image) {
                        let thumb = hands::Picture { image: made.image.clone(), metrics: m }.thumbnail(128);
                        preview.set(thumb.width(), thumb.height(), thumb.into_raw());
                    }
                    s.hand_name.replace_text(crate::handmaker::suggested_name(&path, s.hub.custom_hands().len()));
                    s.draft = Some(Draft { image: made.image, ink: made.ink, preview });
                    s.draft_error = None;
                }
                Err(e) => {
                    s.draft = None;
                    s.draft_error = Some(e);
                }
            },
            Ok(None) => {}
            Err(e) => s.draft_error = Some(e.to_string()),
        });
    });
    let mut pick = col().gap(8.0).child(row().child(choose));
    if let Some(e) = &s.draft_error {
        pick = pick.child(text(e.as_str()).font(type_::LABEL).color(Color { r: 0.95, g: 0.38, b: 0.32, a: 1.0 }).wrap()
            .transition(Transition::offset(0.0, -4.0).combined(Transition::opacity())));
    }
    let mut steps = col().key("steps").fill(p.t.card).radius(radius::LG).border(p.hair, p.t.card_border).shadow(1.0, 2.0, p.t.shadow)
        .child(step(p, 1, tr("Describe your hand, then ask an image tool for it."),
            col().gap(8.0).child(field(p, &s.idea, tr("For example: knitted wool in mustard yellow"), |s| &mut s.idea)).child(ask)))
        .child(el().h(p.hair).ml(space::S3).mr(space::S3).fill(p.t.separator))
        .child(step(p, 2, tr("Save the picture as a PNG with a transparent background, then pick it here."), pick));
    if let Some(draft) = &s.draft {
        steps = steps.child(el().h(p.hair).ml(space::S3).mr(space::S3).fill(p.t.separator)).child(step(p, 3, tr("Give it a name and add it."),
            row().gap(space::S3).center_y()
                .child(el().size(84.0, 84.0).radius(radius::LG).fill(p.t.control_track).center().child(el().size(76.0, 76.0).pixels(&draft.preview))
                    .transition(Transition::scale(0.8).combined(Transition::opacity())))
                .child(col().gap(8.0).grow(1.0)
                    .child(field(p, &s.hand_name, tr("Name"), |s| &mut s.hand_name))
                    .child(row().child(button(p, "add-hand", tr("Add hand"), true).on_press(add_hand)))
                    .child(text(tr("It should show the back of the hand, like the ones in Waypointer. If you see the palm, try another picture."))
                        .font(type_::LABEL).color(p.t.text_secondary).wrap()))));
    }
    page(p, tr("Make your own hand")).child(back).child(steps)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune::{Env, Runtime};

    fn frames(rt: &mut Runtime<Ui>, n: usize) {
        for _ in 0..n {
            rt.frame(1.0 / 60.0);
        }
    }

    fn pointers_page() -> (Arc<Hub>, Runtime<Ui>) {
        crate::settings::test_config();
        let hub = Hub::new(settings::Settings::default());
        let mut ui = Ui::new(hub.clone());
        ui.page = Page::Pointers;
        let mut rt = Runtime::new(ui, view, (860.0, 600.0), Env { system_dark: true, scale: 1.0 });
        frames(&mut rt, 60);
        (hub, rt)
    }

    #[test]
    fn a_pointer_in_the_toolbar_row_drags_to_a_new_place() {
        let (hub, mut rt) = pointers_page();
        let before = hub.pinned();
        let (x0, y) = rt.find(&format!("pinned-{}", before[0])).expect("the row shows");
        let (x2, _) = rt.find(&format!("pinned-{}", before[2])).unwrap();
        rt.pointer_down(x0, y);
        for k in 1..=10 {
            rt.pointer_move(x0 + (x2 - x0) * k as f32 / 10.0, y);
            rt.frame(1.0 / 60.0);
        }
        frames(&mut rt, 30);
        let (mid, _) = rt.find(&format!("pinned-{}", before[1])).unwrap();
        assert!(mid < x0 + 10.0, "the second one stepped aside to the first place: {mid}");
        rt.pointer_up(x2, y);
        frames(&mut rt, 30);
        let mut want = before.clone();
        let first = want.remove(0);
        want.insert(2, first);
        assert_eq!(hub.pinned(), want, "the drop put it in the third place, and took nothing out");

        click(&mut rt, &format!("pinned-{}", want[1]));
        assert_eq!(hub.pinned().len(), before.len() - 1, "a click still takes one out");
    }

    fn click(rt: &mut Runtime<Ui>, key: &str) {
        let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} shows"));
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        frames(rt, 10);
    }

    fn drag(rt: &mut Runtime<Ui>, from: (f32, f32), to: (f32, f32)) {
        rt.pointer_down(from.0, from.1);
        for k in 1..=12 {
            let t = k as f32 / 12.0;
            rt.pointer_move(from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t);
            rt.frame(1.0 / 60.0);
        }
        rt.pointer_up(to.0, to.1);
        frames(rt, 30);
    }

    #[test]
    fn pointers_drag_into_the_toolbar_and_out_of_it() {
        let (hub, mut rt) = pointers_page();
        let before = hub.pinned();
        assert!(!before.contains(&"pink".to_string()));
        let tile = rt.find("tile-pink").expect("all pointers show");
        let (x1, y1) = rt.find(&format!("pinned-{}", before[1])).unwrap();
        drag(&mut rt, tile, (x1 - 10.0, y1));
        let after = hub.pinned();
        assert_eq!(after.len(), before.len() + 1);
        assert_eq!(after[1], "pink", "it went in before the one it was dropped on: {after:?}");

        let (px, py) = rt.find("pinned-pink").unwrap();
        drag(&mut rt, (px, py), (px, py + 260.0));
        assert_eq!(hub.pinned(), before, "dragged out of the row, it left the toolbar");
    }
}
