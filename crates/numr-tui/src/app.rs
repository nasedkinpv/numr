//! Application state and logic

use numr_core::{Currency, Engine, Value};
use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::str::FromStr; // Added for Currency::from_str

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
                    eprintln!("Failed to load file: {}", e);
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
                write!(file, "{}", line)?;
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

    /// Ensure cursor is visible in viewport (both vertical and horizontal)
    pub fn ensure_cursor_visible(&mut self) {
        // Vertical scrolling
        if self.cursor_y < self.viewport_y {
            self.viewport_y = self.cursor_y;
        } else if self.cursor_y >= self.viewport_y + self.viewport_height {
            self.viewport_y = self.cursor_y.saturating_sub(self.viewport_height - 1);
        }

        // Horizontal scrolling (keep some margin)
        let margin = 5.min(self.viewport_width / 4);
        if self.cursor_x < self.viewport_x + margin {
            self.viewport_x = self.cursor_x.saturating_sub(margin);
        } else if self.cursor_x >= self.viewport_x + self.viewport_width.saturating_sub(margin) {
            self.viewport_x = self
                .cursor_x
                .saturating_sub(self.viewport_width.saturating_sub(margin + 1));
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

    /// Get the total sum of all results
    pub fn total(&self) -> f64 {
        self.results.iter().filter_map(|v| v.as_f64()).sum()
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

    /// Update exchange rates
    pub fn update_rates(&mut self, rates: Result<HashMap<String, f64>, String>) {
        match rates {
            Ok(rates) => {
                for (code, rate) in rates {
                    if let Ok(currency) = Currency::from_str(&code) {
                        if currency == Currency::BTC {
                            // BTC rate is "1 BTC = X USD" (from CoinGecko)
                            self.engine
                                .set_exchange_rate(Currency::BTC, Currency::USD, rate);
                        } else {
                            // Fiat rates are "1 USD = X Currency"
                            self.engine.set_exchange_rate(Currency::USD, currency, rate);
                        }
                    }
                }
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
        };
        app.recalculate();
        app
    }
}
