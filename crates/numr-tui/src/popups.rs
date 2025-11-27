//! Popup dialogs (help, quit confirmation)

use ratatui::{
    layout::{Constraint, Flex, Layout, Rect},
    style::{Color, Style, Stylize},
    text::Line,
    widgets::{Block, Clear, Padding, Paragraph, Row, Table, Wrap},
    Frame,
};

use crate::ui::palette;

/// Draw the quit confirmation popup
pub fn draw_quit_popup(frame: &mut Frame, area: Rect) {
    let popup_area = centered_rect(area, 40, 20);

    frame.render_widget(Clear, popup_area);

    let block = Block::bordered()
        .title(" Unsaved Changes ")
        .title_style(Style::new().bold().fg(palette::ERROR))
        .style(Style::new().bg(Color::Black))
        .border_style(Style::new().fg(palette::ERROR))
        .padding(Padding::new(2, 2, 1, 1));

    let text = vec![
        Line::from("You have unsaved changes."),
        Line::from(""),
        Line::from(vec![
            "Save before quitting? ".into(),
            "(y/n/esc)".fg(palette::ACCENT).bold(),
        ]),
    ];

    let paragraph = Paragraph::new(text)
        .block(block)
        .alignment(ratatui::layout::Alignment::Center)
        .wrap(Wrap { trim: true });

    frame.render_widget(paragraph, popup_area);
}

/// Draw the help popup
pub fn draw_help_popup(frame: &mut Frame, area: Rect) {
    let popup_area = centered_rect(area, 60, 60);

    frame.render_widget(Clear, popup_area);

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

    let table = Table::new(
        rows,
        [Constraint::Percentage(40), Constraint::Percentage(60)],
    )
    .block(
        Block::bordered()
            .title(" Help ")
            .title_style(Style::new().bold().fg(palette::ACCENT))
            .style(Style::new().bg(Color::Black))
            .padding(Padding::new(2, 2, 1, 1)),
    )
    .header(
        Row::new(vec!["Key", "Action"])
            .style(Style::new().bold().fg(palette::ACCENT).bg(Color::DarkGray))
            .bottom_margin(1),
    )
    .column_spacing(1);

    frame.render_widget(table, popup_area);
}

/// Helper to center a rect using Flex layout
fn centered_rect(area: Rect, percent_x: u16, percent_y: u16) -> Rect {
    let [vertical] = Layout::vertical([Constraint::Percentage(percent_y)])
        .flex(Flex::Center)
        .areas(area);
    let [horizontal] = Layout::horizontal([Constraint::Percentage(percent_x)])
        .flex(Flex::Center)
        .areas(vertical);
    horizontal
}
