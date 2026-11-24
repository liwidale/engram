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
    use std::path::Path;

    fn literals(text: &str) -> Vec<String> {
        let mut out = Vec::new();
        for start in ["tr(", "trf("] {
            for (i, _) in text.match_indices(start) {
                if text[..i].ends_with(|c: char| c.is_alphanumeric() || c == '_') {
                    continue;
                }
                let Some(rest) = text[i + start.len()..].trim_start().strip_prefix('"') else {
                    continue;
                };
                let mut escaped = false;
                let end = rest
                    .char_indices()
                    .find(|&(_, c)| {
                        let close = c == '"' && !escaped;
                        escaped = c == '\\' && !escaped;
                        close
                    })
                    .map(|(j, _)| j);
                if let Some(end) = end {
                    out.push(rest[..end].to_owned());
                }
            }
        }
        out
    }

    fn sources(dir: &Path, out: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                sources(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs")
                && !path.ends_with("i18n_ru.rs")
                && !path.ends_with("i18n.rs")
            {
                out.extend(literals(&std::fs::read_to_string(&path).unwrap()));
            }
        }
    }

    fn placeholders(s: &str) -> Vec<&str> {
        let mut out: Vec<&str> = s
            .split('{')
            .skip(1)
            .filter_map(|p| p.split_once('}').map(|(name, _)| name))
            .filter(|name| !name.is_empty() && name.chars().all(|c| c.is_ascii_lowercase()))
            .collect();
        out.sort_unstable();
        out
    }

    #[test]
    fn every_interface_string_has_a_russian_translation() {
        let mut keys = Vec::new();
        sources(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut keys);
        assert!(keys.len() > 300, "the scanner found the interface strings");
        let table: Table = crate::i18n_ru::RU.iter().copied().collect();
        let missing: Vec<&String> = keys.iter().filter(|k| !table.contains_key(k.as_str())).collect();
        assert!(missing.is_empty(), "missing Russian translations: {missing:#?}");
        for (source, ru) in crate::i18n_ru::RU {
            assert_eq!(
                placeholders(source),
                placeholders(ru),
                "placeholders differ for {source:?}"
            );
            assert!(keys.iter().any(|k| k == source), "unused translation: {source:?}");
        }
    }

    #[test]
    fn english_is_the_source_language() {
        set_language(Language::English);
        assert_eq!(tr("Save"), "Save");
        assert_eq!(trf("Saved, revision {rev}", &[("rev", &7)]), "Saved, revision 7");
    }
}
