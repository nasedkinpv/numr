//! Small UTF-8 text helpers shared by editor frontends.

/// Convert a character index to a byte index, clamping to the end of `text`.
pub fn char_to_byte_idx(text: &str, char_idx: usize) -> usize {
    text.char_indices()
        .nth(char_idx)
        .map(|(index, _)| index)
        .unwrap_or(text.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_ascii_and_unicode_boundaries() {
        let text = "aé🧮z";
        assert_eq!(char_to_byte_idx(text, 0), 0);
        assert_eq!(char_to_byte_idx(text, 1), 1);
        assert_eq!(char_to_byte_idx(text, 2), 3);
        assert_eq!(char_to_byte_idx(text, 3), 7);
        assert_eq!(char_to_byte_idx(text, 4), text.len());
    }

    #[test]
    fn clamps_indices_past_the_end() {
        assert_eq!(char_to_byte_idx("é", usize::MAX), "é".len());
        assert_eq!(char_to_byte_idx("", 1), 0);
    }
}
