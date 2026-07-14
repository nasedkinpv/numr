//! Shared terminal color palette.

use ratatui::style::Color;
use std::time::{Duration, Instant};

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

#[derive(Clone, Copy)]
pub struct Gradient {
    start: (u8, u8, u8),
    end: (u8, u8, u8),
}

impl Gradient {
    pub const fn new(start: (u8, u8, u8), end: (u8, u8, u8)) -> Self {
        Self { start, end }
    }

    pub fn sample(self, position: f32) -> Color {
        let position = position.clamp(0.0, 1.0);
        let channel = |start: u8, end: u8| {
            (f32::from(start) + position * (f32::from(end) - f32::from(start))).round() as u8
        };
        Color::Rgb(
            channel(self.start.0, self.end.0),
            channel(self.start.1, self.end.1),
            channel(self.start.2, self.end.2),
        )
    }

    /// A smooth ping-pong sample for genuinely time-varying states.
    pub fn pulse(self, start: Instant, period: Duration) -> Color {
        let phase = start.elapsed().as_secs_f32() / period.as_secs_f32();
        let position = (1.0 - (phase * std::f32::consts::TAU).cos()) * 0.5;
        self.sample(position)
    }
}

/// numr's signature cyan-to-magenta terminal gradient.
pub const BRAND_GRADIENT: Gradient = Gradient::new((80, 180, 220), (180, 100, 220));
