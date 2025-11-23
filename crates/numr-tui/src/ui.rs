//! Minimal UI rendering

use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Paragraph, Wrap},
    Frame,
};
use numr_core::{Currency, Unit};
use std::collections::HashSet;
use std::sync::LazyLock;

use crate::app::{App, InputMode};

/// Color palette - minimal and elegant
mod palette {
    use ratatui::style::Color;

    pub const DIM: Color = Color::DarkGray;
    pub const ACCENT: Color = Color::Cyan;
    pub const NUMBER: Color = Color::Yellow;
    pub const OPERATOR: Color = Color::Magenta;
    pub const VARIABLE: Color = Color::Green;
    pub const UNIT: Color = Color::Blue;
    pub const ERROR: Color = Color::Red;
}

/// Cached sets for syntax highlighting - built from registries
static CURRENCY_SYMBOLS: LazyLock<HashSet<char>> = LazyLock::new(|| {
    Currency::all_symbols()
        .filter_map(|s| s.chars().next())
        .collect()
});

static CURRENCY_WORDS: LazyLock<HashSet<String>> = LazyLock::new(|| {
    Currency::all_aliases()
        .map(|s| s.to_lowercase())
        .chain(Currency::all_codes().map(|s| s.to_lowercase()))
        .collect()
});

static UNIT_WORDS: LazyLock<HashSet<String>> = LazyLock::new(|| {
    Unit::all_aliases()
        .map(|s| s.to_lowercase())
        .chain(Unit::all_short_names().map(|s| s.to_lowercase()))
        .collect()
});

/// Check if a character is a currency symbol
fn is_currency_symbol(c: char) -> bool {
    CURRENCY_SYMBOLS.contains(&c)
}

/// Check if a word is a currency code/name
fn is_currency_word(word: &str) -> bool {
    CURRENCY_WORDS.contains(&word.to_lowercase())
}

/// Check if a word is a unit
fn is_unit_word(word: &str) -> bool {
    UNIT_WORDS.contains(&word.to_lowercase())
}

/// Main draw function
pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // Calculate the width needed for results column (fit to content)
    let max_result_width = app
        .results
        .iter()
        .filter(|v| !v.is_error())
        .map(|v| v.to_string().len())
        .max()
        .unwrap_or(0)
        .max(8) as u16;

    // Reserve space for debug panel if in debug mode and there's an error
    let has_error = app.current_line_error().is_some();
    let debug_height = if app.debug_mode && has_error { 5 } else { 0 };

    // Layout: Header | Input/Results | Debug (optional) | Footer
    let [header_area, main_area, debug_area, footer_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(debug_height),
        Constraint::Length(1),
    ])
    .areas(area);

    draw_header(frame, header_area, app);

    // Layout: input (fill) | results (fit)
    let [input_area, result_area] = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(max_result_width + 4),
    ])
    .areas(main_area);

    draw_input(frame, input_area, app);
    draw_results(frame, result_area, app);

    // Draw debug panel if enabled and there's an error
    if app.debug_mode && has_error {
        draw_debug_panel(frame, debug_area, app);
    }

    draw_footer(frame, footer_area, app, max_result_width + 4);
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let filename = app.path.as_ref()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("Untitled");

    let status = if app.dirty { " [+]" } else { "" };
    
    let title = format!(" numr - {}{} ", filename, status);
    
    let block = Block::default().style(Style::default().bg(palette::DIM).fg(Color::White));
    let paragraph = Paragraph::new(title).block(block);
    frame.render_widget(paragraph, area);
}

fn draw_input(frame: &mut Frame, area: Rect, app: &App) {
    let lines: Vec<Line> = app
        .lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            if i == app.cursor_y {
                highlight_line_with_cursor(line, app.cursor_x)
            } else {
                highlight_line(line)
            }
        })
        .collect();

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, area);
}

fn draw_results(frame: &mut Frame, area: Rect, app: &App) {
    let lines: Vec<Line> = app
        .results
        .iter()
        .map(|value| {
            if value.is_error() || value.is_empty() {
                Line::from("")
            } else {
                Line::from(value.to_string().fg(palette::ACCENT))
            }
        })
        .collect();

    let paragraph = Paragraph::new(lines).right_aligned();
    frame.render_widget(paragraph, area);
}

fn draw_debug_panel(frame: &mut Frame, area: Rect, app: &App) {
    if let Some(error) = app.current_line_error() {
        // Clean up error message
        let clean_error = error
            .strip_prefix("Parse error: ")
            .unwrap_or(error);

        // Create a red bordered block
        let block = Block::bordered()
            .title(" error ")
            .title_style(Style::new().fg(palette::ERROR).bold())
            .border_style(Style::new().fg(palette::ERROR));

        // Create paragraph with word wrapping
        let paragraph = Paragraph::new(clean_error.to_string())
            .style(Style::new().fg(palette::ERROR))
            .block(block)
            .wrap(Wrap { trim: true });

        frame.render_widget(paragraph, area);
    }
}

fn draw_footer(frame: &mut Frame, area: Rect, app: &App, result_width: u16) {
    let total = app.total();

    let mode_span = match app.mode {
        InputMode::Normal => " NORMAL ".fg(Color::Black).bg(palette::ACCENT).bold(),
        InputMode::Insert => " INSERT ".fg(Color::Black).bg(palette::VARIABLE).bold(),
    };

    let mut hints = vec![
        mode_span,
        " ".into(),
    ];

    match app.mode {
        InputMode::Normal => {
            hints.push("q".fg(palette::ACCENT));
            hints.push(" quit ".dim());
            hints.push("i".fg(palette::ACCENT));
            hints.push(" insert ".dim());
            hints.push("o".fg(palette::ACCENT));
            hints.push(" new line ".dim());
            hints.push("dd".fg(palette::ACCENT));
            hints.push(" delete ".dim());
            hints.push("^s".fg(palette::ACCENT));
            hints.push(" save ".dim());
        }
        InputMode::Insert => {
            hints.push("esc".fg(palette::ACCENT));
            hints.push(" normal ".dim());
            hints.push("enter".fg(palette::ACCENT));
            hints.push(" new line ".dim());
        }
    }

    if app.debug_mode {
        hints.push("F12".fg(palette::ACCENT));
        hints.push(" debug on ".dim());
    }

    // Fetch status
    match &app.fetch_status {
        crate::app::FetchStatus::Fetching => {
            hints.push(" Rates: Fetching...".fg(Color::Yellow));
        }
        crate::app::FetchStatus::Success => {
            hints.push(" Rates: OK".fg(Color::Green));
        }
        crate::app::FetchStatus::Error(_) => {
            hints.push(" Rates: Error".fg(Color::Red));
        }
        crate::app::FetchStatus::Idle => {}
    }

    // Split footer into left (hints) and right (total) sections
    let [left_area, right_area] = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(result_width),
    ])
    .areas(area);

    let left_footer = Paragraph::new(Line::from(hints))
        .style(Style::default().bg(palette::DIM));
    frame.render_widget(left_footer, left_area);

    let total_line = Line::from(vec![
        "total ".dim(),
        format!("{:.2}", total).fg(palette::ACCENT).bold(),
    ]);
    let right_footer = Paragraph::new(total_line)
        .right_aligned()
        .style(Style::default().bg(palette::DIM));
    frame.render_widget(right_footer, right_area);
}

/// Syntax highlighting for a line
fn highlight_line(input: &str) -> Line<'static> {
    Line::from(tokenize_and_style(input))
}

/// Syntax highlighting with cursor
fn highlight_line_with_cursor(input: &str, cursor_col: usize) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let styled_spans = tokenize_and_style(input);

    let mut current_pos = 0;
    let mut cursor_handled = false;

    for span in styled_spans {
        let span_chars: Vec<char> = span.content.chars().collect();
        let span_len = span_chars.len();

        if !cursor_handled && cursor_col >= current_pos && cursor_col < current_pos + span_len {
            let local_pos = cursor_col - current_pos;

            if local_pos > 0 {
                let before: String = span_chars[..local_pos].iter().collect();
                spans.push(Span::styled(before, span.style));
            }

            let cursor_char = span_chars.get(local_pos).copied().unwrap_or(' ');
            spans.push(Span::styled(
                cursor_char.to_string(),
                Style::new().bg(Color::White).fg(Color::Black),
            ));

            if local_pos + 1 < span_len {
                let after: String = span_chars[local_pos + 1..].iter().collect();
                spans.push(Span::styled(after, span.style));
            }
            cursor_handled = true;
        } else {
            spans.push(span);
        }
        current_pos += span_len;
    }

    if !cursor_handled {
        spans.push(Span::styled(
            " ",
            Style::new().bg(Color::White).fg(Color::Black),
        ));
    }

    Line::from(spans)
}

/// Keywords for syntax highlighting
static KEYWORDS: &[&str] = &["of", "in", "to"];
static FUNCTIONS: &[&str] = &[
    "sum", "avg", "average", "min", "max", "abs", "sqrt", "round", "floor", "ceil", "total",
];

/// Tokenize input and apply syntax highlighting
fn tokenize_and_style(input: &str) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];

        if c.is_ascii_digit() || (c == '-' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit()) {
            // Numbers (including negative and percentages)
            let start = i;
            if c == '-' {
                i += 1;
            }
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            if i < chars.len() && chars[i] == '%' {
                i += 1;
            }
            let num: String = chars[start..i].iter().collect();
            spans.push(num.fg(palette::NUMBER));
        } else if is_currency_symbol(c) {
            // Currency symbols (from registry)
            spans.push(c.to_string().fg(palette::UNIT));
            i += 1;
        } else if c == '+' || c == '*' || c == '/' || c == '^' || c == '×' || c == '÷' {
            spans.push(c.to_string().fg(palette::OPERATOR));
            i += 1;
        } else if c == 'x' && is_multiply_context(&chars, i) {
            // 'x' as multiplication operator (e.g., "2x3")
            spans.push("x".fg(palette::OPERATOR));
            i += 1;
        } else if c == '-' {
            spans.push("-".fg(palette::OPERATOR));
            i += 1;
        } else if c == '=' {
            spans.push("=".fg(palette::OPERATOR).dim());
            i += 1;
        } else if c.is_alphabetic() {
            // Words: check against registries
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            let lower = word.to_lowercase();

            let color = if KEYWORDS.contains(&lower.as_str()) {
                palette::DIM
            } else if FUNCTIONS.contains(&lower.as_str()) {
                palette::OPERATOR
            } else if is_unit_word(&word) || is_currency_word(&word) {
                palette::UNIT
            } else {
                palette::VARIABLE
            };

            spans.push(word.fg(color));
        } else if c == '(' || c == ')' {
            spans.push(c.to_string().fg(palette::DIM));
            i += 1;
        } else if c == ' ' || c == '\t' {
            let start = i;
            while i < chars.len() && (chars[i] == ' ' || chars[i] == '\t') {
                i += 1;
            }
            let ws: String = chars[start..i].iter().collect();
            spans.push(Span::raw(ws));
        } else {
            spans.push(Span::raw(c.to_string()));
            i += 1;
        }
    }

    spans
}

/// Check if 'x' at position i is likely a multiplication operator.
/// True if preceded by digit/)/% and followed by digit/(/currency symbol.
/// Skips whitespace when checking context.
fn is_multiply_context(chars: &[char], i: usize) -> bool {
    // Look backwards, skipping whitespace
    let prev_ok = {
        let mut j = i;
        while j > 0 && chars[j - 1] == ' ' {
            j -= 1;
        }
        j > 0 && {
            let p = chars[j - 1];
            p.is_ascii_digit() || p == ')' || p == '%'
        }
    };
    // Look forwards, skipping whitespace
    let next_ok = {
        let mut j = i + 1;
        while j < chars.len() && chars[j] == ' ' {
            j += 1;
        }
        j < chars.len() && {
            let n = chars[j];
            n.is_ascii_digit() || n == '(' || is_currency_symbol(n)
        }
    };
    prev_ok && next_ok
}
