//! Minimal UI rendering

use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Paragraph, Wrap},
    Frame,
};

use crate::app::{App, InputMode};
use numr_editor::{tokenize, TokenType};

/// Color palette - minimal and elegant (TTY 16-color compatible)
mod palette {
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

    if app.wrap_mode {
        // Wrap mode: render line-by-line with results bottom-aligned
        draw_wrapped_content(frame, main_area, app, max_result_width);
    } else {
        // Normal mode: two columns with scroll
        let [input_area, result_area] = Layout::horizontal([
            Constraint::Fill(1),
            Constraint::Length(max_result_width + 4),
        ])
        .areas(main_area);

        draw_input(frame, input_area, app);
        draw_results(frame, result_area, app);
    }

    // Draw debug panel if enabled and there's an error
    if app.debug_mode && has_error {
        draw_debug_panel(frame, debug_area, app);
    }

    draw_footer(frame, footer_area, app, max_result_width + 4);
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let filename = app
        .path
        .as_ref()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("Untitled");

    let status = if app.dirty { " [+]" } else { "" };

    let title = format!(" numr - {}{} ", filename, status);

    let block = Block::default().style(Style::new().bg(palette::DIM).fg(Color::White));
    let paragraph = Paragraph::new(title).block(block);
    frame.render_widget(paragraph, area);
}

/// Calculate wrapped line height (how many visual rows a line takes)
fn wrapped_height(text: &str, width: usize) -> usize {
    if text.is_empty() || width == 0 {
        return 1;
    }
    // Simple word-wrap estimation: count characters and divide by width
    // For more accurate results, we'd need to track word boundaries
    let char_count = text.chars().count();
    char_count.div_ceil(width).max(1)
}

/// Draw content in wrap mode with results bottom-aligned to each paragraph
fn draw_wrapped_content(frame: &mut Frame, area: Rect, app: &mut App, result_width: u16) {
    let input_width = area.width.saturating_sub(result_width + 2) as usize;

    // Update viewport for cursor visibility
    app.viewport_width = input_width;
    app.viewport_height = area.height as usize;
    app.ensure_cursor_visible();

    // Calculate heights for visible lines
    let mut heights: Vec<u16> = Vec::new();
    let mut total_height: u16 = 0;
    let mut visible_start = app.viewport_y;

    // Find which lines fit in viewport
    for line in app.lines.iter().skip(app.viewport_y) {
        let h = wrapped_height(line, input_width) as u16;
        if total_height + h > area.height {
            break;
        }
        heights.push(h);
        total_height += h;
    }

    // If we have remaining space, try to show more lines above
    while visible_start > 0 && total_height < area.height {
        visible_start -= 1;
        let h = wrapped_height(&app.lines[visible_start], input_width) as u16;
        if total_height + h > area.height {
            visible_start += 1;
            break;
        }
        heights.insert(0, h);
        total_height += h;
    }

    if heights.is_empty() {
        return;
    }

    // Create constraints for visible lines
    let constraints: Vec<Constraint> = heights.iter().map(|&h| Constraint::Length(h)).collect();

    // Split area into row areas
    let row_areas = Layout::vertical(constraints).split(area);

    // Render each visible line
    for (idx, row_area) in row_areas.iter().enumerate() {
        let line_idx = visible_start + idx;
        if line_idx >= app.lines.len() {
            break;
        }

        let line = &app.lines[line_idx];
        let result = &app.results[line_idx];

        // Split row into input + result columns
        let [input_area, result_area] =
            Layout::horizontal([Constraint::Fill(1), Constraint::Length(result_width + 2)])
                .areas(*row_area);

        // Render input with wrap
        let highlighted = if line_idx == app.cursor_y {
            highlight_line_with_cursor(line, app.cursor_x)
        } else {
            highlight_line(line)
        };

        let input_para = Paragraph::new(highlighted).wrap(Wrap { trim: false });
        frame.render_widget(input_para, input_area);

        // Render result bottom-aligned (on the last row of this area)
        if !result.is_error() && !result.is_empty() {
            let result_text = result.to_string();
            // Position result at bottom of result_area
            let result_y = result_area.y + result_area.height.saturating_sub(1);
            let bottom_area = Rect {
                x: result_area.x,
                y: result_y,
                width: result_area.width,
                height: 1,
            };
            let result_para = Paragraph::new(result_text.fg(palette::ACCENT)).right_aligned();
            frame.render_widget(result_para, bottom_area);
        }
    }
}

fn draw_input(frame: &mut Frame, area: Rect, app: &mut App) {
    // Update viewport dimensions based on actual area
    app.viewport_width = area.width as usize;
    app.viewport_height = area.height as usize;
    app.ensure_cursor_visible();

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

    // scroll((row, col)) = (vertical_offset, horizontal_offset)
    let paragraph = Paragraph::new(lines).scroll((app.viewport_y as u16, app.viewport_x as u16));
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

fn draw_footer(frame: &mut Frame, area: Rect, app: &App, result_width: u16) {
    let total = app.total();

    let mode_span = match app.mode {
        InputMode::Normal => " NORMAL ".fg(Color::Black).bg(palette::ACCENT).bold(),
        InputMode::Insert => " INSERT ".fg(Color::Black).bg(palette::VARIABLE).bold(),
    };

    let mut hints = vec![mode_span, " ".into()];

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
            hints.push("w".fg(palette::ACCENT));
            hints.push(" wrap ".dim());
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

    // Wrap mode indicator
    if app.wrap_mode {
        hints.push(" WRAP".fg(palette::KEYWORD));
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
    let [left_area, right_area] =
        Layout::horizontal([Constraint::Fill(1), Constraint::Length(result_width)]).areas(area);

    let left_footer = Paragraph::new(Line::from(hints)).style(Style::new().bg(palette::DIM));
    frame.render_widget(left_footer, left_area);

    let total_line = Line::from(vec![
        "total ".dim(),
        format!("{:.2}", total).fg(palette::ACCENT).bold(),
    ]);
    let right_footer = Paragraph::new(total_line)
        .right_aligned()
        .style(Style::new().bg(palette::DIM));
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
