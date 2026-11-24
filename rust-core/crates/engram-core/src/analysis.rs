use crate::store::fingerprint;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

const STOPWORDS: &[&str] = &[
    "a", "about", "after", "all", "also", "an", "and", "any", "are", "as", "at", "be", "been", "before", "being",
    "but", "by", "can", "could", "did", "do", "does", "doing", "for", "from", "had", "has", "have", "having", "how",
    "i", "if", "in", "into", "is", "it", "its", "just", "me", "more", "most", "my", "new", "now", "of", "on", "once",
    "only", "or", "other", "our", "out", "over", "own", "same", "so", "some", "such", "than", "that", "the", "their",
    "them", "then", "there", "these", "they", "this", "those", "through", "to", "too", "under", "up", "very", "was",
    "we", "were", "what", "when", "where", "which", "while", "who", "why", "will", "with", "would", "you", "your",
    "need", "needs", "want", "make", "get", "let", "please", "like", "add", "use", "using", "used", "should", "must",
    "never", "always", "not", "no", "don", "t", "avoid", "prefer",
];

const STOPWORDS_RU: &[&str] = &[
    "и",
    "в",
    "во",
    "не",
    "что",
    "он",
    "на",
    "я",
    "с",
    "со",
    "как",
    "а",
    "то",
    "все",
    "всё",
    "она",
    "так",
    "его",
    "но",
    "да",
    "ты",
    "к",
    "у",
    "же",
    "вы",
    "за",
    "бы",
    "по",
    "только",
    "ее",
    "её",
    "мне",
    "было",
    "вот",
    "от",
    "меня",
    "еще",
    "ещё",
    "нет",
    "о",
    "из",
    "ему",
    "теперь",
    "когда",
    "даже",
    "ну",
    "ли",
    "если",
    "уже",
    "или",
    "ни",
    "быть",
    "был",
    "него",
    "до",
    "вас",
    "там",
    "потом",
    "себя",
    "ей",
    "может",
    "они",
    "тут",
    "где",
    "есть",
    "надо",
    "ней",
    "для",
    "мы",
    "тебя",
    "их",
    "чем",
    "была",
    "сам",
    "без",
    "чего",
    "тоже",
    "себе",
    "под",
    "будет",
    "тогда",
    "кто",
    "этот",
    "того",
    "потому",
    "этого",
    "какой",
    "здесь",
    "этом",
    "один",
    "мой",
    "тем",
    "чтобы",
    "сейчас",
    "были",
    "всех",
    "можно",
    "при",
    "об",
    "другой",
    "после",
    "над",
    "больше",
    "тот",
    "через",
    "эти",
    "нас",
    "про",
    "всего",
    "них",
    "много",
    "эту",
    "этой",
    "перед",
    "том",
    "такой",
    "им",
    "более",
    "это",
    "также",
    "нужно",
    "надо",
    "всегда",
    "никогда",
    "нельзя",
    "должен",
    "должна",
    "должны",
    "следует",
    "избегай",
    "избегайте",
    "избегать",
    "используй",
    "используйте",
    "использовать",
    "предпочитай",
    "обязательно",
    "запрещено",
    "добавь",
    "добавить",
];

pub fn is_stopword(w: &str) -> bool {
    STOPWORDS.contains(&w) || STOPWORDS_RU.contains(&w)
}

fn is_cyrillic(w: &str) -> bool {
    w.chars().any(|c| matches!(c, '\u{0400}'..='\u{04FF}'))
}

const RU_SUFFIXES: &[&str] = &[
    "ировать",
    "овать",
    "евать",
    "ывать",
    "ивать",
    "уйте",
    "ями",
    "ами",
    "ого",
    "его",
    "ому",
    "ему",
    "ыми",
    "ими",
    "иях",
    "ией",
    "иям",
    "ешь",
    "ете",
    "ите",
    "уем",
    "уют",
    "ует",
    "ать",
    "ять",
    "ить",
    "еть",
    "ия",
    "ие",
    "ий",
    "ый",
    "ой",
    "ая",
    "яя",
    "ое",
    "ее",
    "ые",
    "ую",
    "юю",
    "ов",
    "ев",
    "ей",
    "ам",
    "ям",
    "ах",
    "ях",
    "ом",
    "ем",
    "ью",
    "уй",
    "ет",
    "ют",
    "ут",
    "ит",
    "ят",
    "а",
    "я",
    "о",
    "е",
    "ы",
    "и",
    "у",
    "ю",
    "ь",
    "й",
];

fn stem_ru(w: &str) -> String {
    let len = w.chars().count();
    for suffix in RU_SUFFIXES {
        let s = suffix.chars().count();
        if len >= s + 3 && w.ends_with(suffix) {
            return w[..w.len() - suffix.len()].to_owned();
        }
    }
    w.to_owned()
}

pub fn stem(w: &str) -> String {
    if is_cyrillic(w) {
        return stem_ru(w);
    }
    let n = w.len();
    if n <= 3 || !w.is_ascii() {
        return w.to_owned();
    }
    for (suffix, replacement, min) in [("ies", "y", 5), ("ing", "", 6), ("ed", "", 5), ("s", "", 4)] {
        if n >= min && w.ends_with(suffix) && !w.ends_with("ss") && !w.ends_with("us") {
            let mut s = w[..n - suffix.len()].to_owned();
            s.push_str(replacement);
            if (suffix == "ing" || suffix == "ed")
                && (s.ends_with("ch") || s.ends_with('u') || s.ends_with('v') || s.ends_with('z'))
            {
                s.push('e');
            }
            return s;
        }
    }
    w.to_owned()
}

pub fn words(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|c: char| !(c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | '/')))
        .map(|w| w.trim_matches(|c: char| matches!(c, '-' | '.' | '/')))
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
}

fn long_enough(w: &str) -> bool {
    w.chars().nth(1).is_some()
}

pub fn terms(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for w in words(text) {
        let compound = w.contains(['-', '_', '.', '/']);
        if compound {
            out.push(w.clone());
            for part in w
                .split(['-', '_', '.', '/'])
                .filter(|p| long_enough(p) && !is_stopword(p))
            {
                out.push(stem(part));
            }
        } else if long_enough(&w) && !is_stopword(&w) {
            out.push(stem(&w));
        }
    }
    out
}

pub fn terms_cached(text: &str) -> Arc<Vec<String>> {
    static CACHE: OnceLock<Mutex<HashMap<u64, Arc<Vec<String>>>>> = OnceLock::new();
    let key = fingerprint(text.as_bytes());
    let cache = CACHE.get_or_init(Mutex::default);
    if let Some(hit) = cache.lock().unwrap_or_else(|e| e.into_inner()).get(&key) {
        return hit.clone();
    }
    let value = Arc::new(terms(text));
    let mut map = cache.lock().unwrap_or_else(|e| e.into_inner());
    if map.len() >= 4096 {
        map.clear();
    }
    map.insert(key, value.clone());
    value
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Polarity {
    Positive,
    Negative,
    Neutral,
}

const NEGATIVE_CUES: &[&str] = &[
    "never",
    "don't",
    "dont",
    "do not",
    "does not",
    "must not",
    "mustn't",
    "should not",
    "shouldn't",
    "avoid",
    "forbid",
    "forbidden",
    "prohibit",
    "disallow",
    "no longer",
    "stop using",
    "ban",
    "banned",
    "not allowed",
    "deprecated",
    "instead of",
    "никогда",
    "не используй",
    "не используйте",
    "не использовать",
    "не стоит",
    "нельзя",
    "запрещено",
    "запрещается",
    "избегай",
    "избегайте",
    "избегать",
    "не должен",
    "не должны",
    "не следует",
    "больше не",
    "вместо",
    "устарел",
    "устарело",
];

const POSITIVE_CUES: &[&str] = &[
    "always",
    "must",
    "use ",
    "prefer",
    "required",
    "require",
    "should",
    "standardize on",
    "only use",
    "adopt",
    "всегда",
    "используй ",
    "используйте",
    "использовать",
    "должен",
    "должны",
    "обязательно",
    "предпочитай",
    "следует",
];

pub fn polarity(sentence: &str) -> Polarity {
    let s = format!(" {} ", sentence.to_lowercase());
    let has = |cue: &str| {
        s.match_indices(cue).any(|(i, m)| {
            let before = s[..i].chars().next_back().is_none_or(|c| !c.is_alphanumeric());
            let after = s[i + m.len()..]
                .chars()
                .next()
                .is_none_or(|c| !c.is_alphanumeric() || m.ends_with(' '));
            before && after
        })
    };
    if NEGATIVE_CUES.iter().any(|c| has(c)) {
        Polarity::Negative
    } else if POSITIVE_CUES.iter().any(|c| has(c)) {
        Polarity::Positive
    } else {
        Polarity::Neutral
    }
}

pub fn sentences(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_fence = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence || t.is_empty() {
            continue;
        }
        let t = t.trim_start_matches(['-', '*', '+', '>', '#']).trim();
        for part in t.split_inclusive(['.', '!', '?', ';']) {
            let p = part.trim().trim_end_matches(['.', '!', '?', ';']).trim();
            if p.split_whitespace().count() >= 2 {
                out.push(p.to_owned());
            }
        }
    }
    out
}

pub fn contains_phrase(haystack: &str, phrase: &str) -> bool {
    let (h, p) = (haystack.to_lowercase(), phrase.trim().to_lowercase());
    if p.is_empty() {
        return false;
    }
    h.match_indices(&p).any(|(i, _)| {
        let before = h[..i].chars().next_back().is_none_or(|c| !c.is_alphanumeric());
        let after = h[i + p.len()..].chars().next().is_none_or(|c| !c.is_alphanumeric());
        before && after
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stemming_and_terms() {
        assert_eq!(stem("services"), "service");
        assert_eq!(stem("caching"), "cache");
        assert_eq!(stem("queries"), "query");
        assert_eq!(stem("class"), "class");
        assert_eq!(stem("api"), "api");
        let t = terms("Refactor the auth-service caching layer");
        assert_eq!(t, vec!["refactor", "auth-service", "auth", "service", "cache", "layer"]);
    }

    #[test]
    fn russian_terms_share_stems_across_forms() {
        assert_eq!(stem("платежи"), stem("платежей"));
        assert_eq!(stem("сервиса"), stem("сервисы"));
        assert_eq!(stem("использовать"), stem("используем"));
        let t = terms("Добавь возврат платежей в сервис оплаты");
        assert!(t.contains(&stem("платежи")), "{t:?}");
        assert!(t.contains(&stem("сервиса")), "{t:?}");
        assert!(!t.iter().any(|w| w == "в" || w == "добавь"), "{t:?}");
        assert_eq!(terms("я"), Vec::<String>::new());
        assert_eq!(terms("ёж"), vec!["ёж"]);
    }

    #[test]
    fn cached_terms_match_uncached() {
        let text = "Идемпотентность платежей и auth-service";
        assert_eq!(*terms_cached(text), terms(text));
        assert!(Arc::ptr_eq(&terms_cached(text), &terms_cached(text)));
    }

    #[test]
    fn polarity_detection() {
        assert_eq!(polarity("Always use Postgres for persistence"), Polarity::Positive);
        assert_eq!(polarity("Never use Postgres for persistence"), Polarity::Negative);
        assert_eq!(polarity("Do not call the API from views"), Polarity::Negative);
        assert_eq!(polarity("The API is versioned"), Polarity::Neutral);
        assert_eq!(polarity("Mustard is tasty"), Polarity::Neutral);
    }

    #[test]
    fn russian_polarity_detection() {
        assert_eq!(polarity("Всегда используй табы для отступов"), Polarity::Positive);
        assert_eq!(polarity("Никогда не используй табы для отступов"), Polarity::Negative);
        assert_eq!(polarity("Не используйте Redux в новых экранах"), Polarity::Negative);
        assert_eq!(polarity("Сервис хранит данные в Postgres"), Polarity::Neutral);
    }

    #[test]
    fn sentence_splitting_skips_code() {
        let s = sentences("- Use tabs. Never spaces!\n```\nlet x = 1. y\n```\n# Heading here\nok");
        assert_eq!(s, vec!["Use tabs", "Never spaces", "Heading here"]);
    }

    #[test]
    fn phrase_matching_respects_word_boundaries() {
        assert!(contains_phrase("We use Redux Toolkit", "redux"));
        assert!(!contains_phrase("We use reduxify", "redux"));
        assert!(contains_phrase("state: Redux.", "Redux"));
        assert!(contains_phrase("Мы отказались от Redux.", "redux"));
        assert!(!contains_phrase("платежный шлюз", "платеж"));
    }
}
