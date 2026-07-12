//! Typed errors exposed by the pure core API.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Why an input line could not be parsed.
#[derive(Debug, Clone, PartialEq, Eq, Error, Serialize, Deserialize)]
pub enum ParseError {
    #[error("input is too long ({actual} bytes; maximum is {max})")]
    InputTooLong { actual: usize, max: usize },
    #[error("expression is too complex ({actual} operations; maximum is {max})")]
    TooComplex { actual: usize, max: usize },
    #[error("expression is nested too deeply ({actual}; maximum is {max})")]
    TooDeep { actual: usize, max: usize },
    #[error("could not understand line")]
    InvalidSyntax,
    #[error("invalid expression: {0}")]
    InvalidExpression(String),
}

/// Why a parsed expression could not be evaluated.
#[derive(Debug, Clone, PartialEq, Eq, Error, Serialize)]
pub enum EvalError {
    #[error("{0}")]
    Parse(#[from] ParseError),
    #[error("arithmetic overflow while {operation}")]
    Overflow { operation: &'static str },
    #[error("division by zero")]
    DivisionByZero,
    #[error("unknown variable: {0}")]
    UnknownVariable(String),
    #[error("unknown function: {0}")]
    UnknownFunction(String),
    #[error("unknown target unit: {0}")]
    UnknownTarget(String),
    #[error("{0}")]
    InvalidOperands(String),
    #[error("{0}")]
    InvalidArgument(String),
    #[error("{0}")]
    Message(String),
}

impl EvalError {
    #[must_use]
    pub fn overflow(operation: &'static str) -> Self {
        Self::Overflow { operation }
    }
}

impl From<String> for EvalError {
    fn from(message: String) -> Self {
        Self::Message(message)
    }
}

impl From<&str> for EvalError {
    fn from(message: &str) -> Self {
        Self::Message(message.to_string())
    }
}

/// Filesystem/cache failures are kept separate from evaluation failures.
#[derive(Debug, Error)]
pub enum RateError {
    #[error("rate cache location is unavailable")]
    CacheLocationUnavailable,
    #[error("system clock is before the Unix epoch")]
    Clock,
    #[error("failed to read rate cache: {0}")]
    Read(#[source] std::io::Error),
    #[error("failed to create rate cache directory: {0}")]
    CreateDirectory(#[source] std::io::Error),
    #[error("failed to write rate cache: {0}")]
    Write(#[source] std::io::Error),
    #[error("invalid rate cache: {0}")]
    Deserialize(#[source] serde_json::Error),
    #[error("failed to serialize rate cache: {0}")]
    Serialize(#[source] serde_json::Error),
    #[error("invalid exchange rates: {0}")]
    InvalidRates(String),
    #[error("filesystem rate caching is unavailable on this platform")]
    UnsupportedPlatform,
    #[error("rate request failed: {0}")]
    Network(String),
    #[error("invalid rate response: {0}")]
    Response(String),
}
