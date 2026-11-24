use crate::settings::Language;
use std::collections::HashMap;
use std::fmt::Display;
use std::sync::{OnceLock, RwLock};

type Table = HashMap<&'static str, &'static str>;

fn active() -> &'static RwLock<Option<Table>> {
    static ACTIVE: OnceLock<RwLock<Option<Table>>> = OnceLock::new();
    ACTIVE.get_or_init(|| RwLock::new(table_for(&system_language())))
}

fn system_language() -> String {
    ["ENGRAM_LANG", "LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|k| std::env::var(k).ok())
        .find(|v| !v.is_empty() && v != "C" && v != "POSIX")
        .map(|v| v.split(['_', '.', '-']).next().unwrap_or("en").to_lowercase())
        .unwrap_or_else(|| "en".into())
}

fn table_for(code: &str) -> Option<Table> {
    match code {
        "ru" => Some(crate::i18n_ru::RU.iter().copied().collect()),
        _ => None,
    }
}

pub fn set_language(language: Language) {
    let table = match language {
        Language::System => table_for(&system_language()),
        Language::English => None,
        Language::Russian => table_for("ru"),
    };
    *active().write().unwrap_or_else(|e| e.into_inner()) = table;
}

pub fn tr(source: &'static str) -> &'static str {
    active()
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .and_then(|t| t.get(source).copied())
        .unwrap_or(source)
}

pub fn trf(source: &'static str, args: &[(&str, &dyn Display)]) -> String {
    args.iter().fold(tr(source).to_owned(), |s, (k, v)| {
        s.replace(&format!("{{{k}}}"), &v.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_is_the_source_language() {
        set_language(Language::English);
        assert_eq!(tr("Save"), "Save");
        assert_eq!(trf("Saved revision {rev}", &[("rev", &7)]), "Saved revision 7");
    }
}
