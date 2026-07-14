//! Shared highlighted-line construction and Ratatui wrap measurements.

use crate::theme as palette;
use numr_editor::{
    char_to_byte_idx, expression_prefix, tokenize, tokenize_with_variables, TokenType,
};
use ratatui::{
    buffer::{Buffer, CellWidth},
    layout::{Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget, Wrap},
};
use std::{collections::HashSet, ops::Range};
use unicode_segmentation::UnicodeSegmentation;

// This bit is private cursor metadata. It is removed from the frame buffer
// immediately after the rendered cell has been located.
const CURSOR_MARKER: Modifier = Modifier::SLOW_BLINK;

#[derive(Clone, Debug)]
struct CursorMarker {
    range: Range<usize>,
    after: bool,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct LineMarkers {
    cursor: Option<CursorMarker>,
}

impl LineMarkers {
    pub(crate) fn new(input: &str, cursor_x: Option<usize>) -> Self {
        let cursor = cursor_x.and_then(|cursor_x| cursor_marker(input, cursor_x));
        Self { cursor }
    }

    fn cursor_after(&self) -> bool {
        self.cursor.as_ref().is_some_and(|marker| marker.after)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct MarkerCells {
    pub(crate) cursor: Option<Position>,
}

fn wrap_width(width: usize) -> u16 {
    u16::try_from(width).unwrap_or(u16::MAX)
}

pub(crate) fn wrapped_height(input: &str, variables: &HashSet<String>, width: usize) -> usize {
    if input.is_empty() || width == 0 {
        return 1;
    }

    Paragraph::new(highlight_line(input, variables))
        .wrap(Wrap { trim: false })
        .line_count(wrap_width(width))
        .max(1)
}

/// Zero-based wrapped row containing the end of the executable expression.
pub(crate) fn wrapped_result_row(
    input: &str,
    variables: &HashSet<String>,
    width: usize,
) -> Option<usize> {
    let expression = expression_prefix(input);
    if expression.is_empty() || width == 0 {
        return None;
    }

    Some(
        Paragraph::new(highlight_line(expression, variables))
            .wrap(Wrap { trim: false })
            .line_count(wrap_width(width))
            .max(1)
            - 1,
    )
}

pub(crate) fn measure_wrapped_cursor(
    input: &str,
    variables: &HashSet<String>,
    cursor_x: usize,
    width: usize,
) -> (usize, usize) {
    if width == 0 {
        return (0, cursor_x);
    }

    let markers = LineMarkers::new(input, Some(cursor_x));
    let paragraph =
        Paragraph::new(marked_line(input, variables, &markers)).wrap(Wrap { trim: false });
    let height = paragraph.line_count(wrap_width(width)).max(1);
    let area = Rect::new(
        0,
        0,
        wrap_width(width),
        u16::try_from(height).unwrap_or(u16::MAX),
    );
    let mut buffer = Buffer::empty(area);
    paragraph.render(area, &mut buffer);

    take_marker_cells(&mut buffer, area, &markers, height, 0)
        .cursor
        .map_or((0, 0), |position| {
            (position.y as usize, position.x as usize)
        })
}

pub(crate) fn highlight_line(input: &str, variables: &HashSet<String>) -> Line<'static> {
    marked_line(input, variables, &LineMarkers::default())
}

pub(crate) fn marked_line(
    input: &str,
    variables: &HashSet<String>,
    markers: &LineMarkers,
) -> Line<'static> {
    Line::from(tokenize_and_style_with_markers(input, variables, markers))
}

pub(crate) fn token_color(token_type: TokenType) -> Color {
    match token_type {
        TokenType::Number => palette::NUMBER,
        TokenType::Operator => palette::OPERATOR,
        TokenType::Variable => palette::VARIABLE,
        TokenType::Unit | TokenType::Currency => palette::UNIT,
        TokenType::Keyword => palette::KEYWORD,
        TokenType::Function => palette::OPERATOR,
        TokenType::Comment => palette::DIM,
        TokenType::Text => palette::TEXT,
        TokenType::Whitespace => Color::Reset,
        TokenType::Punctuation => palette::DIM,
    }
}

#[cfg(test)]
pub(crate) fn tokenize_and_style(input: &str, variables: &HashSet<String>) -> Vec<Span<'static>> {
    tokenize_and_style_with_markers(input, variables, &LineMarkers::default())
}

fn tokenize_and_style_with_markers(
    input: &str,
    variables: &HashSet<String>,
    markers: &LineMarkers,
) -> Vec<Span<'static>> {
    let tokens = if variables.is_empty() {
        tokenize(input)
    } else {
        tokenize_with_variables(input, variables)
    };
    let mut byte_offset = 0;
    let mut spans = Vec::with_capacity(tokens.len() + 3);

    for token in tokens {
        let token_start = byte_offset;
        let token_end = token_start + token.text.len();
        byte_offset = token_end;

        // Ratatui treats zero-width space as a wrap boundary. Keep an adjacent
        // comment from moving the expression without changing visible text.
        if token.token_type == TokenType::Comment
            && input[..token_start]
                .chars()
                .next_back()
                .is_some_and(|c| !c.is_whitespace())
        {
            spans.push(Span::raw("\u{200b}"));
        }

        let cursor_range = markers.cursor.as_ref().map(|marker| &marker.range);
        if !range_intersects(token_start..token_end, cursor_range) {
            spans.push(Span::styled(
                token.text,
                Style::new().fg(token_color(token.token_type)),
            ));
            continue;
        }

        // The marker contributes at most two internal boundaries. Keep the
        // cuts on the stack because this path runs on every terminal redraw.
        let mut cuts = [0, token.text.len(), 0, 0];
        let mut cut_count = 2;
        add_marker_cuts(
            &mut cuts,
            &mut cut_count,
            token_start..token_end,
            cursor_range,
        );
        cuts[..cut_count].sort_unstable();
        let mut unique_count = 1;
        for index in 1..cut_count {
            if cuts[index] != cuts[unique_count - 1] {
                cuts[unique_count] = cuts[index];
                unique_count += 1;
            }
        }

        for pair in cuts[..unique_count].windows(2) {
            let local_start = pair[0];
            let local_end = pair[1];
            if local_start == local_end {
                continue;
            }

            let global_start = token_start + local_start;
            let mut style = Style::new().fg(token_color(token.token_type));
            if markers
                .cursor
                .as_ref()
                .is_some_and(|marker| marker.range.contains(&global_start))
            {
                style = style.add_modifier(CURSOR_MARKER);
            }
            spans.push(Span::styled(
                token.text[local_start..local_end].to_owned(),
                style,
            ));
        }
    }

    spans
}

fn add_marker_cuts(
    cuts: &mut [usize; 4],
    cut_count: &mut usize,
    token: Range<usize>,
    marker: Option<&Range<usize>>,
) {
    let Some(marker) = marker else {
        return;
    };
    for boundary in [marker.start, marker.end] {
        if token.start < boundary && boundary < token.end {
            cuts[*cut_count] = boundary - token.start;
            *cut_count += 1;
        }
    }
}

fn range_intersects(token: Range<usize>, marker: Option<&Range<usize>>) -> bool {
    marker.is_some_and(|marker| token.start < marker.end && marker.start < token.end)
}

fn cursor_marker(input: &str, cursor_x: usize) -> Option<CursorMarker> {
    let cursor_byte = char_to_byte_idx(input, cursor_x.min(input.chars().count()));
    if let Some((range, grapheme)) = grapheme_at(input, cursor_byte) {
        if !grapheme.chars().all(char::is_whitespace) {
            return Some(CursorMarker {
                range,
                after: false,
            });
        }
    }

    (cursor_byte > 0).then_some(CursorMarker {
        // Mark the prefix once. The last marker cell that Ratatui actually
        // rendered gives the correct insertion point even when wrap drops
        // boundary whitespace.
        range: 0..cursor_byte,
        after: true,
    })
}

fn grapheme_at(input: &str, byte: usize) -> Option<(Range<usize>, &str)> {
    input
        .grapheme_indices(true)
        .map(|(start, grapheme)| (start..start + grapheme.len(), grapheme))
        .find(|(range, _)| range.start <= byte && byte < range.end)
}

/// Locate and erase the private cursor marker from an already-rendered buffer.
pub(crate) fn take_marker_cells(
    buffer: &mut Buffer,
    area: Rect,
    markers: &LineMarkers,
    line_height: usize,
    skipped_rows: usize,
) -> MarkerCells {
    let mut cells = MarkerCells::default();

    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            let Some(cell) = buffer.cell_mut((x, y)) else {
                continue;
            };
            let cursor = cell.modifier.contains(CURSOR_MARKER);
            let cell_width = cell.symbol().cell_width();
            cell.modifier.remove(CURSOR_MARKER);

            if cursor && (cells.cursor.is_none() || markers.cursor_after()) {
                cells.cursor = cursor_position(
                    Position { x, y },
                    cell_width,
                    area,
                    markers.cursor_after(),
                    line_height,
                    skipped_rows,
                );
            }
        }
    }

    cells
}

fn cursor_position(
    marker: Position,
    cell_width: u16,
    area: Rect,
    after: bool,
    line_height: usize,
    skipped_rows: usize,
) -> Option<Position> {
    if !after {
        return Some(marker);
    }

    let next_x = marker.x.saturating_add(cell_width);
    if next_x < area.right() {
        return Some(Position {
            x: next_x,
            y: marker.y,
        });
    }

    let marker_row = skipped_rows + usize::from(marker.y.saturating_sub(area.y));
    if marker_row + 1 < line_height {
        (marker.y + 1 < area.bottom()).then_some(Position {
            x: area.x,
            y: marker.y + 1,
        })
    } else {
        Some(Position {
            x: area.right().saturating_sub(1),
            y: marker.y,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn result_row_ignores_comments_and_trailing_whitespace() {
        let variables = HashSet::new();
        for (expression, suffix) in [
            ("1 + 2", "   "),
            ("90° to rad", " # trailing comment that wraps"),
            ("é + 12345", " // trailing comment that wraps"),
            ("🧮 + 12345", "#comment"),
        ] {
            for width in [1, 2, 3, 5, 8, 13, 21] {
                assert_eq!(
                    wrapped_result_row(&format!("{expression}{suffix}"), &variables, width),
                    wrapped_result_row(expression, &variables, width),
                    "expression={expression:?}, width={width}"
                );
            }
        }

        assert_eq!(highlight_line("1#c", &variables).width(), 3);
    }
}
