use crate::store::fingerprint;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use tiktoken_rs::o200k_base_singleton;

pub const TOKENIZER: &str = "o200k_base";

pub fn count_tokens(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }
    o200k_base_singleton().encode_ordinary(text).len()
}

pub fn count_tokens_cached(text: &str) -> usize {
    static CACHE: OnceLock<Mutex<HashMap<u64, usize>>> = OnceLock::new();
    let key = fingerprint(text.as_bytes());
    let cache = CACHE.get_or_init(Mutex::default);
    if let Some(n) = cache.lock().unwrap_or_else(|e| e.into_inner()).get(&key) {
        return *n;
    }
    let n = count_tokens(text);
    let mut map = cache.lock().unwrap_or_else(|e| e.into_inner());
    if map.len() >= 8192 {
        map.clear();
    }
    map.insert(key, n);
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_tokens() {
        assert_eq!(count_tokens(""), 0);
        assert_eq!(count_tokens("hello"), 1);
        let n = count_tokens("The quick brown fox jumps over the lazy dog.");
        assert!((8..=12).contains(&n), "{n}");
        assert_eq!(count_tokens_cached("Привет, мир"), count_tokens("Привет, мир"));
        assert_eq!(count_tokens_cached("Привет, мир"), count_tokens("Привет, мир"));
    }
}
