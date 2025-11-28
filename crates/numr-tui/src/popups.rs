//! Popup dialogs (help, quit confirmation)

use ratatui::{
    layout::{Constraint, Flex, Layout, Rect},
    style::{Color, Style, Stylize},
    text::Line,
    widgets::{
        Block, Clear, Padding, Paragraph, Row, Scrollbar, ScrollbarOrientation, ScrollbarState,
        Table,
    },
    Frame,
};

use crate::ui::palette;

/// Number of rows in the help table (for scroll calculations)
pub const HELP_ROWS_COUNT: usize = 21;

/// Draw the quit confirmation popup
pub fn draw_quit_popup(frame: &mut Frame, area: Rect) {
    let text = vec![
        Line::from("You have unsaved changes."),
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

    // Calculate dimensions: content + borders (2) + padding (top:1 + bottom:1)
    let content_height = text.len() as u16 + 4;
    let content_width = 36_u16; // Fixed width for this dialog

    let popup_area = centered_rect(area, content_width, content_height);

    frame.render_widget(Clear, popup_area);

    let block = Block::bordered()
        .title(" Unsaved Changes ")
        .title_style(Style::new().bold().fg(palette::ERROR))
        .style(Style::new().bg(Color::Black))
        .border_style(Style::new().fg(palette::ERROR))
        .padding(Padding::vertical(1));

    let paragraph = Paragraph::new(text)
        .block(block)
        .alignment(ratatui::layout::Alignment::Center);

    frame.render_widget(paragraph, popup_area);
}

/// Draw the help popup with scroll support
pub fn draw_help_popup(frame: &mut Frame, area: Rect, scroll_offset: usize) {
    let all_rows = vec![
        Row::new(vec!["Navigation", ""]).style(Style::new().bold().fg(palette::VARIABLE)),
        Row::new(vec!["Arrows / hjkl", "Move cursor"]),
        Row::new(vec!["Home / 0", "Start of line"]),
        Row::new(vec!["End / $", "End of line"]),
        Row::new(vec!["PageUp / PageDown", "Scroll page"]),
        Row::new(vec!["", ""]),
        Row::new(vec!["Editing", ""]).style(Style::new().bold().fg(palette::VARIABLE)),
        Row::new(vec!["i / a", "Insert mode"]),
        Row::new(vec!["o", "New line below"]),
        Row::new(vec!["dd", "Delete line"]),
        Row::new(vec!["x", "Delete char"]),
        Row::new(vec!["", ""]),
        Row::new(vec!["General", ""]).style(Style::new().bold().fg(palette::VARIABLE)),
        Row::new(vec!["W", "Toggle wrap mode"]),
        Row::new(vec!["N", "Toggle line numbers"]),
        Row::new(vec!["H", "Toggle header"]),
        Row::new(vec!["Ctrl+s", "Save file"]),
        Row::new(vec!["Ctrl+r", "Refresh rates"]),
        Row::new(vec!["F12", "Toggle debug"]),
        Row::new(vec!["? / F1", "Toggle help"]),
        Row::new(vec!["q / Esc", "Quit / Close help"]),
    ];

    // Calculate dimensions: visible rows + header (2) + borders (2) + padding (2)
    let max_visible_rows = area.height.saturating_sub(8) as usize;
    let content_height = (max_visible_rows + 6).min(area.height.saturating_sub(4) as usize) as u16;
    let content_width = 50_u16.min(area.width.saturating_sub(4));

    let popup_area = centered_rect(area, content_width, content_height);

    frame.render_widget(Clear, popup_area);

    // Slice rows based on scroll offset
    let visible_rows: Vec<Row> = all_rows
        .into_iter()
        .skip(scroll_offset)
        .take(max_visible_rows)
        .collect();

    let needs_scroll = HELP_ROWS_COUNT > max_visible_rows;

    let table = Table::new(
        visible_rows,
        [Constraint::Percentage(45), Constraint::Percentage(55)],
    )
    .block(
        Block::bordered()
            .title(" Help ")
            .title_style(Style::new().bold().fg(palette::ACCENT))
            .style(Style::new().bg(Color::Black))
            .padding(Padding::horizontal(1)),
    )
    .header(
        Row::new(vec!["Key", "Action"])
            .style(Style::new().bold().fg(palette::ACCENT).bg(Color::DarkGray))
            .bottom_margin(1),
    )
    .column_spacing(1);

    frame.render_widget(table, popup_area);

    // Draw scrollbar if content overflows
    if needs_scroll {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .track_symbol(Some("│"))
            .thumb_symbol("█");

        let max_scroll = HELP_ROWS_COUNT.saturating_sub(max_visible_rows);
        let mut scrollbar_state = ScrollbarState::new(max_scroll).position(scroll_offset);

        // Scrollbar area inside the popup border
        let scrollbar_area = Rect {
            x: popup_area.x + popup_area.width - 2,
            y: popup_area.y + 3, // After header
            width: 1,
            height: popup_area.height.saturating_sub(4),
        };

        frame.render_stateful_widget(scrollbar, scrollbar_area, &mut scrollbar_state);
    }
}

/// Calculate max scroll offset for help popup
pub fn help_max_scroll(area_height: u16) -> usize {
    let max_visible = area_height.saturating_sub(8) as usize;
    HELP_ROWS_COUNT.saturating_sub(max_visible)
}

/// Center a rect with fixed width and height (modern ratatui approach)
/// See: https://ratatui.rs/recipes/layout/center-a-widget/
fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let [area] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(area);
    let [area] = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .areas(area);
    area
}
