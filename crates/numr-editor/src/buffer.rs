//! Text buffer management
//!
//! Provides a reusable text buffer with cursor tracking, independent of any UI framework.

/// A text buffer with cursor position tracking
#[derive(Debug, Clone)]
pub struct TextBuffer {
    /// Lines of text
    pub lines: Vec<String>,
    /// Cursor column (character index, not byte)
    pub cursor_x: usize,
    /// Cursor row (line index)
    pub cursor_y: usize,
    /// Whether the buffer has unsaved changes
    pub dirty: bool,
}

impl Default for TextBuffer {
    fn default() -> Self {
        Self {
            lines: vec![String::new()],
            cursor_x: 0,
            cursor_y: 0,
            dirty: false,
        }
    }
}

impl TextBuffer {
    /// Create a new empty buffer
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a buffer from content
    pub fn from_content(content: &str) -> Self {
        let lines: Vec<String> = content.lines().map(String::from).collect();
        Self {
            lines: if lines.is_empty() {
                vec![String::new()]
            } else {
                lines
            },
            cursor_x: 0,
            cursor_y: 0,
            dirty: false,
        }
    }

    /// Get the current line
    pub fn current_line(&self) -> &str {
        &self.lines[self.cursor_y]
    }

    /// Get character count of current line
    pub fn current_line_len(&self) -> usize {
        self.lines[self.cursor_y].chars().count()
    }

    /// Get total line count
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// Get content as a single string
    pub fn content(&self) -> String {
        self.lines.join("\n")
    }

    /// Insert a character at cursor position
    pub fn insert_char(&mut self, c: char) {
        let byte_idx = char_to_byte_idx(&self.lines[self.cursor_y], self.cursor_x);
        self.lines[self.cursor_y].insert(byte_idx, c);
        self.cursor_x += 1;
        self.dirty = true;
    }

    /// Delete character before cursor (backspace)
    pub fn delete_char(&mut self) {
        if self.cursor_x > 0 {
            let byte_idx = char_to_byte_idx(&self.lines[self.cursor_y], self.cursor_x - 1);
            self.lines[self.cursor_y].remove(byte_idx);
            self.cursor_x -= 1;
            self.dirty = true;
        } else if self.cursor_y > 0 {
            // Join with previous line
            let current_line = self.lines.remove(self.cursor_y);
            self.cursor_y -= 1;
            self.cursor_x = self.lines[self.cursor_y].chars().count();
            self.lines[self.cursor_y].push_str(&current_line);
            self.dirty = true;
        }
    }

    /// Delete character at cursor (delete key)
    pub fn delete_char_forward(&mut self) {
        let line_len = self.current_line_len();
        if self.cursor_x < line_len {
            let byte_idx = char_to_byte_idx(&self.lines[self.cursor_y], self.cursor_x);
            self.lines[self.cursor_y].remove(byte_idx);
            self.dirty = true;
        } else if self.cursor_y < self.lines.len() - 1 {
            // Join with next line
            let next_line = self.lines.remove(self.cursor_y + 1);
            self.lines[self.cursor_y].push_str(&next_line);
            self.dirty = true;
        }
    }

    /// Delete entire current line
    pub fn delete_line(&mut self) {
        if self.lines.len() > 1 {
            self.lines.remove(self.cursor_y);
            if self.cursor_y >= self.lines.len() {
                self.cursor_y = self.lines.len() - 1;
            }
        } else {
            self.lines[0].clear();
        }
        self.cursor_x = self.cursor_x.min(self.current_line_len());
        self.dirty = true;
    }

    /// Insert a new line at cursor
    pub fn new_line(&mut self) {
        let byte_idx = char_to_byte_idx(&self.lines[self.cursor_y], self.cursor_x);
        let remainder = self.lines[self.cursor_y].split_off(byte_idx);
        self.cursor_y += 1;
        self.lines.insert(self.cursor_y, remainder);
        self.cursor_x = 0;
        self.dirty = true;
    }

    /// Move cursor up
    pub fn move_up(&mut self) {
        if self.cursor_y > 0 {
            self.cursor_y -= 1;
            self.cursor_x = self.cursor_x.min(self.current_line_len());
        }
    }

    /// Move cursor down
    pub fn move_down(&mut self) {
        if self.cursor_y < self.lines.len() - 1 {
            self.cursor_y += 1;
            self.cursor_x = self.cursor_x.min(self.current_line_len());
        }
    }

    /// Move cursor left
    pub fn move_left(&mut self) {
        if self.cursor_x > 0 {
            self.cursor_x -= 1;
        } else if self.cursor_y > 0 {
            self.cursor_y -= 1;
            self.cursor_x = self.current_line_len();
        }
    }

    /// Move cursor right
    pub fn move_right(&mut self) {
        let line_len = self.current_line_len();
        if self.cursor_x < line_len {
            self.cursor_x += 1;
        } else if self.cursor_y < self.lines.len() - 1 {
            self.cursor_y += 1;
            self.cursor_x = 0;
        }
    }

    /// Move cursor to start of line
    pub fn move_to_line_start(&mut self) {
        self.cursor_x = 0;
    }

    /// Move cursor to end of line
    pub fn move_to_line_end(&mut self) {
        self.cursor_x = self.current_line_len();
    }

    /// Move cursor to first line
    pub fn move_to_first_line(&mut self) {
        self.cursor_y = 0;
        self.cursor_x = self.cursor_x.min(self.current_line_len());
    }

    /// Move cursor to last line
    pub fn move_to_last_line(&mut self) {
        self.cursor_y = self.lines.len() - 1;
        self.cursor_x = self.cursor_x.min(self.current_line_len());
    }
}

/// Convert character index to byte index in a string
fn char_to_byte_idx(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(i, _)| i)
        .unwrap_or(s.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_char() {
        let mut buf = TextBuffer::new();
        buf.insert_char('a');
        buf.insert_char('b');
        assert_eq!(buf.current_line(), "ab");
        assert_eq!(buf.cursor_x, 2);
        assert!(buf.dirty);
    }

    #[test]
    fn test_delete_char() {
        let mut buf = TextBuffer::from_content("abc");
        buf.cursor_x = 2;
        buf.delete_char();
        assert_eq!(buf.current_line(), "ac");
        assert_eq!(buf.cursor_x, 1);
    }

    #[test]
    fn test_new_line() {
        let mut buf = TextBuffer::from_content("hello world");
        buf.cursor_x = 5;
        buf.new_line();
        assert_eq!(buf.lines.len(), 2);
        assert_eq!(buf.lines[0], "hello");
        assert_eq!(buf.lines[1], " world");
        assert_eq!(buf.cursor_y, 1);
        assert_eq!(buf.cursor_x, 0);
    }

    #[test]
    fn test_delete_line() {
        let mut buf = TextBuffer::from_content("line1\nline2\nline3");
        buf.cursor_y = 1;
        buf.delete_line();
        assert_eq!(buf.lines.len(), 2);
        assert_eq!(buf.lines[0], "line1");
        assert_eq!(buf.lines[1], "line3");
    }

    #[test]
    fn test_move_cursor() {
        let mut buf = TextBuffer::from_content("abc\ndef");
        buf.move_down();
        assert_eq!(buf.cursor_y, 1);
        buf.move_right();
        buf.move_right();
        assert_eq!(buf.cursor_x, 2);
        buf.move_up();
        assert_eq!(buf.cursor_y, 0);
        assert_eq!(buf.cursor_x, 2);
    }

    #[test]
    fn test_unicode() {
        let mut buf = TextBuffer::from_content("héllo");
        buf.cursor_x = 2;
        buf.insert_char('x');
        assert_eq!(buf.current_line(), "héxllo");
    }
}
