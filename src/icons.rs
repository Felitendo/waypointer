//! Small line icons, drawn in the colour they are given.

use rune::{Color, El, Icon, el, icon};

pub static SPEAKER_ON_ICON: Icon = Icon {
    name: "wp-speaker-on",
    view: 24.0,
    body: r#"<path d="M4 9.5h3.5L12 5.5v13l-4.5-4H4z" fill="currentColor"/><path d="M15.5 9a4 4 0 0 1 0 6M18 6.5a7.5 7.5 0 0 1 0 11" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>"#,
};
static SPEAKER_OFF: Icon = Icon {
    name: "wp-speaker-off",
    view: 24.0,
    body: r#"<path d="M4 9.5h3.5L12 5.5v13l-4.5-4H4z" fill="currentColor"/><path d="M16 9.5l5 5M21 9.5l-5 5" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>"#,
};
static CLOSE: Icon = Icon {
    name: "wp-close",
    view: 24.0,
    body: r#"<path d="M6.5 6.5l11 11M17.5 6.5l-11 11" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"/>"#,
};
pub static GEAR: Icon = Icon {
    name: "wp-gear",
    view: 24.0,
    body: r#"<path d="M12 8.5a3.5 3.5 0 1 0 0 7 3.5 3.5 0 0 0 0-7z" fill="none" stroke="currentColor" stroke-width="1.8"/><path d="M19.4 13a7.6 7.6 0 0 0 0-2l2-1.6-2-3.4-2.4 1a7.4 7.4 0 0 0-1.7-1L15 3.5h-4L10.6 6a7.4 7.4 0 0 0-1.7 1l-2.4-1-2 3.4 2 1.6a7.6 7.6 0 0 0 0 2l-2 1.6 2 3.4 2.4-1a7.4 7.4 0 0 0 1.7 1l.4 2.5h4l.4-2.5a7.4 7.4 0 0 0 1.7-1l2.4 1 2-3.4z" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"/>"#,
};
pub static POINTER: Icon = Icon {
    name: "wp-pointer",
    view: 24.0,
    body: r#"<path d="M9.5 11.5V4.75a1.75 1.75 0 0 1 3.5 0V11M13 10.25a1.75 1.75 0 0 1 3.5 0V11.5M16.5 11a1.75 1.75 0 0 1 3.5 0v3.5c0 3.9-2.6 6.5-6.3 6.5h-.9c-2.4 0-3.9-1-5.2-2.7l-2.4-3.2a1.75 1.75 0 0 1 2.7-2.2l1.6 1.7" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/>"#,
};
pub static SPARKLE: Icon = Icon {
    name: "wp-sparkle",
    view: 24.0,
    body: r#"<path d="M11 3.5l1.7 4.8 4.8 1.7-4.8 1.7L11 16.5l-1.7-4.8L4.5 10l4.8-1.7z" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"/><path d="M18 15v4M16 17h4" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/>"#,
};
pub static SPEAKER_LINE: Icon = Icon {
    name: "wp-speaker-line",
    view: 24.0,
    body: r#"<path d="M11 5.5L6.5 9H4v6h2.5l4.5 3.5z" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"/><path d="M15 9.5a3.5 3.5 0 0 1 0 5M17.8 7a7 7 0 0 1 0 10" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/>"#,
};
pub static PENCIL: Icon = Icon {
    name: "wp-pencil",
    view: 24.0,
    body: r#"<path d="M4 20l1-4.5L15.5 5a2.1 2.1 0 0 1 3 0l.5.5a2.1 2.1 0 0 1 0 3L8.5 19z" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round"/><path d="M13.5 7l3.5 3.5" stroke="currentColor" stroke-width="2"/>"#,
};
pub static INFO: Icon = Icon {
    name: "wp-info",
    view: 24.0,
    body: r#"<circle cx="12" cy="12" r="9" fill="none" stroke="currentColor" stroke-width="2"/><path d="M12 11v6" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"/><circle cx="12" cy="7.6" r="1.3" fill="currentColor"/>"#,
};
pub static CHECK: Icon = Icon {
    name: "wp-check",
    view: 24.0,
    body: r#"<path d="M5 12.5l4.5 4.5L19 7.5" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"/>"#,
};
pub static PLUS: Icon = Icon {
    name: "wp-plus",
    view: 24.0,
    body: r#"<path d="M12 5v14M5 12h14" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"/>"#,
};
static RAINBOW: Icon = Icon {
    name: "wp-rainbow",
    view: 24.0,
    body: r##"<path d="M12 12L12 0A12 12 0 0 1 22.39 6z" fill="#ff3b30"/><path d="M12 12L22.39 6A12 12 0 0 1 22.39 18z" fill="#ff9500"/><path d="M12 12L22.39 18A12 12 0 0 1 12 24z" fill="#ffcc00"/><path d="M12 12L12 24A12 12 0 0 1 1.61 18z" fill="#34c759"/><path d="M12 12L1.61 18A12 12 0 0 1 1.61 6z" fill="#007aff"/><path d="M12 12L1.61 6A12 12 0 0 1 12 0z" fill="#af52de"/>"##,
};

pub fn speaker(on: bool, size: f32, color: Color) -> El {
    icon(if on { &SPEAKER_ON_ICON } else { &SPEAKER_OFF }, size).color(color)
}

pub fn close(size: f32, color: Color) -> El {
    icon(&CLOSE, size).color(color)
}

/// "Match the pointer": a little colour wheel.
pub fn rainbow(size: f32) -> El {
    el().size(size, size).clip_rounded(size / 2.0).child(icon(&RAINBOW, size))
}
