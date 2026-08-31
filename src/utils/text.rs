/// Convert a UTF-16 code unit index from DOM APIs into a UTF-8 byte index for Rust strings.
pub fn utf16_to_utf8_index(text: &str, utf16_index: usize) -> usize {
    let mut utf16_count = 0;
    let mut utf8_byte_index = 0;
    for ch in text.chars() {
        if utf16_count >= utf16_index {
            break;
        }
        utf16_count += ch.len_utf16();
        utf8_byte_index += ch.len_utf8();
    }
    utf8_byte_index.min(text.len())
}

/// Convert a UTF-8 byte index for Rust strings into a UTF-16 code unit index for DOM APIs.
pub fn utf8_to_utf16_index(text: &str, utf8_index: usize) -> usize {
    let utf8_index = utf8_index.min(text.len());
    let mut utf16_count = 0;
    let mut utf8_byte_index = 0;
    for ch in text.chars() {
        if utf8_byte_index >= utf8_index {
            break;
        }
        utf16_count += ch.len_utf16();
        utf8_byte_index += ch.len_utf8();
    }
    utf16_count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_to_utf8_ascii() {
        assert_eq!(utf16_to_utf8_index("hello", 3), 3);
        assert_eq!(utf16_to_utf8_index("hello", 100), 5);
    }

    #[test]
    fn utf16_to_utf8_emoji() {
        // "a😀b": 😀 is 4 UTF-8 bytes / 2 UTF-16 units
        let s = "a\u{1F600}b";
        assert_eq!(utf16_to_utf8_index(s, 1), 1);
        assert_eq!(utf16_to_utf8_index(s, 3), 5); // after emoji
        assert_eq!(utf16_to_utf8_index(s, 4), 6);
    }

    #[test]
    fn utf8_to_utf16_ascii() {
        assert_eq!(utf8_to_utf16_index("hello", 3), 3);
        assert_eq!(utf8_to_utf16_index("hello", 100), 5);
    }

    #[test]
    fn utf8_to_utf16_emoji() {
        let s = "a\u{1F600}b";
        assert_eq!(utf8_to_utf16_index(s, 1), 1);
        assert_eq!(utf8_to_utf16_index(s, 5), 3);
        assert_eq!(utf8_to_utf16_index(s, 6), 4);
    }

    #[test]
    fn conversions_round_trip() {
        let s = "héllo 😀 wörld";
        for byte_idx in 0..=s.len() {
            if !s.is_char_boundary(byte_idx) {
                continue;
            }
            let utf16 = utf8_to_utf16_index(s, byte_idx);
            let back = utf16_to_utf8_index(s, utf16);
            assert_eq!(back, byte_idx);
        }
    }
}
