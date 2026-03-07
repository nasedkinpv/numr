pub mod buffer;
pub mod highlight;

#[cfg(feature = "wasm")]
pub mod wasm;

pub use buffer::{char_to_byte_idx, TextBuffer};
pub use highlight::{tokenize, Token, TokenType};
