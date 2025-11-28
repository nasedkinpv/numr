//! Popup dialogs (help, quit confirmation)

use ratatui::{
    layout::{Constraint, Flex, Layout, Rect},
    style::{Color, Style, Stylize},
    text::Line,
    widgets::{Block, Clear, Padding, Paragraph, Row, Table},
    Frame,
};

use crate::ui::palette;

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

/// Draw the help popup
pub fn draw_help_popup(frame: &mut Frame, area: Rect) {
    let rows = vec![
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
        Row::new(vec!["w", "Toggle wrap mode"]),
        Row::new(vec!["n", "Toggle line numbers"]),
        Row::new(vec!["Ctrl+s", "Save file"]),
        Row::new(vec!["F12", "Toggle debug"]),
        Row::new(vec!["? / F1", "Toggle help"]),
        Row::new(vec!["q / Esc", "Quit / Close help"]),
    ];

    // Calculate dimensions: rows + header (2) + borders (2) + padding (2)
    let content_height = (rows.len() as u16 + 6).min(area.height.saturating_sub(4));
    let content_width = 50_u16.min(area.width.saturating_sub(4));

    let popup_area = centered_rect(area, content_width, content_height);

    frame.render_widget(Clear, popup_area);

    let table = Table::new(
        rows,
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
