//! Token counting with the `o200k_base` BPE (GPT-4o family). Counts are an approximation for
//! other model families but are stable and deterministic, which is what budgeting needs.

use tiktoken_rs::o200k_base_singleton;

pub const TOKENIZER: &str = "o200k_base";

pub fn count_tokens(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }
    o200k_base_singleton().encode_ordinary(text).len()
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
    }
}
