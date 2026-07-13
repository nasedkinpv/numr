//! Shared terminal color palette.

use ratatui::style::Color;

pub const DIM: Color = Color::DarkGray;
pub const ACCENT: Color = Color::Cyan;
pub const NUMBER: Color = Color::Yellow;
pub const OPERATOR: Color = Color::Magenta;
pub const VARIABLE: Color = Color::LightGreen;
pub const UNIT: Color = Color::Blue;
pub const ERROR: Color = Color::Red;
pub const KEYWORD: Color = Color::Cyan;
pub const TEXT: Color = Color::Gray;
pub const POPUP_BG: Color = Color::Black;

/// Generate the popup separator gradient at position `t` (0.0 to 1.0).
pub fn gradient(t: f32) -> Color {
    let r = (80.0 + t * 100.0) as u8;
    let g = (180.0 - t * 80.0) as u8;
    Color::Rgb(r, g, 220)
}
