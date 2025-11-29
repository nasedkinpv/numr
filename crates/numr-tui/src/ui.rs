//! Minimal UI rendering

use ratatui::{
    layout::{Constraint, Layout, Position, Rect},
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Paragraph, Wrap},
    Frame,
};

use crate::app::{App, InputMode};
use crate::popups::{draw_help_popup, draw_quit_popup};
use numr_editor::{tokenize, TokenType};

/// Color palette - minimal and elegant (TTY 16-color compatible)
pub mod palette {
    use ratatui::style::Color;

    pub const DIM: Color = Color::DarkGray;
    pub const ACCENT: Color = Color::Cyan;
    pub const NUMBER: Color = Color::Yellow;
    pub const OPERATOR: Color = Color::Magenta;
    pub const VARIABLE: Color = Color::LightGreen;
    pub const UNIT: Color = Color::Blue;
    pub const ERROR: Color = Color::Red;
    pub const KEYWORD: Color = Color::Cyan; // "in", "of", "to"
    pub const TEXT: Color = Color::Gray; // unrecognized prose (neutral)
}

/// Main draw function
pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();

    // Calculate the width needed for results column (fit to content)
    let content_width = app
        .results
        .iter()
        .filter(|v| !v.is_error())
        .map(|v| v.to_string().len())
        .max()
        .unwrap_or(0)
        .max(8);

    // Clamp width: max 40 chars or 50% of screen width (whichever is smaller)
    // But ensure at least 8 chars if possible (unless screen is extremely small)
    let max_allowed = (area.width as usize / 2).min(40);
    let max_result_width = content_width.min(max_allowed) as u16;

    // Reserve space for debug panel if in debug mode and there's an error
    let has_error = app.current_line_error().is_some();
    let debug_height = if app.debug_mode && has_error { 5 } else { 0 };
    let header_height = if app.show_header { 1 } else { 0 };
    let footer_h = footer_height(app, area.width);

    // Layout: Header (optional) | Input/Results | Debug (optional) | Footer
    let [header_area, main_area, debug_area, footer_area] = Layout::vertical([
        Constraint::Length(header_height),
        Constraint::Fill(1),
        Constraint::Length(debug_height),
        Constraint::Length(footer_h),
    ])
    .areas(area);

    // Calculate width for line numbers
    let line_count = app.lines.len();
    let line_num_width = if app.show_line_numbers {
        line_count.to_string().len() as u16 + 1 // +1 for padding
    } else {
        0
    };

    if app.show_header {
        draw_header(frame, header_area, app);
    }

    if app.wrap_mode {
        // Wrap mode: render line-by-line with results bottom-aligned
        draw_wrapped_content(frame, main_area, app, max_result_width, line_num_width);
    } else {
        // Normal mode: three columns [nums | input | results]
        let [nums_area, rest_area] =
            Layout::horizontal([Constraint::Length(line_num_width), Constraint::Fill(1)])
                .areas(main_area);

        let [input_area, result_area] = Layout::horizontal([
            Constraint::Fill(1),
            Constraint::Length(max_result_width + 4),
        ])
        .areas(rest_area);

        draw_line_numbers(frame, nums_area, app);
        draw_input(frame, input_area, app);
        draw_results(frame, result_area, app);
    }

    // Draw debug panel if enabled and there's an error
    if app.debug_mode && has_error {
        draw_debug_panel(frame, debug_area, app);
    }

    // Check status expiry
    app.clear_status_if_expired();

    draw_footer(frame, footer_area, app, max_result_width + 4);

    if app.show_help {
        draw_help_popup(frame, area, app.help_scroll);
    }

    if app.show_quit_confirmation {
        draw_quit_popup(frame, area);
    }
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let filename = app
        .path
        .as_ref()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("Untitled");

    let status = if app.dirty { " [+]" } else { "" };

    let title = format!("numr - {filename}{status}");

    let block = Block::default().style(Style::new().fg(Color::White));
    let paragraph = Paragraph::new(title).block(block);
    frame.render_widget(paragraph, area);
}

/// Draw content in wrap mode with results bottom-aligned to each paragraph
fn draw_wrapped_content(
    frame: &mut Frame,
    area: Rect,
    app: &mut App,
    result_width: u16,
    line_num_width: u16,
) {
    let input_width = area.width.saturating_sub(result_width + 2 + line_num_width) as usize;

    // Update viewport for cursor visibility
    app.viewport_width = input_width;
    app.viewport_height = area.height as usize;
    app.ensure_cursor_visible();

    // Calculate cursor screen position for later
    let (cursor_row_in_line, cursor_x_in_row) = app.get_cursor_wrapped_position();
    let mut cursor_set = false;

    let mut current_visual_row = 0;
    let mut rendered_height = 0;

    // Iterate through all lines to find what to render
    // This might be slow for huge files, but for a calculator it's fine.
    // Optimization: we could cache heights or use a smarter data structure if needed.
    for (line_idx, line) in app.lines.iter().enumerate() {
        let line_height = app.get_wrapped_height(line);

        // Check if this line is visible
        if current_visual_row + line_height > app.viewport_y {
            // Calculate how much of the top of this line is hidden
            let skip_rows = if current_visual_row < app.viewport_y {
                (app.viewport_y - current_visual_row) as u16
            } else {
                0
            };

            // Calculate how much space we have left in the viewport
            let remaining_height = (area.height as usize).saturating_sub(rendered_height);
            if remaining_height == 0 {
                break;
            }

            // Calculate how much of this line we can show
            let visible_rows = (line_height as u16)
                .saturating_sub(skip_rows)
                .min(remaining_height as u16);

            if visible_rows > 0 {
                let row_area = Rect {
                    x: area.x,
                    y: area.y + rendered_height as u16,
                    width: area.width,
                    height: visible_rows,
                };

                let result = &app.results[line_idx];

                // Split row into [nums | input | result]
                let [nums_area, rest_area] =
                    Layout::horizontal([Constraint::Length(line_num_width), Constraint::Fill(1)])
                        .areas(row_area);

                let [input_area, result_area] =
                    Layout::horizontal([Constraint::Fill(1), Constraint::Length(result_width + 2)])
                        .areas(rest_area);

                // Render line number (only if we are showing the first row of the line)
                if skip_rows == 0 {
                    let num_style = if line_idx == app.cursor_y {
                        Style::new().fg(palette::ACCENT).bold()
                    } else {
                        Style::new().fg(palette::DIM)
                    };
                    let num_para = Paragraph::new(format!("{}", line_idx + 1)).style(num_style);
                    // Only render on the first row of the area (height 1)
                    let num_rect = Rect {
                        height: 1,
                        ..nums_area
                    };
                    frame.render_widget(num_para, num_rect);
                }

                // Render input with wrap
                let input_para = Paragraph::new(highlight_line(line))
                    .wrap(Wrap { trim: false })
                    .scroll((skip_rows, 0));

                frame.render_widget(input_para, input_area);

                // Set cursor position if this is the cursor line
                if !cursor_set && line_idx == app.cursor_y {
                    let cursor_visual_row_in_area = cursor_row_in_line as u16;
                    if cursor_visual_row_in_area >= skip_rows
                        && cursor_visual_row_in_area < skip_rows + visible_rows
                    {
                        let screen_y = input_area.y + (cursor_visual_row_in_area - skip_rows);
                        let screen_x = input_area.x + cursor_x_in_row as u16;
                        if screen_x < input_area.x + input_area.width {
                            frame.set_cursor_position(Position {
                                x: screen_x,
                                y: screen_y,
                            });
                            cursor_set = true;
                        }
                    }
                }

                // Render result bottom-aligned (on the last row of this area)
                // Only show result if we are showing the bottom of the line
                let is_showing_bottom = skip_rows + visible_rows == line_height as u16;

                if is_showing_bottom && !result.is_error() && !result.is_empty() {
                    let result_text = result.to_string();
                    // Position result at bottom of result_area
                    let result_y = result_area.y + result_area.height.saturating_sub(1);
                    let bottom_area = Rect {
                        x: result_area.x,
                        y: result_y,
                        width: result_area.width,
                        height: 1,
                    };
                    let result_para =
                        Paragraph::new(result_text.fg(palette::ACCENT)).right_aligned();
                    frame.render_widget(result_para, bottom_area);
                }

                rendered_height += visible_rows as usize;
            }
        }

        current_visual_row += line_height;

        if rendered_height >= area.height as usize {
            break;
        }
    }
}

fn draw_line_numbers(frame: &mut Frame, area: Rect, app: &App) {
    let lines: Vec<Line> = (0..app.lines.len())
        .map(|i| {
            let num = (i + 1).to_string();
            let style = if i == app.cursor_y {
                Style::new().fg(palette::ACCENT).bold()
            } else {
                Style::new().fg(palette::DIM)
            };
            Line::from(Span::styled(num, style))
        })
        .collect();

    let paragraph = Paragraph::new(lines).scroll((app.viewport_y as u16, 0));
    frame.render_widget(paragraph, area);
}

fn draw_input(frame: &mut Frame, area: Rect, app: &mut App) {
    // Update viewport dimensions based on actual area
    app.viewport_width = area.width as usize;
    app.viewport_height = area.height as usize;
    app.ensure_cursor_visible();

    let lines: Vec<Line> = app.lines.iter().map(|line| highlight_line(line)).collect();

    let paragraph = Paragraph::new(lines).scroll((app.viewport_y as u16, app.viewport_x as u16));
    frame.render_widget(paragraph, area);

    // Set terminal cursor position
    let cursor_screen_x = area.x + (app.cursor_x.saturating_sub(app.viewport_x)) as u16;
    let cursor_screen_y = area.y + (app.cursor_y.saturating_sub(app.viewport_y)) as u16;

    if cursor_screen_x < area.x + area.width && cursor_screen_y < area.y + area.height {
        frame.set_cursor_position(Position {
            x: cursor_screen_x,
            y: cursor_screen_y,
        });
    }
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

    // Results scroll vertically only (no horizontal scroll needed)
    let paragraph = Paragraph::new(lines)
        .right_aligned()
        .scroll((app.viewport_y as u16, 0));
    frame.render_widget(paragraph, area);
}

fn draw_debug_panel(frame: &mut Frame, area: Rect, app: &App) {
    if let Some(error) = app.current_line_error() {
        // Clean up error message
        let clean_error = error.strip_prefix("Parse error: ").unwrap_or(error);

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

/// Smooth easing function (ease-in-out)
fn ease_in_out(t: f64) -> f64 {
    if t < 0.5 {
        2.0 * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
    }
}

/// Generate a flowing gradient color for loading animation
fn loading_pulse_color(start: std::time::Instant) -> Color {
    let elapsed = start.elapsed().as_millis() as f64;
    // Smooth cycle every 1.5s
    let raw_t = (elapsed / 1500.0).fract();
    // Ping-pong: 0->1->0
    let t = if raw_t < 0.5 {
        raw_t * 2.0
    } else {
        2.0 - raw_t * 2.0
    };
    let t = ease_in_out(t);

    // Flow from Cyan (80, 180, 220) -> Magenta (180, 100, 220)
    let r = (80.0 + t * 100.0) as u8;
    let g = (180.0 - t * 80.0) as u8;
    let b = 220;
    Color::Rgb(r, g, b)
}

/// Generate a flowing gradient color for saved state
fn saved_pulse_color(start: std::time::Instant) -> Color {
    let elapsed = start.elapsed().as_millis() as f64;
    // Smooth cycle every 1.5s
    let raw_t = (elapsed / 1500.0).fract();
    // Ping-pong: 0->1->0
    let t = if raw_t < 0.5 {
        raw_t * 2.0
    } else {
        2.0 - raw_t * 2.0
    };
    let t = ease_in_out(t);

    // Flow from Green (100, 200, 120) -> Teal (80, 180, 180)
    let r = (100.0 - t * 20.0) as u8;
    let g = (200.0 - t * 20.0) as u8;
    let b = (120.0 + t * 60.0) as u8;
    Color::Rgb(r, g, b)
}

/// Build the totals string from grouped totals
fn build_totals_string(app: &App) -> String {
    let grouped = app.grouped_totals();
    if grouped.is_empty() {
        String::new()
    } else {
        grouped
            .iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join("  ") // Use double space as clean separator
    }
}

/// Calculate footer height needed (1 or 2 rows based on content width)
pub fn footer_height(app: &App, area_width: u16) -> u16 {
    let totals_str = build_totals_string(app);
    if totals_str.is_empty() {
        return 1;
    }

    // Estimate hints width (mode + filename + hints ≈ 40-50 chars typically)
    let hints_width_estimate = 45_u16;
    // "total: " prefix + values
    let totals_width = (totals_str.len() + 7) as u16;

    // If both fit on one line with some padding, use 1 row
    if hints_width_estimate + totals_width + 4 <= area_width {
        1
    } else {
        2
    }
}

fn draw_footer(frame: &mut Frame, area: Rect, app: &App, _result_width: u16) {
    let totals_str = build_totals_string(app);
    let use_two_rows = area.height >= 2 && !totals_str.is_empty();

    // Build hints line
    let hints = build_hints_line(app);

    if use_two_rows {
        // Two-row layout: totals on top (right), hints on bottom (space-between)
        let [totals_area, hints_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(area);

        // Totals row (right-aligned): "total:" dim, values bold
        let total_line = Line::from(vec!["total: ".dim(), totals_str.bold()]);
        let totals_widget = Paragraph::new(total_line).right_aligned();
        frame.render_widget(totals_widget, totals_area);

        // Hints row: split into left (mode+file) and right (keybindings)
        let (left_hints, right_hints) = build_hints_parts(app);
        let [left_area, right_area] =
            Layout::horizontal([Constraint::Fill(1), Constraint::Fill(1)]).areas(hints_area);
        let left_widget = Paragraph::new(Line::from(left_hints));
        let right_widget = Paragraph::new(Line::from(right_hints)).right_aligned();
        frame.render_widget(left_widget, left_area);
        frame.render_widget(right_widget, right_area);
    } else {
        // Single-row layout: hints left, totals right
        // Account for "total: " prefix (7 chars)
        let totals_width = if totals_str.is_empty() {
            0
        } else {
            (totals_str.len() + 7) as u16
        };

        let [left_area, right_area] =
            Layout::horizontal([Constraint::Fill(1), Constraint::Length(totals_width)]).areas(area);

        let left_footer = Paragraph::new(Line::from(hints));
        frame.render_widget(left_footer, left_area);

        if !totals_str.is_empty() {
            let total_line = Line::from(vec!["total: ".dim(), totals_str.bold()]);
            let right_footer = Paragraph::new(total_line).right_aligned();
            frame.render_widget(right_footer, right_area);
        }
    }
}

/// Build hints split into (left, right) for two-row layout
/// Left: mode indicator (with unsaved dot)
/// Right: keybindings
fn build_hints_parts(app: &App) -> (Vec<Span<'static>>, Vec<Span<'static>>) {
    // Left part: mode/status indicator + unsaved indicator
    let first_span = build_mode_indicator(app);

    let mut left = vec![first_span];

    // Unsaved indicator: a subtle dot after mode
    if app.dirty {
        left.push(" •".fg(palette::NUMBER));
    }

    // Right part: keybindings
    let mut right: Vec<Span<'static>> = Vec::new();

    match app.mode {
        InputMode::Normal => {
            right.push("?".fg(palette::ACCENT));
            right.push(" help ".dim());
        }
        InputMode::Insert => {
            right.push("esc".fg(palette::ACCENT));
            right.push(" normal ".dim());
        }
    }

    right.push("^s".fg(palette::ACCENT));
    right.push(" save ".dim());

    if app.debug_mode {
        right.push("F12".fg(palette::ACCENT));
        right.push(" debug ".dim());
    }

    if app.wrap_mode {
        right.push("WRAP ".fg(palette::KEYWORD));
    }

    let rates_color = match &app.fetch_status {
        crate::app::FetchStatus::Fetching => Color::Yellow,
        crate::app::FetchStatus::Success => Color::Green,
        crate::app::FetchStatus::Error(_) => palette::ERROR,
        crate::app::FetchStatus::Idle => palette::DIM,
    };
    right.push("^r".fg(palette::ACCENT));
    right.push(Span::styled(" rates", Style::new().fg(rates_color)));

    (left, right)
}

/// Build the mode/status indicator span
fn build_mode_indicator(app: &App) -> Span<'static> {
    if let Some(msg) = &app.status_message {
        let bg = if msg == "Saved" {
            // Animated green gradient for saved
            app.status_start
                .map(saved_pulse_color)
                .unwrap_or(palette::VARIABLE)
        } else {
            palette::ACCENT
        };
        Span::styled(
            format!(" {} ", msg.to_uppercase()),
            Style::new().fg(Color::Black).bg(bg).bold(),
        )
    } else if app.fetch_status == crate::app::FetchStatus::Fetching {
        let bg_color = app
            .fetch_start
            .map(loading_pulse_color)
            .unwrap_or(palette::ACCENT);
        Span::styled(
            " LOADING ",
            Style::new().fg(Color::Black).bg(bg_color).bold(),
        )
    } else {
        match app.mode {
            InputMode::Normal => " NORMAL ".fg(Color::Black).bg(palette::ACCENT).bold(),
            InputMode::Insert => " INSERT ".fg(Color::Black).bg(palette::VARIABLE).bold(),
        }
    }
}

/// Build the hints/status line spans (single row layout)
fn build_hints_line(app: &App) -> Vec<Span<'static>> {
    let (mut left, right) = build_hints_parts(app);
    // Add space after filename for single-row
    left.push(" ".into());
    left.extend(right);
    left
}

/// Syntax highlighting for a line
fn highlight_line(input: &str) -> Line<'static> {
    Line::from(tokenize_and_style(input))
}

fn token_color(token_type: TokenType) -> Color {
    match token_type {
        TokenType::Number => palette::NUMBER,
        TokenType::Operator => palette::OPERATOR,
        TokenType::Variable => palette::VARIABLE,
        TokenType::Unit => palette::UNIT,
        TokenType::Currency => palette::UNIT,
        TokenType::Keyword => palette::KEYWORD,
        TokenType::Function => palette::OPERATOR,
        TokenType::Comment => palette::DIM,
        TokenType::Text => palette::TEXT,
        TokenType::Whitespace => Color::Reset,
        TokenType::Punctuation => palette::DIM,
    }
}

/// Tokenize input and apply syntax highlighting
fn tokenize_and_style(input: &str) -> Vec<Span<'static>> {
    let tokens = tokenize(input);
    tokens
        .into_iter()
        .map(|t| Span::styled(t.text, Style::new().fg(token_color(t.token_type))))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Extract (text, color) pairs from tokenized spans for testing
    fn tokenize_to_pairs(input: &str) -> Vec<(String, Color)> {
        tokenize_and_style(input)
            .into_iter()
            .map(|span| {
                let text = span.content.to_string();
                let color = span.style.fg.unwrap_or(Color::Reset);
                (text, color)
            })
            .collect()
    }

    /// Helper to check if a token exists with expected color
    fn has_token(pairs: &[(String, Color)], text: &str, expected_color: Color) -> bool {
        pairs.iter().any(|(t, c)| t == text && *c == expected_color)
    }

    #[test]
    fn test_simple_number() {
        let pairs = tokenize_to_pairs("42");
        assert!(has_token(&pairs, "42", palette::NUMBER));
    }

    #[test]
    fn test_negative_number() {
        let pairs = tokenize_to_pairs("-5");
        assert!(has_token(&pairs, "-5", palette::NUMBER));
    }

    #[test]
    fn test_percentage() {
        let pairs = tokenize_to_pairs("20%");
        assert!(has_token(&pairs, "20%", palette::NUMBER));
    }

    #[test]
    fn test_basic_operators() {
        let pairs = tokenize_to_pairs("1 + 2");
        assert!(has_token(&pairs, "1", palette::NUMBER));
        assert!(has_token(&pairs, "+", palette::OPERATOR));
        assert!(has_token(&pairs, "2", palette::NUMBER));
    }

    #[test]
    fn test_multiply_asterisk() {
        let pairs = tokenize_to_pairs("3 * 4");
        assert!(has_token(&pairs, "*", palette::OPERATOR));
    }

    #[test]
    fn test_multiply_x_no_spaces() {
        let pairs = tokenize_to_pairs("2x3");
        assert!(has_token(&pairs, "2", palette::NUMBER));
        assert!(has_token(&pairs, "x", palette::OPERATOR));
        assert!(has_token(&pairs, "3", palette::NUMBER));
    }

    #[test]
    fn test_multiply_x_with_spaces() {
        let pairs = tokenize_to_pairs("2 x 3");
        assert!(has_token(&pairs, "2", palette::NUMBER));
        assert!(has_token(&pairs, "x", palette::OPERATOR));
        assert!(has_token(&pairs, "3", palette::NUMBER));
    }

    #[test]
    fn test_word_not_multiply() {
        // "tax" alone is plain text (not a defined variable)
        let pairs = tokenize_to_pairs("tax");
        assert!(has_token(&pairs, "tax", palette::TEXT));
    }

    #[test]
    fn test_word_x2() {
        // "x2" alone is plain text
        let pairs = tokenize_to_pairs("x2");
        assert!(has_token(&pairs, "x2", palette::TEXT));
    }

    #[test]
    fn test_variable_assignment() {
        // Variable being defined gets VARIABLE color
        let pairs = tokenize_to_pairs("tax = 20%");
        assert!(has_token(&pairs, "tax", palette::VARIABLE));
        assert!(has_token(&pairs, "20%", palette::NUMBER));
    }

    #[test]
    fn test_comment_line() {
        // Comment lines are dimmed
        let pairs = tokenize_to_pairs("# this is a comment");
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].1, palette::DIM);
    }

    #[test]
    fn test_prose_with_numbers() {
        // Prose text: words are TEXT, but numbers/units still highlighted
        let pairs = tokenize_to_pairs("i put 10 usd here");
        assert!(has_token(&pairs, "i", palette::TEXT));
        assert!(has_token(&pairs, "put", palette::TEXT));
        assert!(has_token(&pairs, "10", palette::NUMBER));
        assert!(has_token(&pairs, "usd", palette::UNIT));
        assert!(has_token(&pairs, "here", palette::TEXT));
    }

    #[test]
    fn test_currency_symbol_before() {
        let pairs = tokenize_to_pairs("$100");
        assert!(has_token(&pairs, "$", palette::UNIT));
        assert!(has_token(&pairs, "100", palette::NUMBER));
    }

    #[test]
    fn test_currency_code() {
        let pairs = tokenize_to_pairs("100 USD");
        assert!(has_token(&pairs, "100", palette::NUMBER));
        assert!(has_token(&pairs, "USD", palette::UNIT));
    }

    #[test]
    fn test_unit() {
        let pairs = tokenize_to_pairs("5 km");
        assert!(has_token(&pairs, "5", palette::NUMBER));
        assert!(has_token(&pairs, "km", palette::UNIT));
    }

    #[test]
    fn test_assignment() {
        let pairs = tokenize_to_pairs("x = 10");
        assert!(has_token(&pairs, "x", palette::VARIABLE));
        assert!(has_token(&pairs, "=", palette::OPERATOR));
        assert!(has_token(&pairs, "10", palette::NUMBER));
    }

    #[test]
    fn test_function_call() {
        let pairs = tokenize_to_pairs("sum(1, 2)");
        assert!(has_token(&pairs, "sum", palette::OPERATOR));
        assert!(has_token(&pairs, "1", palette::NUMBER));
        assert!(has_token(&pairs, "2", palette::NUMBER));
    }

    #[test]
    fn test_keyword_in() {
        let pairs = tokenize_to_pairs("$100 in EUR");
        assert!(has_token(&pairs, "in", palette::KEYWORD));
    }

    #[test]
    fn test_keyword_of() {
        let pairs = tokenize_to_pairs("20% of 100");
        assert!(has_token(&pairs, "of", palette::KEYWORD));
    }

    #[test]
    fn test_keyword_to() {
        let pairs = tokenize_to_pairs("5 km to miles");
        assert!(has_token(&pairs, "to", palette::KEYWORD));
    }
}
