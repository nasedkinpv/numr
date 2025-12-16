pub mod buffer;
pub mod highlight;

#[cfg(feature = "wasm")]
pub mod wasm;

pub use buffer::TextBuffer;
pub use highlight::{tokenize, Token, TokenType};
