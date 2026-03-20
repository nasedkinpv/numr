//! Application state and logic

use crate::config::Config;
use numr_core::{Decimal, Engine, FetchConfig, Value};
use numr_editor::char_to_byte_idx;
use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;
use textwrap::{wrap, Options, WordSplitter};

// ========================================
// Status Display Constants
// ========================================

/// Status message for successful save
pub(crate) const STATUS_SAVED: &str = "Saved";

/// Timeout for "Saved" status message (milliseconds)
const STATUS_SAVED_TIMEOUT_MS: u128 = 1500;

/// Timeout for general status messages (milliseconds)
const STATUS_TIMEOUT_MS: u128 = 3000;

/// Horizontal scroll margin (keep cursor this many chars from edge)
const CURSOR_MARGIN: usize = 5;

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

/// Pending command for multi-key sequences (like dd, gg)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PendingCommand {
    #[default]
    None,
    Delete, // Waiting for second 'd' to complete 'dd'
    Go,     // Waiting for second 'g' to complete 'gg'
}

// Re-export KeybindingMode so main.rs can use it
pub use crate::config::KeybindingMode;

/// Get character count of a string (not byte count)
fn char_count(s: &str) -> usize {
    s.chars().count()
}

struct Document {
    lines: Vec<String>,
    results: Vec<Value>,
    path: Option<PathBuf>,
    dirty: bool,
    engine: Engine,
}

impl Document {
    pub fn new(path: Option<PathBuf>) -> Self {
        let mut document = Self {
            lines: vec![String::new()],
            results: vec![Value::Empty],
            path,
            dirty: false,
            engine: Engine::new(),
        };
        document.refresh_results();
        document
    }

    #[cfg(test)]
    fn from_lines(lines: Vec<String>) -> Self {
        let mut document = Self {
            lines: if lines.is_empty() {
                vec![String::new()]
            } else {
                lines
            },
            results: Vec::new(),
            path: None,
            dirty: false,
            engine: Engine::new(),
        };
        document.refresh_results();
        document
    }

    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    pub fn results(&self) -> &[Value] {
        &self.results
    }

    pub fn line(&self, index: usize) -> Option<&str> {
        self.lines.get(index).map(String::as_str)
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    pub fn line_char_len(&self, index: usize) -> usize {
        self.line(index).map(char_count).unwrap_or(0)
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn dirty(&self) -> bool {
        self.dirty
    }

    pub fn grouped_totals(&self) -> Vec<Value> {
        self.engine.grouped_totals()
    }

    pub fn current_line_error(&self, line_idx: usize) -> Option<&str> {
        if let Some(Value::Error(msg)) = self.results.get(line_idx) {
            Some(msg.as_str())
        } else {
            None
        }
    }

    pub fn load(&mut self) -> io::Result<()> {
        if let Some(path) = &self.path {
            let content = fs::read_to_string(path)?;
            self.lines = content.lines().map(String::from).collect();
            if self.lines.is_empty() {
                self.lines.push(String::new());
            }
            self.refresh_results();
            self.dirty = false;
        }
        Ok(())
    }

    pub fn save(&mut self) -> io::Result<()> {
        if let Some(path) = &self.path {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }

            let mut file = fs::File::create(path)?;
            for line in &self.lines {
                writeln!(file, "{line}")?;
            }
            self.dirty = false;
        }
        Ok(())
    }

    pub fn update_rates(&mut self, raw_rates: &HashMap<String, Decimal>) {
        self.engine.apply_raw_rates(raw_rates);
        self.engine.save_rates_to_cache(raw_rates);
        self.refresh_results();
    }

    /// Re-evaluate all lines after a user edit. Marks document as dirty.
    pub fn recalculate(&mut self) {
        self.dirty = true;
        self.recompute_results();
    }

    /// Re-evaluate all lines without marking dirty (for loads, rate updates, etc.)
    pub fn refresh_results(&mut self) {
        self.recompute_results();
    }

    pub fn insert_char(&mut self, line: usize, char_col: usize, c: char) -> bool {
        if line < self.lines.len() {
            let byte_idx = char_to_byte_idx(&self.lines[line], char_col);
            self.lines[line].insert(byte_idx, c);
            self.recalculate();
            true
        } else {
            false
        }
    }

    pub fn delete_char_before(&mut self, line: usize, char_col: usize) -> Option<(usize, usize)> {
        if char_col > 0 && line < self.lines.len() {
            let byte_idx = char_to_byte_idx(&self.lines[line], char_col - 1);
            self.lines[line].remove(byte_idx);
            self.recalculate();
            Some((line, char_col - 1))
        } else if char_col == 0 && line > 0 && line < self.lines.len() {
            let current_line = self.lines.remove(line);
            let previous_col = char_count(&self.lines[line - 1]);
            self.lines[line - 1].push_str(&current_line);
            self.recalculate();
            Some((line - 1, previous_col))
        } else {
            None
        }
    }

    pub fn delete_char_forward(&mut self, line: usize, char_col: usize) -> bool {
        if line >= self.lines.len() {
            return false;
        }

        let line_char_len = char_count(&self.lines[line]);
        if char_col < line_char_len {
            let byte_idx = char_to_byte_idx(&self.lines[line], char_col);
            self.lines[line].remove(byte_idx);
            self.recalculate();
            true
        } else if char_col == line_char_len && line < self.lines.len() - 1 {
            let next_line = self.lines.remove(line + 1);
            self.lines[line].push_str(&next_line);
            self.recalculate();
            true
        } else {
            false
        }
    }

    pub fn delete_line(&mut self, line: usize) -> usize {
        if self.lines.len() > 1 && line < self.lines.len() {
            self.lines.remove(line);
            self.recalculate();
            line.min(self.lines.len().saturating_sub(1))
        } else {
            self.lines[0].clear();
            self.recalculate();
            0
        }
    }

    pub fn new_line(&mut self, line: usize, char_col: usize) -> Option<(usize, usize)> {
        if line < self.lines.len() {
            let byte_idx = char_to_byte_idx(&self.lines[line], char_col);
            let remainder = self.lines[line].split_off(byte_idx);
            self.lines.insert(line + 1, remainder);
            self.recalculate();
            Some((line + 1, 0))
        } else {
            None
        }
    }

    pub fn delete_to_line_end(&mut self, line: usize, char_col: usize) -> bool {
        if line < self.lines.len() {
            let byte_idx = char_to_byte_idx(&self.lines[line], char_col);
            self.lines[line].truncate(byte_idx);
            self.recalculate();
            true
        } else {
            false
        }
    }

    #[cfg(test)]
    fn set_lines(&mut self, lines: Vec<String>) {
        self.lines = if lines.is_empty() {
            vec![String::new()]
        } else {
            lines
        };
        self.refresh_results();
        self.dirty = false;
    }

    // NOTE: Re-evaluates all lines from scratch on every edit. Could be optimized
    // to re-eval from the dirty line onward using engine state snapshots, but at
    // typical document sizes (<100 lines) this completes in ~1ms. Not worth the
    // complexity of incremental eval until users report actual lag.
    fn recompute_results(&mut self) {
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

impl Default for Document {
    fn default() -> Self {
        Self::new(None)
    }
}

struct ViewState {
    cursor_x: usize,
    cursor_y: usize,
    viewport_x: usize,
    viewport_y: usize,
    viewport_width: usize,
    viewport_height: usize,
}

impl ViewState {
    fn sync_after_wrap_toggle(&mut self, wrap_mode: bool, doc: &Document) {
        if wrap_mode {
            self.viewport_x = 0;
        } else {
            self.viewport_y = self.cursor_y.saturating_sub(self.viewport_height / 2);
        }
        self.ensure_cursor_visible(doc, wrap_mode);
    }

    pub fn cursor_x(&self) -> usize {
        self.cursor_x
    }

    pub fn cursor_y(&self) -> usize {
        self.cursor_y
    }

    pub fn viewport_x(&self) -> usize {
        self.viewport_x
    }

    pub fn viewport_y(&self) -> usize {
        self.viewport_y
    }

    pub fn set_viewport_size(
        &mut self,
        width: usize,
        height: usize,
        doc: &Document,
        wrap_mode: bool,
    ) {
        self.viewport_width = width;
        self.viewport_height = height;
        self.ensure_cursor_visible(doc, wrap_mode);
    }

    pub fn get_wrapped_height(&self, text: &str) -> usize {
        if text.is_empty() || self.viewport_width == 0 {
            return 1;
        }
        let options = Options::new(self.viewport_width)
            .break_words(true)
            .word_splitter(WordSplitter::NoHyphenation);
        wrap(text, options).len().max(1)
    }

    pub fn get_cursor_wrapped_position(&self, doc: &Document) -> (usize, usize) {
        if self.viewport_width == 0 {
            return (0, self.cursor_x);
        }

        let line = doc.line(self.cursor_y).unwrap_or("");
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

        let last_row = wrapped.len().saturating_sub(1);
        let last_len = wrapped.last().map(|s| s.chars().count()).unwrap_or(0);
        (last_row, last_len)
    }

    pub fn ensure_cursor_visible(&mut self, doc: &Document, wrap_mode: bool) {
        if wrap_mode {
            let visual_row = self.get_cursor_visual_row(doc);

            if visual_row < self.viewport_y {
                self.viewport_y = visual_row;
            } else if visual_row >= self.viewport_y + self.viewport_height {
                self.viewport_y = visual_row.saturating_sub(self.viewport_height.saturating_sub(1));
            }
        } else {
            if self.cursor_y < self.viewport_y {
                self.viewport_y = self.cursor_y;
            } else if self.cursor_y >= self.viewport_y + self.viewport_height {
                self.viewport_y = self
                    .cursor_y
                    .saturating_sub(self.viewport_height.saturating_sub(1));
            }

            let margin = CURSOR_MARGIN.min(self.viewport_width / 4);
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

    pub fn move_up(&mut self, doc: &Document, wrap_mode: bool) {
        if self.cursor_y > 0 {
            self.cursor_y -= 1;
            self.cursor_x = self.cursor_x.min(doc.line_char_len(self.cursor_y));
            self.ensure_cursor_visible(doc, wrap_mode);
        }
    }

    pub fn move_down(&mut self, doc: &Document, wrap_mode: bool) {
        if self.cursor_y < doc.line_count().saturating_sub(1) {
            self.cursor_y += 1;
            self.cursor_x = self.cursor_x.min(doc.line_char_len(self.cursor_y));
            self.ensure_cursor_visible(doc, wrap_mode);
        }
    }

    pub fn move_left(&mut self, doc: &Document, wrap_mode: bool) {
        if self.cursor_x > 0 {
            self.cursor_x -= 1;
        } else if self.cursor_y > 0 {
            self.cursor_y -= 1;
            self.cursor_x = doc.line_char_len(self.cursor_y);
        }
        self.ensure_cursor_visible(doc, wrap_mode);
    }

    pub fn move_right(&mut self, doc: &Document, wrap_mode: bool) {
        let line_char_len = doc.line_char_len(self.cursor_y);
        if self.cursor_x < line_char_len {
            self.cursor_x += 1;
        } else if self.cursor_y < doc.line_count().saturating_sub(1) {
            self.cursor_y += 1;
            self.cursor_x = 0;
        }
        self.ensure_cursor_visible(doc, wrap_mode);
    }

    pub fn move_to_line_start(&mut self, doc: &Document, wrap_mode: bool) {
        self.cursor_x = 0;
        self.ensure_cursor_visible(doc, wrap_mode);
    }

    pub fn move_to_line_end(&mut self, doc: &Document, wrap_mode: bool) {
        self.cursor_x = doc.line_char_len(self.cursor_y);
        self.ensure_cursor_visible(doc, wrap_mode);
    }

    pub fn move_to_first_line(&mut self, doc: &Document, wrap_mode: bool) {
        self.cursor_y = 0;
        self.cursor_x = 0;
        self.ensure_cursor_visible(doc, wrap_mode);
    }

    pub fn move_to_last_line(&mut self, doc: &Document, wrap_mode: bool) {
        self.cursor_y = doc.line_count().saturating_sub(1);
        self.cursor_x = 0;
        self.ensure_cursor_visible(doc, wrap_mode);
    }

    pub fn page_up(&mut self, doc: &Document, wrap_mode: bool) {
        let page_size = self.viewport_height.saturating_sub(1).max(1);
        if self.cursor_y > 0 {
            self.cursor_y = self.cursor_y.saturating_sub(page_size);
            self.cursor_x = self.cursor_x.min(doc.line_char_len(self.cursor_y));
            self.ensure_cursor_visible(doc, wrap_mode);
        }
    }

    pub fn page_down(&mut self, doc: &Document, wrap_mode: bool) {
        let page_size = self.viewport_height.saturating_sub(1).max(1);
        if self.cursor_y < doc.line_count().saturating_sub(1) {
            self.cursor_y = (self.cursor_y + page_size).min(doc.line_count().saturating_sub(1));
            self.cursor_x = self.cursor_x.min(doc.line_char_len(self.cursor_y));
            self.ensure_cursor_visible(doc, wrap_mode);
        }
    }

    pub fn move_word_forward(&mut self, doc: &Document, wrap_mode: bool) {
        let line = doc.line(self.cursor_y).unwrap_or("");
        let chars: Vec<char> = line.chars().collect();
        let len = chars.len();

        if self.cursor_x >= len {
            if self.cursor_y < doc.line_count().saturating_sub(1) {
                self.cursor_y += 1;
                self.cursor_x = 0;
                let next_line: Vec<char> = doc.line(self.cursor_y).unwrap_or("").chars().collect();
                while self.cursor_x < next_line.len() && next_line[self.cursor_x].is_whitespace() {
                    self.cursor_x += 1;
                }
            }
            self.ensure_cursor_visible(doc, wrap_mode);
            return;
        }

        let mut pos = self.cursor_x;
        while pos < len && !chars[pos].is_whitespace() {
            pos += 1;
        }
        while pos < len && chars[pos].is_whitespace() {
            pos += 1;
        }

        if pos >= len && self.cursor_y < doc.line_count().saturating_sub(1) {
            self.cursor_y += 1;
            self.cursor_x = 0;
            let next_line: Vec<char> = doc.line(self.cursor_y).unwrap_or("").chars().collect();
            while self.cursor_x < next_line.len() && next_line[self.cursor_x].is_whitespace() {
                self.cursor_x += 1;
            }
        } else {
            self.cursor_x = pos.min(len);
        }
        self.ensure_cursor_visible(doc, wrap_mode);
    }

    pub fn move_word_backward(&mut self, doc: &Document, wrap_mode: bool) {
        if self.cursor_x == 0 {
            if self.cursor_y > 0 {
                self.cursor_y -= 1;
                self.cursor_x = doc.line_char_len(self.cursor_y);
            } else {
                return;
            }
        }

        let line = doc.line(self.cursor_y).unwrap_or("");
        let chars: Vec<char> = line.chars().collect();

        if chars.is_empty() {
            self.cursor_x = 0;
            self.ensure_cursor_visible(doc, wrap_mode);
            return;
        }

        let mut pos = self.cursor_x.saturating_sub(1);
        while pos > 0 && chars[pos].is_whitespace() {
            pos -= 1;
        }
        while pos > 0 && !chars[pos - 1].is_whitespace() {
            pos -= 1;
        }

        self.cursor_x = pos;
        self.ensure_cursor_visible(doc, wrap_mode);
    }

    pub fn move_word_end(&mut self, doc: &Document, wrap_mode: bool) {
        let line = doc.line(self.cursor_y).unwrap_or("");
        let chars: Vec<char> = line.chars().collect();
        let len = chars.len();

        if self.cursor_x >= len.saturating_sub(1) {
            if self.cursor_y < doc.line_count().saturating_sub(1) {
                self.cursor_y += 1;
                self.cursor_x = 0;
                let next_line: Vec<char> = doc.line(self.cursor_y).unwrap_or("").chars().collect();
                while self.cursor_x < next_line.len() && next_line[self.cursor_x].is_whitespace() {
                    self.cursor_x += 1;
                }
                while self.cursor_x < next_line.len().saturating_sub(1)
                    && !next_line[self.cursor_x + 1].is_whitespace()
                {
                    self.cursor_x += 1;
                }
            }
            self.ensure_cursor_visible(doc, wrap_mode);
            return;
        }

        let mut pos = self.cursor_x + 1;
        while pos < len && chars[pos].is_whitespace() {
            pos += 1;
        }
        while pos < len.saturating_sub(1) && !chars[pos + 1].is_whitespace() {
            pos += 1;
        }

        self.cursor_x = pos.min(len.saturating_sub(1));
        self.ensure_cursor_visible(doc, wrap_mode);
    }

    fn get_cursor_visual_row(&self, doc: &Document) -> usize {
        let mut visual_row = 0;
        for (i, line) in doc.lines().iter().enumerate() {
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
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            cursor_x: 0,
            cursor_y: 0,
            viewport_x: 0,
            viewport_y: 0,
            viewport_width: 80,
            viewport_height: 20,
        }
    }
}

pub struct App {
    document: Document,
    view: ViewState,
    pub mode: InputMode,
    pub keybinding_mode: KeybindingMode,
    pub pending: PendingCommand,
    pub debug_mode: bool,
    pub wrap_mode: bool,
    pub fetch_status: FetchStatus,
    pub fetch_start: Option<Instant>,
    pub status_message: Option<String>,
    pub status_start: Option<Instant>,
    pub show_help: bool,
    pub help_scroll: usize,
    pub show_line_numbers: bool,
    pub show_header: bool,
    pub show_quit_confirmation: bool,
    config: Config,
}

impl App {
    pub fn new(path: Option<PathBuf>, config: Config) -> Self {
        let keybinding_mode = config.preferences.keybinding_mode;
        let mode = match keybinding_mode {
            KeybindingMode::Standard => InputMode::Insert,
            KeybindingMode::Vim => InputMode::Normal,
        };
        let wrap_mode = config.preferences.wrap_mode;
        let show_line_numbers = config.preferences.show_line_numbers;
        let show_header = config.preferences.show_header;
        let debug_mode = config.preferences.debug_mode;

        let mut app = Self {
            document: Document::new(path),
            config,
            keybinding_mode,
            mode,
            wrap_mode,
            show_line_numbers,
            show_header,
            debug_mode,
            ..Self::default()
        };

        if let Some(path) = app.document.path() {
            if path.exists() {
                if let Err(_e) = app.load() {
                    app.set_status("Load failed");
                }
            }
        }
        app
    }

    pub fn lines(&self) -> &[String] {
        self.document.lines()
    }

    pub fn results(&self) -> &[Value] {
        self.document.results()
    }

    pub fn path(&self) -> Option<&Path> {
        self.document.path()
    }

    pub fn is_dirty(&self) -> bool {
        self.document.dirty()
    }

    pub fn cursor_x(&self) -> usize {
        self.view.cursor_x()
    }

    pub fn cursor_y(&self) -> usize {
        self.view.cursor_y()
    }

    pub fn viewport_x(&self) -> usize {
        self.view.viewport_x()
    }

    pub fn viewport_y(&self) -> usize {
        self.view.viewport_y()
    }

    #[cfg(test)]
    pub(crate) fn set_lines_for_test(&mut self, lines: Vec<String>) {
        self.document.set_lines(lines);
    }

    /// Load lines from the file
    pub fn load(&mut self) -> io::Result<()> {
        self.document.load()
    }

    /// Save lines to the file
    pub fn save(&mut self) -> io::Result<()> {
        self.document.save()
    }

    /// Save current preferences to config file
    fn save_config(&mut self) {
        self.config.preferences.keybinding_mode = self.keybinding_mode;
        self.config.preferences.wrap_mode = self.wrap_mode;
        self.config.preferences.show_line_numbers = self.show_line_numbers;
        self.config.preferences.show_header = self.show_header;
        self.config.preferences.debug_mode = self.debug_mode;
        if let Err(_e) = self.config.save() {
            self.set_status("Config error");
        }
    }

    /// Build fetch configuration for exchange-rate APIs from persisted settings.
    pub fn fetch_config(&self) -> FetchConfig {
        (&self.config.api).into()
    }

    /// Update viewport dimensions and keep the cursor visible within them.
    pub fn set_viewport_size(&mut self, width: usize, height: usize) {
        self.view
            .set_viewport_size(width, height, &self.document, self.wrap_mode);
    }

    /// Toggle debug mode
    pub fn toggle_debug(&mut self) {
        self.debug_mode = !self.debug_mode;
        self.save_config();
    }

    /// Toggle wrap mode
    pub fn toggle_wrap(&mut self) {
        self.wrap_mode = !self.wrap_mode;
        self.view
            .sync_after_wrap_toggle(self.wrap_mode, &self.document);
        self.save_config();
    }

    /// Set a temporary status message
    pub fn set_status(&mut self, msg: &str) {
        self.status_message = Some(msg.to_string());
        self.status_start = Some(Instant::now());
    }

    /// Clear status message if it has expired
    /// "Saved" expires after STATUS_SAVED_TIMEOUT_MS, others after STATUS_TIMEOUT_MS
    pub fn clear_status_if_expired(&mut self) {
        if let (Some(start), Some(msg)) = (self.status_start, &self.status_message) {
            let timeout_ms = if msg == STATUS_SAVED {
                STATUS_SAVED_TIMEOUT_MS
            } else {
                STATUS_TIMEOUT_MS
            };
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
            self.help_scroll = 0;
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
        self.save_config();
    }

    /// Toggle header visibility
    pub fn toggle_header(&mut self) {
        self.show_header = !self.show_header;
        self.save_config();
    }

    /// Page up
    pub fn page_up(&mut self) {
        self.view.page_up(&self.document, self.wrap_mode);
    }

    /// Page down
    pub fn page_down(&mut self) {
        self.view.page_down(&self.document, self.wrap_mode);
    }

    /// Insert a character at cursor position
    pub fn insert_char(&mut self, c: char) {
        let line = self.view.cursor_y;
        let char_col = self.view.cursor_x;
        if self.document.insert_char(line, char_col, c) {
            self.view.cursor_x += 1;
            self.view
                .ensure_cursor_visible(&self.document, self.wrap_mode);
        }
    }

    /// Delete character before cursor
    pub fn delete_char(&mut self) {
        let line = self.view.cursor_y;
        let char_col = self.view.cursor_x;
        if let Some((new_y, new_x)) = self.document.delete_char_before(line, char_col) {
            self.view.cursor_y = new_y;
            self.view.cursor_x = new_x;
            self.view
                .ensure_cursor_visible(&self.document, self.wrap_mode);
        }
    }

    /// Delete character after cursor
    pub fn delete_char_forward(&mut self) {
        if self
            .document
            .delete_char_forward(self.view.cursor_y, self.view.cursor_x)
        {
            self.view
                .ensure_cursor_visible(&self.document, self.wrap_mode);
        }
    }

    /// Delete the current line
    pub fn delete_line(&mut self) {
        self.view.cursor_y = self.document.delete_line(self.view.cursor_y);
        self.view.cursor_x = 0;
        self.view
            .ensure_cursor_visible(&self.document, self.wrap_mode);
    }

    /// Insert a new line
    pub fn new_line(&mut self) {
        if let Some((new_y, new_x)) = self
            .document
            .new_line(self.view.cursor_y, self.view.cursor_x)
        {
            self.view.cursor_y = new_y;
            self.view.cursor_x = new_x;
            self.view
                .ensure_cursor_visible(&self.document, self.wrap_mode);
        }
    }

    /// Move cursor up
    pub fn move_up(&mut self) {
        self.view.move_up(&self.document, self.wrap_mode);
    }

    /// Move cursor down
    pub fn move_down(&mut self) {
        self.view.move_down(&self.document, self.wrap_mode);
    }

    /// Calculate wrapped height of a line
    pub fn get_wrapped_height(&self, text: &str) -> usize {
        self.view.get_wrapped_height(text)
    }

    /// Get cursor position within wrapped line: (row_offset_within_line, x_position)
    pub fn get_cursor_wrapped_position(&self) -> (usize, usize) {
        self.view.get_cursor_wrapped_position(&self.document)
    }

    /// Move cursor left
    pub fn move_left(&mut self) {
        self.view.move_left(&self.document, self.wrap_mode);
    }

    /// Move cursor right
    pub fn move_right(&mut self) {
        self.view.move_right(&self.document, self.wrap_mode);
    }

    /// Move to start of current line
    pub fn move_to_line_start(&mut self) {
        self.view.move_to_line_start(&self.document, self.wrap_mode);
    }

    /// Move to end of current line
    pub fn move_to_line_end(&mut self) {
        self.view.move_to_line_end(&self.document, self.wrap_mode);
    }

    /// Move to first line (gg in vim)
    pub fn move_to_first_line(&mut self) {
        self.view.move_to_first_line(&self.document, self.wrap_mode);
    }

    /// Move to last line (G in vim)
    pub fn move_to_last_line(&mut self) {
        self.view.move_to_last_line(&self.document, self.wrap_mode);
    }

    /// Delete from cursor to end of line (D in vim)
    pub fn delete_to_line_end(&mut self) {
        if self
            .document
            .delete_to_line_end(self.view.cursor_y, self.view.cursor_x)
        {
            self.view
                .ensure_cursor_visible(&self.document, self.wrap_mode);
        }
    }

    /// Move to next word start (w in vim)
    pub fn move_word_forward(&mut self) {
        self.view.move_word_forward(&self.document, self.wrap_mode);
    }

    /// Move to previous word start (b in vim)
    pub fn move_word_backward(&mut self) {
        self.view.move_word_backward(&self.document, self.wrap_mode);
    }

    /// Move to end of word (e in vim)
    pub fn move_word_end(&mut self) {
        self.view.move_word_end(&self.document, self.wrap_mode);
    }

    /// Join the current line with the next line, inserting a single space when needed.
    pub fn join_with_next_line(&mut self) {
        self.move_to_line_end();
        let join_col = self.view.cursor_x;
        self.delete_char_forward();

        let line = self.document.line(self.view.cursor_y).unwrap_or("");
        let char_before = join_col.checked_sub(1).and_then(|i| line.chars().nth(i));
        let char_after = line.chars().nth(join_col);
        let needs_space =
            char_before.is_some_and(|c| c != ' ') && char_after.is_some_and(|c| c != ' ');
        if needs_space {
            self.insert_char(' ');
        }
    }

    /// Toggle keybinding mode between Vim and Standard
    pub fn toggle_keybinding_mode(&mut self) {
        self.keybinding_mode = match self.keybinding_mode {
            KeybindingMode::Vim => {
                self.mode = InputMode::Insert;
                KeybindingMode::Standard
            }
            KeybindingMode::Standard => {
                self.mode = InputMode::Normal;
                KeybindingMode::Vim
            }
        };
        self.save_config();
    }

    /// Get totals grouped by type (currency, unit, etc.)
    pub fn grouped_totals(&self) -> Vec<Value> {
        self.document.grouped_totals()
    }

    /// Get errors for the current line (for debug panel)
    pub fn current_line_error(&self) -> Option<&str> {
        self.document.current_line_error(self.view.cursor_y)
    }

    /// Update exchange rates and save to cache
    pub fn update_rates(&mut self, result: Result<numr_core::FetchResult, String>) {
        match result {
            Ok(fetch_result) => {
                self.document.update_rates(&fetch_result.rates);
                self.fetch_status = FetchStatus::Success;
                if fetch_result.warning.is_some() {
                    self.set_status("Rates partial");
                }
            }
            Err(e) => {
                self.fetch_status = FetchStatus::Error(e);
            }
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self {
            document: Document::default(),
            view: ViewState::default(),
            mode: InputMode::Normal,
            keybinding_mode: KeybindingMode::Vim,
            pending: PendingCommand::None,
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
            config: Config::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_wrapped_height() {
        let mut app = App::default();
        app.set_viewport_size(5, 20);
        assert_eq!(app.get_wrapped_height("hello world"), 2);

        app.set_viewport_size(11, 20);
        assert_eq!(app.get_wrapped_height("hello world"), 1);

        app.set_viewport_size(3, 20);
        assert!(app.get_wrapped_height("hello world") >= 2);
    }

    #[test]
    fn test_get_cursor_wrapped_position() {
        let mut app = App {
            document: Document::from_lines(vec!["hello world".to_string()]),
            ..Default::default()
        };
        app.set_viewport_size(6, 20);

        assert_eq!(app.get_cursor_wrapped_position(), (0, 0));

        app.view.cursor_x = 5;
        assert_eq!(app.get_cursor_wrapped_position(), (0, 5));

        app.view.cursor_x = 6;
        let (row, _col) = app.get_cursor_wrapped_position();
        assert_eq!(row, 1, "cursor_x=6 should be on row 1");

        app.view.cursor_x = 8;
        let (row, _col) = app.get_cursor_wrapped_position();
        assert_eq!(row, 1, "cursor_x=8 should be on row 1");
    }

    #[test]
    fn test_default_app_starts_clean() {
        let app = App::default();
        assert!(!app.is_dirty());
    }

    #[test]
    fn test_update_rates_success_updates_results() {
        let mut app = App {
            document: Document::from_lines(vec!["1 BTC in USD".to_string()]),
            ..Default::default()
        };

        // Simulate successful fetch with a specific rate
        let mut rates = std::collections::HashMap::new();
        rates.insert("BTC".to_string(), Decimal::from(42000));
        let result = Ok(numr_core::FetchResult {
            rates,
            warning: None,
        });
        app.update_rates(result);

        assert!(matches!(app.fetch_status, FetchStatus::Success));
        // Result should reflect the new rate
        let value = &app.document.results()[0];
        assert_eq!(
            value.as_decimal(),
            Some(Decimal::from(42000)),
            "1 BTC in USD should equal the updated rate"
        );
        assert!(
            !app.is_dirty(),
            "rate updates should not mark document dirty"
        );
    }

    #[test]
    fn test_update_rates_with_crypto_warning_shows_status() {
        let mut app = App::default();

        let rates = std::collections::HashMap::new();
        let result = Ok(numr_core::FetchResult {
            rates,
            warning: Some("crypto rates unavailable: 403 Forbidden".to_string()),
        });
        app.update_rates(result);

        assert!(matches!(app.fetch_status, FetchStatus::Success));
        assert_eq!(
            app.status_message.as_deref(),
            Some("Rates partial"),
            "warning should be surfaced in status bar"
        );
    }

    #[test]
    fn test_update_rates_error_sets_error_status() {
        let mut app = App::default();

        let result = Err("Failed to fetch fiat rates: timeout".to_string());
        app.update_rates(result);

        assert!(matches!(app.fetch_status, FetchStatus::Error(_)));
        if let FetchStatus::Error(msg) = &app.fetch_status {
            assert!(msg.contains("timeout"));
        }
    }

    #[test]
    fn test_refresh_results_preserves_dirty_state() {
        let mut app = App {
            document: Document::from_lines(vec!["1 + 1".to_string()]),
            ..Default::default()
        };

        app.document.refresh_results();
        assert!(!app.is_dirty());

        app.document.recalculate();
        assert!(app.is_dirty());

        app.document.refresh_results();
        assert!(app.is_dirty());
    }
}
