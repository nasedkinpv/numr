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

pub struct App {
    pub lines: Vec<String>,
    pub results: Vec<Value>,
    pub cursor_x: usize,
    pub cursor_y: usize,
    pub engine: Engine,
    pub mode: InputMode,
    pub path: Option<PathBuf>,
    pub dirty: bool,
    pub debug_mode: bool,
    pub fetch_status: FetchStatus,
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

    /// Insert a character at cursor position
    pub fn insert_char(&mut self, c: char) {
        let (line, col) = (self.cursor_y, self.cursor_x);
        if line < self.lines.len() {
            self.lines[line].insert(col, c);
            self.cursor_x += 1;
            self.recalculate();
        }
    }

    /// Delete character before cursor
    pub fn delete_char(&mut self) {
        let (line, col) = (self.cursor_y, self.cursor_x);
        if col > 0 && line < self.lines.len() {
            self.lines[line].remove(col - 1);
            self.cursor_x -= 1;
            self.recalculate();
        } else if col == 0 && line > 0 {
            // Merge with previous line
            let current_line = self.lines.remove(line);
            self.results.remove(line);
            let prev_len = self.lines[line - 1].len();
            self.lines[line - 1].push_str(&current_line);
            self.cursor_y = line - 1;
            self.cursor_x = prev_len;
            self.recalculate();
        }
    }

    /// Delete character after cursor
    pub fn delete_char_forward(&mut self) {
        let (line, col) = (self.cursor_y, self.cursor_x);
        if line < self.lines.len() && col < self.lines[line].len() {
            self.lines[line].remove(col);
            self.recalculate();
        } else if col == self.lines[line].len() && line < self.lines.len() - 1 {
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
        let (line, col) = (self.cursor_y, self.cursor_x);
        if line < self.lines.len() {
            let remainder = self.lines[line].split_off(col);
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
            self.cursor_x = self.cursor_x.min(self.lines[self.cursor_y].len());
        }
    }

    /// Move cursor down
    pub fn move_down(&mut self) {
        if self.cursor_y < self.lines.len() - 1 {
            self.cursor_y += 1;
            self.cursor_x = self.cursor_x.min(self.lines[self.cursor_y].len());
        }
    }

    /// Move cursor left
    pub fn move_left(&mut self) {
        if self.cursor_x > 0 {
            self.cursor_x -= 1;
        } else if self.cursor_y > 0 {
            self.cursor_y -= 1;
            self.cursor_x = self.lines[self.cursor_y].len();
        }
    }

    /// Move cursor right
    pub fn move_right(&mut self) {
        let (line, col) = (self.cursor_y, self.cursor_x);
        if col < self.lines[line].len() {
            self.cursor_x += 1;
        } else if line < self.lines.len() - 1 {
            self.cursor_y += 1;
            self.cursor_x = 0;
        }
    }

    /// Move to start of current line
    pub fn move_to_line_start(&mut self) {
        self.cursor_x = 0;
    }

    /// Move to end of current line
    pub fn move_to_line_end(&mut self) {
        self.cursor_x = self.lines[self.cursor_y].len();
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
                            self.engine.set_exchange_rate(Currency::BTC, Currency::USD, rate);
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
            engine: Engine::new(),
            mode: InputMode::Normal,
            path: None,
            dirty: false,
            debug_mode: false,
            fetch_status: FetchStatus::Idle,
        };
        app.recalculate();
        app
    }
}
