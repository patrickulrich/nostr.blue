use std::collections::HashSet;

pub fn contains_muted_word(
    content: &str,
    hashtags: &[String],
    muted_words: &HashSet<String>,
) -> bool {
    if muted_words.is_empty() {
        return false;
    }
    let content_lower = content.to_lowercase();
    for word in muted_words {
        let word_lower = word.to_lowercase();
        if word_lower.is_empty() {
            continue;
        }
        if is_whole_word_match(&content_lower, &word_lower) {
            return true;
        }
        for hashtag in hashtags {
            if hashtag.to_lowercase() == word_lower {
                return true;
            }
        }
    }
    false
}

fn is_whole_word_match(text_lower: &str, word_lower: &str) -> bool {
    let mut start = 0;
    while let Some(pos) = text_lower[start..].find(word_lower) {
        let match_start = start + pos;
        let match_end = match_start + word_lower.len();
        let before_is_boundary = match_start == 0
            || !text_lower.as_bytes()[match_start - 1].is_ascii_alphanumeric();
        let after_is_boundary =
            match_end >= text_lower.len() || !text_lower.as_bytes()[match_end].is_ascii_alphanumeric();
        if before_is_boundary && after_is_boundary {
            return true;
        }
        // Advance by one UTF-8 character, not one byte, to stay on char
        // boundaries. Advancing by a single byte can land inside a multi-byte
        // character (e.g. CJK, emoji, accented letters), causing the next
        // `text_lower[start..]` slice to panic with "byte index is not a char
        // boundary".
        start = match_start
            + text_lower[match_start..]
                .chars()
                .next()
                .map(|c| c.len_utf8())
                .unwrap_or(1);
        if start >= text_lower.len() {
            break;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_words(words: &[&str]) -> HashSet<String> {
        words.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn test_empty_muted_words() {
        let words = HashSet::new();
        assert!(!contains_muted_word("hello world", &[], &words));
    }

    #[test]
    fn test_simple_match() {
        let words = make_words(&["spam"]);
        assert!(contains_muted_word("this is spam content", &[], &words));
    }

    #[test]
    fn test_case_insensitive() {
        let words = make_words(&["Bitcoin"]);
        assert!(contains_muted_word("I love BITCOIN", &[], &words));
        assert!(contains_muted_word("I love bitcoin", &[], &words));
        assert!(contains_muted_word("I love Bitcoin", &[], &words));
    }

    #[test]
    fn test_whole_word_only() {
        let words = make_words(&["bit"]);
        assert!(!contains_muted_word("bitcoin is great", &[], &words));
        assert!(contains_muted_word("a bit of text", &[], &words));
    }

    #[test]
    fn test_whole_word_at_start() {
        let words = make_words(&["spam"]);
        assert!(contains_muted_word("spam is here", &[], &words));
    }

    #[test]
    fn test_whole_word_at_end() {
        let words = make_words(&["spam"]);
        assert!(contains_muted_word("here is spam", &[], &words));
    }

    #[test]
    fn test_whole_word_punctuation() {
        let words = make_words(&["spam"]);
        assert!(contains_muted_word("spam, eggs", &[], &words));
        assert!(contains_muted_word("spam.", &[], &words));
        assert!(contains_muted_word("(spam)", &[], &words));
    }

    #[test]
    fn test_no_match() {
        let words = make_words(&["bitcoin"]);
        assert!(!contains_muted_word("I love cats", &[], &words));
    }

    #[test]
    fn test_hashtag_match() {
        let words = make_words(&["bitcoin"]);
        let hashtags = vec!["bitcoin".to_string()];
        assert!(contains_muted_word("check this out", &hashtags, &words));
    }

    #[test]
    fn test_hashtag_case_insensitive() {
        let words = make_words(&["Bitcoin"]);
        let hashtags = vec!["bitcoin".to_string()];
        assert!(contains_muted_word("check this out", &hashtags, &words));
    }

    #[test]
    fn test_multiple_muted_words() {
        let words = make_words(&["spam", "scam"]);
        assert!(contains_muted_word("this is spam", &[], &words));
        assert!(contains_muted_word("this is a scam", &[], &words));
        assert!(!contains_muted_word("this is fine", &[], &words));
    }

    #[test]
    fn test_empty_word_skipped() {
        let mut words = HashSet::new();
        words.insert("".to_string());
        words.insert("spam".to_string());
        assert!(contains_muted_word("spam here", &[], &words));
    }

    #[test]
    fn test_unicode_content() {
        let words = make_words(&["spam"]);
        assert!(contains_muted_word("spam 日本語", &[], &words));
    }

    #[test]
    fn test_multi_word_phrase() {
        let words = make_words(&["foo bar"]);
        assert!(contains_muted_word("test foo bar baz", &[], &words));
    }

    #[test]
    fn test_whole_word_does_not_match_substring() {
        let words = make_words(&["ass"]);
        assert!(!contains_muted_word("assertion failed", &[], &words));
    }

    #[test]
    fn test_multibyte_word_not_whole_match_no_panic() {
        // "日本" found in "x日本" — not a whole-word match (preceded by 'x').
        // Previously: panicked because start=match_start+1 landed mid-character.
        let words = make_words(&["日本"]);
        assert!(!contains_muted_word("x日本", &[], &words));
    }

    #[test]
    fn test_multibyte_word_whole_match() {
        let words = make_words(&["日本"]);
        assert!(contains_muted_word("hello 日本 world", &[], &words));
    }

    #[test]
    fn test_accented_word_no_panic() {
        let words = make_words(&["café"]);
        assert!(!contains_muted_word("xcafé here", &[], &words));
        assert!(contains_muted_word("drinking café now", &[], &words));
    }

    #[test]
    fn test_emoji_word_no_panic() {
        let words = make_words(&["🦀"]);
        assert!(!contains_muted_word("x🦀", &[], &words));
        assert!(contains_muted_word("love 🦀 rust", &[], &words));
    }
}
