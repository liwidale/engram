use std::collections::HashMap;
use std::fmt::Display;
use std::sync::OnceLock;

static TABLE: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();

const TRANSLATIONS: &[(&str, &[(&str, &str)])] = &[];

fn language() -> String {
    ["ENGRAM_LANG", "LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|k| std::env::var(k).ok())
        .find(|v| !v.is_empty() && v != "C" && v != "POSIX")
        .map(|v| v.split(['_', '.', '-']).next().unwrap_or("en").to_lowercase())
        .unwrap_or_else(|| "en".into())
}

fn table() -> &'static HashMap<&'static str, &'static str> {
    TABLE.get_or_init(|| {
        let lang = language();
        TRANSLATIONS
            .iter()
            .find(|(code, _)| *code == lang)
            .map(|(_, entries)| entries.iter().copied().collect())
            .unwrap_or_default()
    })
}

pub fn tr(source: &'static str) -> &'static str {
    table().get(source).copied().unwrap_or(source)
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
        assert_eq!(tr("Save"), "Save");
        assert_eq!(trf("Saved revision {rev}", &[("rev", &7)]), "Saved revision 7");
    }
}
