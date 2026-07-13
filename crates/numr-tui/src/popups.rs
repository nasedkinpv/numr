//! Popup dialogs (help, quit confirmation)

use ratatui::{
    layout::{Constraint, Flex, Layout, Rect},
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{
        Block, Clear, Padding, Paragraph, Row, Scrollbar, ScrollbarOrientation, ScrollbarState,
        Table,
    },
    Frame,
};

use crate::app::KeybindingMode;
use crate::theme as palette;

/// Render the shared popup background and optional titled separator.
fn render_popup_frame(
    frame: &mut Frame,
    area: Rect,
    width: u16,
    height: u16,
    title: Option<&str>,
) -> Rect {
    let popup_area = centered_rect(area, width, height);

    // Clear and background
    frame.render_widget(Clear, popup_area);
    frame.render_widget(
        Block::new().style(Style::new().bg(palette::POPUP_BG)),
        popup_area,
    );

    let mut content_start_y: u16 = 0;

    // Title
    if let Some(title_text) = title {
        let title_area = Rect {
            x: popup_area.x,
            y: popup_area.y,
            width: popup_area.width,
            height: 2,
        };
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::from(vec![
                    "  ".into(),
                    Span::styled(title_text, Style::new().fg(palette::ACCENT).bold()),
                ]),
            ]),
            title_area,
        );
        content_start_y = 2;
    }

    // Gradient separator
    if title.is_some() {
        draw_separator(frame, popup_area, content_start_y);
        content_start_y += 1;
    }

    // Return content area
    Rect {
        x: popup_area.x,
        y: popup_area.y + content_start_y,
        width: popup_area.width,
        height: popup_area.height.saturating_sub(content_start_y),
    }
}

/// Draw gradient separator line
fn draw_separator(frame: &mut Frame, area: Rect, y_offset: u16) {
    let width = area.width.saturating_sub(4) as usize; // padding
    let spans: Vec<Span> = (0..width)
        .map(|i| {
            let t = i as f32 / (width - 1).max(1) as f32;
            Span::styled("─", Style::new().fg(palette::gradient(t)))
        })
        .collect();

    let sep_area = Rect {
        x: area.x + 2,
        y: area.y + y_offset,
        width: area.width.saturating_sub(4),
        height: 1,
    };
    frame.render_widget(Paragraph::new(Line::from(spans)), sep_area);
}

/// Draw the quit confirmation popup
pub fn draw_quit_popup(frame: &mut Frame, area: Rect) {
    let content_area = render_popup_frame(frame, area, 34, 8, None);

    let text = vec![
        Line::from(""),
        Line::from("You have unsaved changes.").bold(),
        Line::from(""),
        Line::from("Save before quitting?"),
        Line::from(""),
        Line::from(vec![
            "[y]".fg(palette::VARIABLE).bold(),
            " yes  ".into(),
            "[n]".fg(palette::ERROR).bold(),
            " no  ".into(),
            "[esc]".fg(palette::DIM).bold(),
            " cancel".into(),
        ]),
    ];

    let paragraph = Paragraph::new(text)
        .block(Block::new().padding(Padding::horizontal(2)))
        .alignment(ratatui::layout::Alignment::Center);

    frame.render_widget(paragraph, content_area);
}

/// Draw the help popup with scroll support
pub fn draw_help_popup(
    frame: &mut Frame,
    area: Rect,
    scroll_offset: usize,
    keybinding_mode: KeybindingMode,
) {
    let (all_rows, title) = match keybinding_mode {
        KeybindingMode::Vim => (vim_help_rows(), "Help (Vim)"),
        KeybindingMode::Standard => (standard_help_rows(), "Help (Standard)"),
    };
    let help_rows_count = all_rows.len();

    let max_visible_rows = area.height.saturating_sub(6) as usize;
    let content_height = (max_visible_rows + 5).min(area.height.saturating_sub(4) as usize) as u16;
    let content_width = 46_u16.min(area.width.saturating_sub(6));

    // Render popup frame (clear, background, title, separator)
    let content_area = render_popup_frame(frame, area, content_width, content_height, Some(title));

    // Slice rows based on scroll offset
    let visible_rows: Vec<Row> = all_rows
        .into_iter()
        .skip(scroll_offset)
        .take(max_visible_rows)
        .collect();

    let needs_scroll = help_rows_count > max_visible_rows;

    // Render table in content area
    let table = Table::new(
        visible_rows,
        [Constraint::Percentage(45), Constraint::Percentage(55)],
    )
    .block(Block::new().padding(Padding::horizontal(2)))
    .column_spacing(1);

    frame.render_widget(table, content_area);

    // Draw scrollbar if content overflows
    if needs_scroll {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .track_symbol(Some("│"))
            .track_style(Style::new().fg(Color::DarkGray))
            .thumb_symbol("┃")
            .thumb_style(Style::new().fg(palette::DIM));

        let max_scroll = help_rows_count.saturating_sub(max_visible_rows);
        let mut scrollbar_state = ScrollbarState::new(max_scroll).position(scroll_offset);

        let scrollbar_area = Rect {
            x: content_area.x + content_area.width - 2,
            y: content_area.y,
            width: 1,
            height: content_area.height.saturating_sub(1),
        };

        frame.render_stateful_widget(scrollbar, scrollbar_area, &mut scrollbar_state);
    }
}

/// Help rows for Vim mode
fn vim_help_rows() -> Vec<Row<'static>> {
    vec![
        Row::new(vec!["Shift+Tab", "Switch to Standard mode"]),
        Row::new(vec!["", ""]),
        Row::new(vec!["Navigation", ""]).style(Style::new().bold().fg(palette::VARIABLE)),
        Row::new(vec!["h/j/k/l", "Move cursor"]),
        Row::new(vec!["w / b / e", "Word forward/back/end"]),
        Row::new(vec!["0 / $", "Line start/end"]),
        Row::new(vec!["gg / G", "First/last line"]),
        Row::new(vec!["PageUp / PageDown", "Scroll page"]),
        Row::new(vec!["", ""]),
        Row::new(vec!["Insert Mode", ""]).style(Style::new().bold().fg(palette::VARIABLE)),
        Row::new(vec!["i / a", "Insert at/after cursor"]),
        Row::new(vec!["I / A", "Insert at line start/end"]),
        Row::new(vec!["o / O", "New line below/above"]),
        Row::new(vec!["s", "Substitute char"]),
        Row::new(vec!["C", "Change to end of line"]),
        Row::new(vec!["Option+Backspace / Ctrl+w", "Delete previous word"]),
        Row::new(vec!["Cmd+Backspace / Ctrl+u", "Delete to line start"]),
        Row::new(vec!["Esc", "Back to normal mode"]),
        Row::new(vec!["", ""]),
        Row::new(vec!["Editing", ""]).style(Style::new().bold().fg(palette::VARIABLE)),
        Row::new(vec!["x / X", "Delete char fwd/back"]),
        Row::new(vec!["dd", "Delete line"]),
        Row::new(vec!["D", "Delete to end of line"]),
        Row::new(vec!["J", "Join lines"]),
        Row::new(vec!["", ""]),
        Row::new(vec!["General", ""]).style(Style::new().bold().fg(palette::VARIABLE)),
        Row::new(vec!["W / N / H", "Toggle wrap/numbers/header"]),
        Row::new(vec!["Ctrl+s", "Save file"]),
        Row::new(vec!["Ctrl+r", "Refresh rates"]),
        Row::new(vec!["F12", "Toggle debug"]),
        Row::new(vec!["? / F1", "Toggle help"]),
        Row::new(vec!["q", "Quit"]),
    ]
}

/// Help rows for Standard mode
fn standard_help_rows() -> Vec<Row<'static>> {
    vec![
        Row::new(vec!["Shift+Tab", "Switch to Vim mode"]),
        Row::new(vec!["", ""]),
        Row::new(vec!["Navigation", ""]).style(Style::new().bold().fg(palette::VARIABLE)),
        Row::new(vec!["Arrow keys", "Move cursor"]),
        Row::new(vec!["Home / End", "Line start/end"]),
        Row::new(vec!["PageUp / PageDown", "Scroll page"]),
        Row::new(vec!["Ctrl+g", "Go to first line"]),
        Row::new(vec!["Ctrl+a / Ctrl+e", "Line start/end"]),
        Row::new(vec!["", ""]),
        Row::new(vec!["Editing", ""]).style(Style::new().bold().fg(palette::VARIABLE)),
        Row::new(vec!["Type directly", "Insert text"]),
        Row::new(vec!["Backspace / Delete", "Delete char"]),
        Row::new(vec!["Option+Backspace / Ctrl+w", "Delete previous word"]),
        Row::new(vec!["Cmd+Backspace / Ctrl+u", "Delete to line start"]),
        Row::new(vec!["Ctrl+k", "Delete to line end"]),
        Row::new(vec!["Enter", "New line"]),
        Row::new(vec!["", ""]),
        Row::new(vec!["General", ""]).style(Style::new().bold().fg(palette::VARIABLE)),
        Row::new(vec!["? / F1", "Toggle help"]),
        Row::new(vec!["Option+z", "Toggle wrap"]),
        Row::new(vec!["Ctrl+l / Ctrl+h", "Toggle numbers/header"]),
        Row::new(vec!["Ctrl+s", "Save file"]),
        Row::new(vec!["Ctrl+r", "Refresh rates"]),
        Row::new(vec!["Ctrl+q", "Quit"]),
    ]
}

/// Calculate max scroll offset for help popup
pub fn help_max_scroll(area_height: u16, keybinding_mode: KeybindingMode) -> usize {
    let max_visible = area_height.saturating_sub(6) as usize;
    let rows_count = match keybinding_mode {
        KeybindingMode::Vim => vim_help_rows().len(),
        KeybindingMode::Standard => standard_help_rows().len(),
    };
    rows_count.saturating_sub(max_visible)
}

/// Center a rect with fixed width and height
fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let [area] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(area);
    let [area] = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .areas(area);
    area
}
