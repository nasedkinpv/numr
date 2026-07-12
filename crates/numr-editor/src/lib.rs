pub mod highlight;
mod text;

#[cfg(feature = "wasm")]
pub mod wasm;

pub use highlight::{tokenize, tokenize_with_variables, Token, TokenType};
pub use text::char_to_byte_idx;
