//! Minimal UI rendering

use std::collections::HashSet;

use ratatui::{
    layout::{Constraint, Layout, Position, Rect},
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Paragraph, Wrap},
    Frame,
};

use crate::app::{App, InputMode, KeybindingMode};
use crate::line_layout::{
    highlight_line, marked_line, take_marker_cells, wrapped_result_row, LineMarkers,
};
use crate::popups::{draw_help_popup, draw_quit_popup};
use crate::theme as palette;

#[cfg(test)]
use crate::line_layout::{token_color, tokenize_and_style};
#[cfg(test)]
use numr_editor::TokenType;

// ========================================
// Layout Constants
// ========================================

/// Maximum width for the results column (characters)
const MAX_RESULT_WIDTH: u16 = 40;

/// Estimated width of hints section for layout calculations
const HINTS_WIDTH_ESTIMATE: u16 = 45;

fn result_column_width(app: &App, area: Rect) -> u16 {
    let content_width = app.max_result_width().max(8);

    let max_allowed = (area.width as usize / 2).min(MAX_RESULT_WIDTH as usize);
    content_width.min(max_allowed) as u16
}

fn line_number_width(app: &App) -> u16 {
    if app.show_line_numbers {
        app.lines().len().to_string().len() as u16 + 1
    } else {
        0
    }
}

pub fn viewport_dimensions(app: &App, area: Rect) -> (usize, usize) {
    let max_result_width = result_column_width(app, area);
    let has_error = app.current_line_error().is_some();
    let debug_height = if app.debug_mode && has_error { 5 } else { 0 };
    let header_height = if app.show_header { 1 } else { 0 };
    let footer_h = footer_height(app, area.width);

    let [_header_area, main_area, _debug_area, _footer_area] = Layout::vertical([
        Constraint::Length(header_height),
        Constraint::Fill(1),
        Constraint::Length(debug_height),
        Constraint::Length(footer_h),
    ])
    .areas(area);

    let line_num_width = line_number_width(app);

    if app.wrap_mode {
        let width = main_area
            .width
            .saturating_sub(max_result_width + 2 + line_num_width) as usize;
        (width, main_area.height as usize)
    } else {
        let [_nums_area, rest_area] =
            Layout::horizontal([Constraint::Length(line_num_width), Constraint::Fill(1)])
                .areas(main_area);
        let [input_area, _result_area] = Layout::horizontal([
            Constraint::Fill(1),
            Constraint::Length(max_result_width + 4),
        ])
        .areas(rest_area);
        (input_area.width as usize, input_area.height as usize)
    }
}

/// Main draw function
pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();

    let max_result_width = result_column_width(app, area);

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
    let line_num_width = line_number_width(app);

    if app.show_header {
        draw_header(frame, header_area, app);
    }

    if app.wrap_mode {
        // Wrap mode: render each result beside the end of its expression.
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

    draw_footer(frame, footer_area, app, max_result_width + 4);

    if app.show_help {
        draw_help_popup(frame, area, app.help_scroll, app.keybinding_mode);
    }

    if app.show_quit_confirmation {
        draw_quit_popup(frame, area);
    }
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let filename = app
        .path()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("Untitled");

    let status = if app.is_dirty() { " [+]" } else { "" };

    let title = format!("numr - {filename}{status}");

    let block = Block::default().style(Style::new().fg(Color::White));
    let paragraph = Paragraph::new(title).block(block);
    frame.render_widget(paragraph, area);
}

/// Draw wrapped content with results anchored to each expression, before comments.
fn draw_wrapped_content(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    result_width: u16,
    line_num_width: u16,
) {
    let variables = app.variable_names();
    let mut cursor_set = false;
    let input_width = area.width.saturating_sub(line_num_width + result_width + 2);

    let mut current_visual_row = 0;
    let mut rendered_height = 0;

    // Iterate through all lines to find what to render
    for (line_idx, line) in app.lines().iter().enumerate() {
        let markers =
            LineMarkers::new(line, (line_idx == app.cursor_y()).then_some(app.cursor_x()));
        let result_text = app.result_text(line_idx);
        let result_row =
            result_text.and_then(|_| wrapped_result_row(line, variables, input_width as usize));
        let input_para =
            Paragraph::new(marked_line(line, variables, &markers)).wrap(Wrap { trim: false });
        let line_height = if line.is_empty() || input_width == 0 {
            1
        } else {
            input_para.line_count(input_width).max(1)
        };

        // Check if this line is visible
        if current_visual_row + line_height > app.viewport_y() {
            // Calculate how much of the top of this line is hidden
            let skip_rows = if current_visual_row < app.viewport_y() {
                (app.viewport_y() - current_visual_row) as u16
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

                // Split row into [nums | input | result]
                let [nums_area, rest_area] =
                    Layout::horizontal([Constraint::Length(line_num_width), Constraint::Fill(1)])
                        .areas(row_area);

                let [input_area, result_area] =
                    Layout::horizontal([Constraint::Fill(1), Constraint::Length(result_width + 2)])
                        .areas(rest_area);

                // Render line number (only if we are showing the first row of the line)
                if skip_rows == 0 {
                    let num_style = if line_idx == app.cursor_y() {
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

                frame.render_widget(input_para.scroll((skip_rows, 0)), input_area);
                let marker_cells = take_marker_cells(
                    frame.buffer_mut(),
                    input_area,
                    &markers,
                    line_height,
                    skip_rows as usize,
                );

                if !cursor_set && line_idx == app.cursor_y() {
                    let cursor = marker_cells.cursor.or_else(|| {
                        (app.cursor_x() == 0 && skip_rows == 0 && input_area.width > 0).then_some(
                            Position {
                                x: input_area.x,
                                y: input_area.y,
                            },
                        )
                    });
                    if let Some(cursor) = cursor {
                        frame.set_cursor_position(cursor);
                        cursor_set = true;
                    }
                }

                // Anchor the result to the final wrapped row of the expression.
                // A trailing comment may continue below it without moving the result.
                if let (Some(result_text), Some(result_row)) = (result_text, result_row) {
                    let visible_result_row = result_row.checked_sub(skip_rows as usize);
                    if let Some(row) = visible_result_row.filter(|row| *row < visible_rows as usize)
                    {
                        let result_y = result_area.y + row as u16;
                        let anchor_area = Rect {
                            x: result_area.x,
                            y: result_y,
                            width: result_area.width,
                            height: 1,
                        };
                        let result_para =
                            Paragraph::new(highlighted_value_line(result_text)).right_aligned();
                        frame.render_widget(result_para, anchor_area);
                    }
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
    let start = app.viewport_y().min(app.lines().len());
    let end = (start + area.height as usize).min(app.lines().len());
    let lines: Vec<Line> = (start..end)
        .map(|i| {
            let num = (i + 1).to_string();
            let style = if i == app.cursor_y() {
                Style::new().fg(palette::ACCENT).bold()
            } else {
                Style::new().fg(palette::DIM)
            };
            Line::from(Span::styled(num, style))
        })
        .collect();

    frame.render_widget(Paragraph::new(lines), area);
}

fn draw_input(frame: &mut Frame, area: Rect, app: &App) {
    let variables = app.variable_names();
    let start = app.viewport_y().min(app.lines().len());
    let end = (start + area.height as usize).min(app.lines().len());
    let lines: Vec<Line> = app.lines()[start..end]
        .iter()
        .map(|line| highlight_line(line, variables))
        .collect();

    let paragraph = Paragraph::new(lines).scroll((0, app.viewport_x() as u16));
    frame.render_widget(paragraph, area);

    // Set terminal cursor position
    let cursor_screen_x = area.x + (app.cursor_x().saturating_sub(app.viewport_x())) as u16;
    let cursor_screen_y = area.y + (app.cursor_y().saturating_sub(app.viewport_y())) as u16;

    if cursor_screen_x < area.x + area.width && cursor_screen_y < area.y + area.height {
        frame.set_cursor_position(Position {
            x: cursor_screen_x,
            y: cursor_screen_y,
        });
    }
}

fn draw_results(frame: &mut Frame, area: Rect, app: &App) {
    let start = app.viewport_y().min(app.lines().len());
    let end = (start + area.height as usize).min(app.lines().len());
    let lines: Vec<Line> = (start..end)
        .map(|line_idx| {
            app.result_text(line_idx)
                .map(highlighted_value_line)
                .unwrap_or_default()
        })
        .collect();

    let paragraph = Paragraph::new(lines).right_aligned();
    frame.render_widget(paragraph, area);
}

fn highlighted_value_line(text: &str) -> Line<'static> {
    highlight_line(text, &HashSet::new())
}

fn total_line(text: &str) -> Line<'static> {
    let mut spans = vec!["total: ".dim()];
    spans.extend(
        highlighted_value_line(text)
            .spans
            .into_iter()
            .map(Stylize::bold),
    );
    Line::from(spans)
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

/// Sample the brand gradient only for the genuinely time-varying loading state.
fn loading_pulse_color(start: std::time::Instant) -> Color {
    palette::BRAND_GRADIENT.pulse(start, std::time::Duration::from_millis(1800))
}

/// Calculate footer height needed (1 or 2 rows based on content width)
pub fn footer_height(app: &App, area_width: u16) -> u16 {
    let totals_str = app.totals_text();
    if totals_str.is_empty() {
        return 1;
    }

    // Estimate hints width (mode + filename + hints ≈ 40-50 chars typically)
    let hints_width_estimate = HINTS_WIDTH_ESTIMATE;
    // "total: " prefix + values
    let totals_width = (totals_str.chars().count() + 7) as u16;

    // If both fit on one line with some padding, use 1 row
    if hints_width_estimate + totals_width + 4 <= area_width {
        1
    } else {
        2
    }
}

fn draw_footer(frame: &mut Frame, area: Rect, app: &App, _result_width: u16) {
    let totals_str = app.totals_text();
    let use_two_rows = area.height >= 2 && !totals_str.is_empty();

    // Build hints line
    let hints = build_hints_line(app);

    if use_two_rows {
        // Two-row layout: totals on top (right), hints on bottom (space-between)
        let [totals_area, hints_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(area);

        // Totals row (right-aligned): "total:" dim, values bold
        let totals_widget = Paragraph::new(total_line(totals_str)).right_aligned();
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
            (totals_str.chars().count() + 7) as u16
        };

        let [left_area, right_area] =
            Layout::horizontal([Constraint::Fill(1), Constraint::Length(totals_width)]).areas(area);

        let left_footer = Paragraph::new(Line::from(hints));
        frame.render_widget(left_footer, left_area);

        if !totals_str.is_empty() {
            let right_footer = Paragraph::new(total_line(totals_str)).right_aligned();
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
    if app.is_dirty() {
        left.push(" •".fg(palette::NUMBER));
    }

    // Right part: keybindings (different for each mode)
    let mut right: Vec<Span<'static>> = Vec::new();

    match app.keybinding_mode {
        KeybindingMode::Vim => {
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
        }
        KeybindingMode::Standard => {
            right.push("F1".fg(palette::ACCENT));
            right.push(" help ".dim());
            right.push("^s".fg(palette::ACCENT));
            right.push(" save ".dim());
            right.push("^q".fg(palette::ACCENT));
            right.push(" quit ".dim());
        }
    }

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
        let bg = match msg.as_str() {
            crate::app::STATUS_SAVED => palette::VARIABLE,
            crate::app::STATUS_RATES_UNAVAILABLE => palette::ERROR,
            _ => palette::ACCENT,
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
        match app.keybinding_mode {
            KeybindingMode::Standard => " STANDARD ".fg(Color::Black).bg(palette::OPERATOR).bold(),
            KeybindingMode::Vim => match app.mode {
                InputMode::Normal => " NORMAL ".fg(Color::Black).bg(palette::ACCENT).bold(),
                InputMode::Insert => " INSERT ".fg(Color::Black).bg(palette::VARIABLE).bold(),
            },
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

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, layout::Rect, Terminal};

    /// Extract (text, color) pairs from tokenized spans for testing
    fn tokenize_to_pairs(input: &str) -> Vec<(String, Color)> {
        tokenize_and_style(input, &HashSet::new())
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
    fn token_types_map_to_the_tui_palette() {
        let cases = [
            (TokenType::Number, palette::NUMBER),
            (TokenType::Operator, palette::OPERATOR),
            (TokenType::Variable, palette::VARIABLE),
            (TokenType::Unit, palette::UNIT),
            (TokenType::Currency, palette::UNIT),
            (TokenType::Keyword, palette::KEYWORD),
            (TokenType::Function, palette::OPERATOR),
            (TokenType::Comment, palette::DIM),
            (TokenType::Text, palette::TEXT),
            (TokenType::Whitespace, Color::Reset),
            (TokenType::Punctuation, palette::DIM),
        ];
        for (token_type, expected) in cases {
            assert_eq!(token_color(token_type), expected, "{token_type:?}");
        }
    }

    #[test]
    fn mixed_tokens_are_styled_without_changing_text() {
        let input = "subtotal = sum($100, 20%) in USD # note";
        let pairs = tokenize_to_pairs(input);
        assert_eq!(
            pairs
                .iter()
                .map(|(text, _)| text.as_str())
                .collect::<String>(),
            input
        );
        for (text, color) in [
            ("subtotal", palette::VARIABLE),
            ("=", palette::OPERATOR),
            ("sum", palette::OPERATOR),
            ("$", palette::UNIT),
            ("100", palette::NUMBER),
            ("in", palette::KEYWORD),
            ("USD", palette::UNIT),
            ("# note", palette::DIM),
        ] {
            assert!(has_token(&pairs, text, color), "missing {text:?}");
        }
    }

    #[test]
    fn results_and_totals_use_the_token_palette() {
        let result = highlighted_value_line("$120  90 km");
        let total = total_line("$120  90 km");

        for line in [result, total] {
            let pairs = line
                .spans
                .iter()
                .map(|span| {
                    (
                        span.content.to_string(),
                        span.style.fg.unwrap_or(Color::Reset),
                    )
                })
                .collect::<Vec<_>>();
            assert!(has_token(&pairs, "$", palette::UNIT));
            assert!(has_token(&pairs, "120", palette::NUMBER));
            assert!(has_token(&pairs, "90", palette::NUMBER));
            assert!(has_token(&pairs, "km", palette::UNIT));
        }
    }

    #[test]
    fn test_known_variable_reference() {
        let pairs = tokenize_and_style("100 + tax", &HashSet::from(["tax".to_string()]))
            .into_iter()
            .map(|span| {
                (
                    span.content.to_string(),
                    span.style.fg.unwrap_or(Color::Reset),
                )
            })
            .collect::<Vec<_>>();
        assert!(has_token(&pairs, "tax", palette::VARIABLE));
    }

    #[test]
    fn test_viewport_dimensions_non_wrap() {
        let app = App::default();
        let (width, height) = viewport_dimensions(
            &app,
            Rect {
                x: 0,
                y: 0,
                width: 80,
                height: 24,
            },
        );

        assert_eq!(width, 68);
        assert_eq!(height, 23);
    }

    #[test]
    fn test_viewport_dimensions_wrap_mode() {
        let mut app = App::default();
        app.wrap_mode = true;
        app.show_line_numbers = true;
        app.set_lines_for_test(vec!["1".to_string(); 120]);
        let (width, height) = viewport_dimensions(
            &app,
            Rect {
                x: 0,
                y: 0,
                width: 80,
                height: 24,
            },
        );

        assert_eq!(width, 66);
        assert_eq!(height, 23);
    }

    #[test]
    fn draws_small_terminal_in_both_layout_modes() {
        for wrap_mode in [false, true] {
            let backend = TestBackend::new(12, 4);
            let mut terminal = Terminal::new(backend).unwrap();
            let mut app = App::default();
            app.wrap_mode = wrap_mode;
            app.show_line_numbers = true;
            app.set_lines_for_test(vec![
                "tax = 20%".into(),
                "100 + tax".into(),
                "unicode = 2 🧮".into(),
            ]);
            let (width, height) = viewport_dimensions(&app, Rect::new(0, 0, 12, 4));
            app.set_viewport_size(width, height);

            terminal.draw(|frame| draw(frame, &app)).unwrap();

            assert_eq!(terminal.backend().buffer().area, Rect::new(0, 0, 12, 4));
        }
    }

    #[test]
    fn wrapped_result_follows_expression_end_not_trailing_comment() {
        for comment in [
            " // comment comment comment",
            "//comment-comment-comment",
            "#comment-comment-comment",
        ] {
            let backend = TestBackend::new(32, 8);
            let mut terminal = Terminal::new(backend).unwrap();
            let mut app = App::default();
            app.wrap_mode = true;
            let line = format!("1 + 2 + 3 + 4 + 5 + 6 + 7{comment}");
            app.set_lines_for_test(vec![line.clone()]);
            let (width, height) = viewport_dimensions(&app, Rect::new(0, 0, 32, 8));
            app.set_viewport_size(width, height);

            assert!(app.get_wrapped_height(&line) > 2);
            terminal.draw(|frame| draw(frame, &app)).unwrap();

            let buffer = terminal.backend().buffer();
            let expression_row = (0..buffer.area.height)
                .find(|&y| (0..20).any(|x| buffer.cell((x, y)).unwrap().symbol() == "7"))
                .expect("expression anchor should be visible");
            let result_row = (0..buffer.area.height)
                .find(|&y| {
                    buffer.cell((30, y)).unwrap().symbol() == "2"
                        && buffer.cell((31, y)).unwrap().symbol() == "8"
                })
                .expect("result should be visible");

            assert_eq!(result_row, expression_row, "{comment}");
            assert!(
                (result_row + 1..buffer.area.height).any(|y| (0..20).any(|x| buffer
                    .cell((x, y))
                    .unwrap()
                    .symbol()
                    != " ")),
                "comment should wrap below the result: {comment}"
            );
        }
    }
}
