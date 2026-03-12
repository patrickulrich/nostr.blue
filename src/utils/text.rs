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
