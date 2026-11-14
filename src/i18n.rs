//! Translations, keyed by the English text. The language follows the system unless the user
//! picks one in the settings; text without a translation stays English.
//!
//! Catalogs are `i18n/<code>.json` (English text → translation), built into the binary. Mark
//! every text with [`tr`], [`trf`] (with `{}` for what is filled in) or, in a constant, the
//! [`n!`](crate::n) marker, and translate it in every catalog: a test fails otherwise.

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

/// The languages, by code and their own name. English first: it is the source.
pub const LANGUAGES: &[(&str, &str)] = &[
    ("en", "English"),
    ("de", "Deutsch"),
    ("es", "Español"),
    ("fr", "Français"),
    ("it", "Italiano"),
    ("pt", "Português (Brasil)"),
    ("nl", "Nederlands"),
    ("pl", "Polski"),
    ("tr", "Türkçe"),
    ("ru", "Русский"),
    ("uk", "Українська"),
    ("ja", "日本語"),
    ("ko", "한국어"),
    ("zh", "简体中文"),
];

const CATALOGS: &[(&str, &str)] = &[
    ("de", include_str!("../i18n/de.json")),
    ("es", include_str!("../i18n/es.json")),
    ("fr", include_str!("../i18n/fr.json")),
    ("it", include_str!("../i18n/it.json")),
    ("pt", include_str!("../i18n/pt.json")),
    ("nl", include_str!("../i18n/nl.json")),
    ("pl", include_str!("../i18n/pl.json")),
    ("tr", include_str!("../i18n/tr.json")),
    ("ru", include_str!("../i18n/ru.json")),
    ("uk", include_str!("../i18n/uk.json")),
    ("ja", include_str!("../i18n/ja.json")),
    ("ko", include_str!("../i18n/ko.json")),
    ("zh", include_str!("../i18n/zh.json")),
];

/// Marks a text in a constant for translation; it stays English there and goes through
/// [`tr`] where it shows.
macro_rules! n {
    ($text:literal) => {
        $text
    };
}

type Table = HashMap<&'static str, &'static str>;

static CURRENT: RwLock<(&str, Option<&'static Table>)> = RwLock::new(("en", None));

fn table(code: &str) -> Option<&'static Table> {
    static TABLES: OnceLock<HashMap<&'static str, Table>> = OnceLock::new();
    TABLES
        .get_or_init(|| {
            CATALOGS
                .iter()
                .map(|(code, json)| {
                    let parsed: HashMap<String, String> = serde_json::from_str(json).unwrap_or_default();
                    let table: Table = parsed
                        .into_iter()
                        .filter(|(_, v)| !v.is_empty())
                        .map(|(k, v)| (&*Box::leak(k.into_boxed_str()), &*Box::leak(v.into_boxed_str())))
                        .collect();
                    (*code, table)
                })
                .collect()
        })
        .get(code)
}

/// The supported language closest to the system's: LANGUAGE, then LC_ALL, LC_MESSAGES, LANG.
pub fn system_language() -> &'static str {
    let mut wanted = Vec::new();
    for var in ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(value) = std::env::var(var) {
            wanted.extend(value.split(':').filter(|v| !v.is_empty()).map(str::to_string));
        }
    }
    for tag in wanted {
        let code = tag.split(['_', '-', '.', '@']).next().unwrap_or("").to_lowercase();
        if code == "c" || code == "posix" {
            continue;
        }
        if let Some((code, _)) = LANGUAGES.iter().find(|(c, _)| *c == code) {
            return code;
        }
    }
    "en"
}

/// Switches every later lookup: a language code, or "system".
pub fn set(choice: &str) {
    let code = if choice == "system" {
        system_language()
    } else {
        LANGUAGES.iter().find(|(c, _)| *c == choice).map(|(c, _)| *c).unwrap_or("en")
    };
    *CURRENT.write().unwrap() = (code, table(code));
    // Han text in Chinese or Japanese fonts as fits, shaped in that language's forms.
    rune::text::set_language(Some(if code == "zh" { "zh-Hans" } else { code }));
}

/// The text in the language in use.
pub fn tr(text: &'static str) -> &'static str {
    match CURRENT.read().unwrap().1 {
        Some(table) => table.get(text).copied().unwrap_or(text),
        None => text,
    }
}

/// The text in the language in use, with each `{}` filled from `args` in turn.
pub fn trf(text: &'static str, args: &[&dyn std::fmt::Display]) -> String {
    let mut out = String::new();
    let mut args = args.iter();
    let mut rest = tr(text);
    while let Some(at) = rest.find("{}") {
        out.push_str(&rest[..at]);
        if let Some(arg) = args.next() {
            out.push_str(&arg.to_string());
        }
        rest = &rest[at + 2..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// Every text marked in the code, by reading the code.
    fn marked() -> BTreeSet<String> {
        let mut keys = BTreeSet::new();
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|e| e != "rs") || path.ends_with("i18n.rs") {
                continue;
            }
            let src = std::fs::read_to_string(&path).unwrap();
            for marker in ["tr(\"", "trf(\"", "n!(\""] {
                let mut rest = src.as_str();
                while let Some(at) = rest.find(marker) {
                    // `str(` and the like are no markers.
                    let before = rest[..at].chars().last();
                    rest = &rest[at + marker.len()..];
                    if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                        continue;
                    }
                    let end = rest.find('"').expect("a closed string");
                    keys.insert(rest[..end].replace("\\\"", "\""));
                    rest = &rest[end..];
                }
            }
        }
        keys
    }

    #[test]
    fn every_catalog_translates_every_text() {
        let keys = marked();
        assert!(keys.len() > 100, "found the texts: {}", keys.len());
        for (code, json) in CATALOGS {
            let catalog: HashMap<String, String> = serde_json::from_str(json).unwrap_or_else(|e| panic!("{code}: {e}"));
            let missing: Vec<_> = keys.iter().filter(|k| catalog.get(*k).is_none_or(|v| v.is_empty())).collect();
            let extra: Vec<_> = catalog.keys().filter(|k| !keys.contains(*k)).collect();
            assert!(missing.is_empty(), "{code} is missing {missing:#?}");
            assert!(extra.is_empty(), "{code} has texts the code no longer uses: {extra:#?}");
            for (key, value) in &catalog {
                assert_eq!(key.matches("{}").count(), value.matches("{}").count(), "{code}: {key:?} → {value:?}");
            }
        }
    }

    #[test]
    fn placeholders_fill_in_order() {
        assert_eq!(trf("Version {}", &[&"1.2"]), "Version 1.2");
    }
}
