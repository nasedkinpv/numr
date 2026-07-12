//! Language metadata shared by evaluators, editors, and other adapters.

use serde::Serialize;

use crate::CURRENCIES;

/// Functions accepted by the evaluator.
pub const BUILTIN_FUNCTIONS: &[&str] = &[
    "sum",
    "total",
    "avg",
    "average",
    "min",
    "max",
    "median",
    "clamp",
    "abs",
    "round",
    "floor",
    "ceil",
    "sin",
    "cos",
    "tan",
    "rad",
    "radians",
    "deg",
    "degrees",
    "sinh",
    "cosh",
    "tanh",
    "exp",
    "ln",
    "log",
    "sqrt",
    "factorial",
    "mod",
    "log_y",
];

/// Word operators recognized by the grammar.
pub const KEYWORDS: &[&str] = &["of", "in", "to"];

/// Built-in mathematical constants.
pub const MATH_CONSTANTS: &[&str] = &["pi", "e", "phi"];

/// Aliases for the previous successful value.
pub const ANSWER_ALIASES: &[&str] = &["_", "ANS", "ans"];

#[must_use]
pub fn is_builtin_function(name: &str) -> bool {
    BUILTIN_FUNCTIONS
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(name))
}

/// Stable transport metadata for currency pickers and rate providers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CurrencyMetadata {
    pub code: &'static str,
    pub symbol: &'static str,
    pub is_crypto: bool,
    pub coingecko_id: Option<&'static str>,
    pub display_precision: u32,
}

#[must_use]
pub fn currency_catalog() -> Vec<CurrencyMetadata> {
    CURRENCIES
        .iter()
        .map(|definition| CurrencyMetadata {
            code: definition.code,
            symbol: definition.symbol,
            is_crypto: definition.is_crypto,
            coingecko_id: definition.coingecko_id,
            display_precision: definition.display_precision,
        })
        .collect()
}
