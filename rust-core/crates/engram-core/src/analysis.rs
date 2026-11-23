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

pub fn is_stopword(w: &str) -> bool {
    STOPWORDS.contains(&w)
}

pub fn stem(w: &str) -> String {
    let n = w.len();
    if n <= 3 || !w.is_ascii() {
        return w.to_owned();
    }
    for (suffix, replacement, min) in [("ies", "y", 5), ("ing", "", 6), ("ed", "", 5), ("s", "", 4)] {
        if n >= min && w.ends_with(suffix) && !w.ends_with("ss") && !w.ends_with("us") {
            let mut s = w[..n - suffix.len()].to_owned();
            s.push_str(replacement);
            if (suffix == "ing" || suffix == "ed")
                && (s.ends_with("ch") || s.ends_with('u') || s.ends_with('v') || s.ends_with('z')) {
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

pub fn terms(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for w in words(text) {
        let compound = w.contains(['-', '_', '.', '/']);
        if compound {
            out.push(w.clone());
            for part in w.split(['-', '_', '.', '/']).filter(|p| p.len() > 1 && !is_stopword(p)) {
                out.push(stem(part));
            }
        } else if w.len() > 1 && !is_stopword(&w) {
            out.push(stem(&w));
        }
    }
    out
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
    fn polarity_detection() {
        assert_eq!(polarity("Always use Postgres for persistence"), Polarity::Positive);
        assert_eq!(polarity("Never use Postgres for persistence"), Polarity::Negative);
        assert_eq!(polarity("Do not call the API from views"), Polarity::Negative);
        assert_eq!(polarity("The API is versioned"), Polarity::Neutral);
        assert_eq!(polarity("Mustard is tasty"), Polarity::Neutral);
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
    }
}
