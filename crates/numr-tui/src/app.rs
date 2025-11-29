//! Application state and logic

use numr_core::{Engine, Value};
use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use textwrap::{wrap, Options, WordSplitter};

use std::time::Instant;

/// Application state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Insert,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FetchStatus {
    Idle,
    Fetching,
    Success,
    Error(String),
}

/// Pending command for multi-key sequences (like dd, yy)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PendingCommand {
    #[default]
    None,
    Delete, // Waiting for second 'd' to complete 'dd'
}

pub struct App {
    pub lines: Vec<String>,
    pub results: Vec<Value>,
    pub cursor_x: usize,
    pub cursor_y: usize,
    pub viewport_x: usize,      // Horizontal scroll offset
    pub viewport_y: usize,      // Vertical scroll offset
    pub viewport_width: usize,  // Visible columns count
    pub viewport_height: usize, // Visible lines count
    pub engine: Engine,
    pub mode: InputMode,
    pub pending: PendingCommand, // For multi-key commands like dd
    pub path: Option<PathBuf>,
    pub dirty: bool,
    pub debug_mode: bool,
    pub wrap_mode: bool, // Toggle text wrapping
    pub fetch_status: FetchStatus,
    pub fetch_start: Option<Instant>, // For loading animation
    pub status_message: Option<String>,
    pub status_start: Option<Instant>,
    pub show_help: bool,
    pub help_scroll: usize, // Scroll offset for help popup
    pub show_line_numbers: bool,
    pub show_header: bool,
    pub show_quit_confirmation: bool,
}

/// Convert character index to byte index in a string
fn char_to_byte_idx(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(i, _)| i)
        .unwrap_or(s.len())
}

/// Get character count of a string (not byte count)
fn char_count(s: &str) -> usize {
    s.chars().count()
}

impl App {
    pub fn new(path: Option<PathBuf>) -> Self {
        let mut app = Self {
            path,
            ..Self::default()
        };
        if let Some(p) = &app.path {
            if p.exists() {
                if let Err(e) = app.load() {
                    eprintln!("Failed to load file: {e}");
                }
            }
        }
        app
    }

    /// Load lines from the file
    pub fn load(&mut self) -> io::Result<()> {
        if let Some(path) = &self.path {
            let content = fs::read_to_string(path)?;
            self.lines = content.lines().map(String::from).collect();
            if self.lines.is_empty() {
                self.lines.push(String::new());
            }
            self.recalculate();
            self.dirty = false;
        }
        Ok(())
    }

    /// Save lines to the file
    pub fn save(&mut self) -> io::Result<()> {
        if let Some(path) = &self.path {
            // Ensure directory exists
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }

            let mut file = fs::File::create(path)?;
            for (i, line) in self.lines.iter().enumerate() {
                if i > 0 {
                    writeln!(file)?;
                }
                write!(file, "{line}")?;
            }
            self.dirty = false;
        }
        Ok(())
    }

    /// Toggle debug mode
    pub fn toggle_debug(&mut self) {
        self.debug_mode = !self.debug_mode;
    }

    /// Toggle wrap mode
    pub fn toggle_wrap(&mut self) {
        self.wrap_mode = !self.wrap_mode;
        // Reset horizontal scroll when entering wrap mode
        if self.wrap_mode {
            self.viewport_x = 0;
            // Recalculate viewport_y to ensure cursor is visible in new mode
            self.ensure_cursor_visible();
        } else {
            // Reset to line-based scrolling
            self.viewport_y = self.cursor_y.saturating_sub(self.viewport_height / 2);
            self.ensure_cursor_visible();
        }
    }

    /// Set a temporary status message
    pub fn set_status(&mut self, msg: &str) {
        self.status_message = Some(msg.to_string());
        self.status_start = Some(Instant::now());
    }

    /// Clear status message if it has expired
    /// "Saved" expires after 1.5s, others after 3s
    pub fn clear_status_if_expired(&mut self) {
        if let (Some(start), Some(msg)) = (self.status_start, &self.status_message) {
            let timeout_ms = if msg == "Saved" { 1500 } else { 3000 };
            if start.elapsed().as_millis() >= timeout_ms {
                self.status_message = None;
                self.status_start = None;
            }
        }
    }

    /// Toggle help popup
    pub fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
        if self.show_help {
            self.help_scroll = 0; // Reset scroll when opening
        }
    }

    /// Scroll help popup up
    pub fn help_scroll_up(&mut self) {
        self.help_scroll = self.help_scroll.saturating_sub(1);
    }

    /// Scroll help popup down
    pub fn help_scroll_down(&mut self, max_scroll: usize) {
        if self.help_scroll < max_scroll {
            self.help_scroll += 1;
        }
    }

    /// Toggle line numbers
    pub fn toggle_line_numbers(&mut self) {
        self.show_line_numbers = !self.show_line_numbers;
    }

    /// Toggle header visibility
    pub fn toggle_header(&mut self) {
        self.show_header = !self.show_header;
    }

    /// Page up
    pub fn page_up(&mut self) {
        let page_size = self.viewport_height.saturating_sub(1).max(1);
        if self.cursor_y > 0 {
            self.cursor_y = self.cursor_y.saturating_sub(page_size);
            self.cursor_x = self.cursor_x.min(char_count(&self.lines[self.cursor_y]));
            self.ensure_cursor_visible();
        }
    }

    /// Page down
    pub fn page_down(&mut self) {
        let page_size = self.viewport_height.saturating_sub(1).max(1);
        if self.cursor_y < self.lines.len() - 1 {
            self.cursor_y = (self.cursor_y + page_size).min(self.lines.len() - 1);
            self.cursor_x = self.cursor_x.min(char_count(&self.lines[self.cursor_y]));
            self.ensure_cursor_visible();
        }
    }

    /// Insert a character at cursor position
    pub fn insert_char(&mut self, c: char) {
        let (line, char_col) = (self.cursor_y, self.cursor_x);
        if line < self.lines.len() {
            let byte_idx = char_to_byte_idx(&self.lines[line], char_col);
            self.lines[line].insert(byte_idx, c);
            self.cursor_x += 1;
            self.recalculate();
        }
    }

    /// Delete character before cursor
    pub fn delete_char(&mut self) {
        let (line, char_col) = (self.cursor_y, self.cursor_x);
        if char_col > 0 && line < self.lines.len() {
            let byte_idx = char_to_byte_idx(&self.lines[line], char_col - 1);
            self.lines[line].remove(byte_idx);
            self.cursor_x -= 1;
            self.recalculate();
        } else if char_col == 0 && line > 0 {
            // Merge with previous line
            let current_line = self.lines.remove(line);
            self.results.remove(line);
            let prev_char_len = char_count(&self.lines[line - 1]);
            self.lines[line - 1].push_str(&current_line);
            self.cursor_y = line - 1;
            self.cursor_x = prev_char_len;
            self.recalculate();
        }
    }

    /// Delete character after cursor
    pub fn delete_char_forward(&mut self) {
        let (line, char_col) = (self.cursor_y, self.cursor_x);
        let line_char_len = char_count(&self.lines[line]);
        if line < self.lines.len() && char_col < line_char_len {
            let byte_idx = char_to_byte_idx(&self.lines[line], char_col);
            self.lines[line].remove(byte_idx);
            self.recalculate();
        } else if char_col == line_char_len && line < self.lines.len() - 1 {
            // Merge with next line
            let next_line = self.lines.remove(line + 1);
            self.results.remove(line + 1);
            self.lines[line].push_str(&next_line);
            self.recalculate();
        }
    }

    /// Delete the current line
    pub fn delete_line(&mut self) {
        let line = self.cursor_y;
        if self.lines.len() > 1 {
            self.lines.remove(line);
            self.results.remove(line);
            if line >= self.lines.len() {
                self.cursor_y = self.lines.len() - 1;
            }
            self.cursor_x = 0; // Reset col
            self.recalculate();
        } else {
            // If only one line, just clear it
            self.lines[0].clear();
            self.results[0] = Value::Empty;
            self.cursor_x = 0;
            self.recalculate();
        }
    }

    /// Insert a new line
    pub fn new_line(&mut self) {
        let (line, char_col) = (self.cursor_y, self.cursor_x);
        if line < self.lines.len() {
            let byte_idx = char_to_byte_idx(&self.lines[line], char_col);
            let remainder = self.lines[line].split_off(byte_idx);
            self.lines.insert(line + 1, remainder);
            self.results.insert(line + 1, Value::Empty);
            self.cursor_y = line + 1;
            self.cursor_x = 0;
            self.recalculate();
        }
    }

    /// Move cursor up
    pub fn move_up(&mut self) {
        if self.cursor_y > 0 {
            self.cursor_y -= 1;
            self.cursor_x = self.cursor_x.min(char_count(&self.lines[self.cursor_y]));
            self.ensure_cursor_visible();
        }
    }

    /// Move cursor down
    pub fn move_down(&mut self) {
        if self.cursor_y < self.lines.len() - 1 {
            self.cursor_y += 1;
            self.cursor_x = self.cursor_x.min(char_count(&self.lines[self.cursor_y]));
            self.ensure_cursor_visible();
        }
    }

    /// Calculate wrapped height of a line
    pub fn get_wrapped_height(&self, text: &str) -> usize {
        if text.is_empty() || self.viewport_width == 0 {
            return 1;
        }
        let options = Options::new(self.viewport_width)
            .break_words(true)
            .word_splitter(WordSplitter::NoHyphenation);
        wrap(text, options).len().max(1)
    }

    /// Get the visual row index of the cursor (0-indexed global)
    pub fn get_cursor_visual_row(&self) -> usize {
        let mut visual_row = 0;
        for (i, line) in self.lines.iter().enumerate() {
            if i == self.cursor_y {
                let options = Options::new(self.viewport_width)
                    .break_words(true)
                    .word_splitter(WordSplitter::NoHyphenation);
                let wrapped = wrap(line, options);

                if wrapped.is_empty() {
                    return visual_row;
                }

                let mut current_len = 0;
                for (idx, part) in wrapped.iter().enumerate() {
                    let part_len = part.chars().count();
                    // If cursor is within this part (inclusive of end)
                    if self.cursor_x <= current_len + part_len {
                        return visual_row + idx;
                    }
                    current_len += part_len;
                }
                return visual_row + wrapped.len().saturating_sub(1);
            }
            visual_row += self.get_wrapped_height(line);
        }
        visual_row
    }

    /// Get cursor position within wrapped line: (row_offset_within_line, x_position)
    /// row_offset_within_line: which visual row within the current line (0 = first row)
    /// x_position: character position within that wrapped row
    pub fn get_cursor_wrapped_position(&self) -> (usize, usize) {
        if self.viewport_width == 0 {
            return (0, self.cursor_x);
        }

        let line = &self.lines[self.cursor_y];
        let options = Options::new(self.viewport_width)
            .break_words(true)
            .word_splitter(WordSplitter::NoHyphenation);
        let wrapped = wrap(line, options);

        if wrapped.is_empty() {
            return (0, self.cursor_x);
        }

        let mut current_len = 0;
        for (idx, part) in wrapped.iter().enumerate() {
            let part_len = part.chars().count();
            if self.cursor_x <= current_len + part_len {
                return (idx, self.cursor_x - current_len);
            }
            current_len += part_len;
        }
        // Cursor is past the end
        let last_row = wrapped.len().saturating_sub(1);
        let last_len = wrapped.last().map(|s| s.chars().count()).unwrap_or(0);
        (last_row, last_len)
    }

    /// Ensure cursor is visible in viewport (both vertical and horizontal)
    pub fn ensure_cursor_visible(&mut self) {
        if self.wrap_mode {
            let visual_row = self.get_cursor_visual_row();

            // Vertical scrolling (visual rows)
            if visual_row < self.viewport_y {
                self.viewport_y = visual_row;
            } else if visual_row >= self.viewport_y + self.viewport_height {
                self.viewport_y = visual_row.saturating_sub(self.viewport_height - 1);
            }
            // No horizontal scrolling in wrap mode
        } else {
            // Vertical scrolling (lines)
            if self.cursor_y < self.viewport_y {
                self.viewport_y = self.cursor_y;
            } else if self.cursor_y >= self.viewport_y + self.viewport_height {
                self.viewport_y = self.cursor_y.saturating_sub(self.viewport_height - 1);
            }

            // Horizontal scrolling (keep some margin)
            let margin = 5.min(self.viewport_width / 4);
            if self.cursor_x < self.viewport_x + margin {
                self.viewport_x = self.cursor_x.saturating_sub(margin);
            } else if self.cursor_x >= self.viewport_x + self.viewport_width.saturating_sub(margin)
            {
                self.viewport_x = self
                    .cursor_x
                    .saturating_sub(self.viewport_width.saturating_sub(margin + 1));
            }
        }
    }

    /// Move cursor left
    pub fn move_left(&mut self) {
        if self.cursor_x > 0 {
            self.cursor_x -= 1;
        } else if self.cursor_y > 0 {
            self.cursor_y -= 1;
            self.cursor_x = char_count(&self.lines[self.cursor_y]);
        }
        self.ensure_cursor_visible();
    }

    /// Move cursor right
    pub fn move_right(&mut self) {
        let (line, char_col) = (self.cursor_y, self.cursor_x);
        let line_char_len = char_count(&self.lines[line]);
        if char_col < line_char_len {
            self.cursor_x += 1;
        } else if line < self.lines.len() - 1 {
            self.cursor_y += 1;
            self.cursor_x = 0;
        }
        self.ensure_cursor_visible();
    }

    /// Move to start of current line
    pub fn move_to_line_start(&mut self) {
        self.cursor_x = 0;
        self.ensure_cursor_visible();
    }

    /// Move to end of current line
    pub fn move_to_line_end(&mut self) {
        self.cursor_x = char_count(&self.lines[self.cursor_y]);
        self.ensure_cursor_visible();
    }

    /// Get totals grouped by type (currency, unit, etc.)
    pub fn grouped_totals(&self) -> Vec<Value> {
        self.engine.grouped_totals()
    }

    /// Get errors for the current line (for debug panel)
    pub fn current_line_error(&self) -> Option<&str> {
        let line_idx = self.cursor_y;
        if let Some(Value::Error(msg)) = self.results.get(line_idx) {
            Some(msg.as_str())
        } else {
            None
        }
    }

    /// Update exchange rates and save to cache
    pub fn update_rates(&mut self, rates: Result<HashMap<String, f64>, String>) {
        match rates {
            Ok(raw_rates) => {
                // Apply rates to engine
                self.engine.apply_raw_rates(&raw_rates);
                // Save to file cache for CLI and future use
                self.engine.save_rates_to_cache(&raw_rates);
                self.fetch_status = FetchStatus::Success;
            }
            Err(e) => {
                self.fetch_status = FetchStatus::Error(e);
            }
        }
        // Re-evaluate all lines with new rates
        self.recalculate();
    }

    /// Recalculate all results
    pub fn recalculate(&mut self) {
        self.dirty = true;
        self.engine.clear();
        self.results.clear();

        for line in &self.lines {
            let value = if line.trim().is_empty() {
                Value::Empty
            } else {
                self.engine.eval(line)
            };
            self.results.push(value);
        }
    }
}

impl Default for App {
    fn default() -> Self {
        let mut app = Self {
            lines: vec![String::new()],
            results: vec![Value::Empty],
            cursor_x: 0,
            cursor_y: 0,
            viewport_x: 0,
            viewport_y: 0,
            viewport_width: 80,  // Will be updated by UI
            viewport_height: 20, // Will be updated by UI
            engine: Engine::new(),
            mode: InputMode::Normal,
            pending: PendingCommand::None,
            path: None,
            dirty: false,
            debug_mode: false,
            wrap_mode: false,
            fetch_status: FetchStatus::Idle,
            fetch_start: None,
            status_message: None,
            status_start: None,
            show_help: false,
            help_scroll: 0,
            show_line_numbers: false,
            show_header: false,
            show_quit_confirmation: false,
        };
        app.recalculate();
        app
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_wrapped_height() {
        // "hello world" (11 chars)
        // width 5: "hello" + "world" -> 2 lines
        let app = App {
            viewport_width: 5,
            ..Default::default()
        };
        assert_eq!(app.get_wrapped_height("hello world"), 2);

        // width 11: "hello world" -> 1 line
        let app = App {
            viewport_width: 11,
            ..Default::default()
        };
        assert_eq!(app.get_wrapped_height("hello world"), 1);

        // width 3: splits into 4+ lines
        let app = App {
            viewport_width: 3,
            ..Default::default()
        };
        assert!(app.get_wrapped_height("hello world") >= 2);
    }

    #[test]
    fn test_get_cursor_wrapped_position() {
        // "hello world" with width 6 wraps to:
        // Row 0: "hello " (6 chars)
        // Row 1: "world" (5 chars)
        let mut app = App {
            viewport_width: 6,
            lines: vec!["hello world".to_string()],
            cursor_y: 0,
            cursor_x: 0,
            ..Default::default()
        };

        // Cursor at start
        assert_eq!(app.get_cursor_wrapped_position(), (0, 0));

        // Cursor at position 5 (at space in "hello ")
        app.cursor_x = 5;
        assert_eq!(app.get_cursor_wrapped_position(), (0, 5));

        // Cursor at position 6 (start of "world") - should be on second row
        app.cursor_x = 6;
        let (row, _col) = app.get_cursor_wrapped_position();
        assert_eq!(row, 1, "cursor_x=6 should be on row 1");

        // Cursor at position 8 (in "world")
        app.cursor_x = 8;
        let (row, _col) = app.get_cursor_wrapped_position();
        assert_eq!(row, 1, "cursor_x=8 should be on row 1");
    }
}
