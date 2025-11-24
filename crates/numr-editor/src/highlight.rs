use numr_core::{Currency, Unit};
use std::collections::HashSet;
use std::sync::LazyLock;

/// Semantic token types (UI-agnostic)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenType {
    Number,
    Operator,
    Variable,
    Unit,
    Currency,
    Keyword,      // "in", "of", "to"
    Function,     // "sum", "avg", etc.
    Comment,
    Text,         // Unrecognized prose
    Whitespace,
    Punctuation,
}

/// A token with its text and semantic type
#[derive(Clone, Debug)]
pub struct Token {
    pub text: String,
    pub token_type: TokenType,
}

/// Cached sets for syntax highlighting - built from registries
static CURRENCY_SYMBOLS: LazyLock<HashSet<char>> = LazyLock::new(|| {
    Currency::all_symbols()
        .filter_map(|s| s.chars().next())
        .collect()
});

static CURRENCY_WORDS: LazyLock<HashSet<String>> = LazyLock::new(|| {
    Currency::all_aliases()
        .map(|s| s.to_lowercase())
        .chain(Currency::all_codes().map(|s| s.to_lowercase()))
        .collect()
});

static UNIT_WORDS: LazyLock<HashSet<String>> = LazyLock::new(|| {
    Unit::all_aliases()
        .map(|s| s.to_lowercase())
        .chain(Unit::all_short_names().map(|s| s.to_lowercase()))
        .collect()
});

/// Check if a character is a currency symbol
fn is_currency_symbol(c: char) -> bool {
    CURRENCY_SYMBOLS.contains(&c)
}

/// Check if a word is a currency code/name
fn is_currency_word(word: &str) -> bool {
    CURRENCY_WORDS.contains(&word.to_lowercase())
}

/// Check if a word is a unit
fn is_unit_word(word: &str) -> bool {
    UNIT_WORDS.contains(&word.to_lowercase())
}

/// Keywords for syntax highlighting
static KEYWORDS: &[&str] = &["of", "in", "to"];
static FUNCTIONS: &[&str] = &[
    "sum", "avg", "average", "min", "max", "abs", "sqrt", "round", "floor", "ceil", "total",
];

/// Tokenize input and apply syntax highlighting
pub fn tokenize(input: &str) -> Vec<Token> {
    let trimmed = input.trim_start();

    // Comment lines (starting with #)
    if trimmed.starts_with('#') {
        return vec![Token {
            text: input.to_string(),
            token_type: TokenType::Comment,
        }];
    }

    let mut tokens = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    // Check if line has assignment (word = ...) to identify variable definition
    let assignment_var = find_assignment_variable(input);

    while i < chars.len() {
        let c = chars[i];

        if c.is_ascii_digit() || (c == '-' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit())
        {
            // Numbers (including negative and percentages)
            let start = i;
            if c == '-' {
                i += 1;
            }
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            if i < chars.len() && chars[i] == '%' {
                i += 1;
            }
            let num: String = chars[start..i].iter().collect();
            tokens.push(Token {
                text: num,
                token_type: TokenType::Number,
            });
        } else if is_currency_symbol(c) {
            // Currency symbols (from registry)
            tokens.push(Token {
                text: c.to_string(),
                token_type: TokenType::Currency,
            });
            i += 1;
        } else if c == '+' || c == '*' || c == '/' || c == '^' || c == '×' || c == '÷' {
            tokens.push(Token {
                text: c.to_string(),
                token_type: TokenType::Operator,
            });
            i += 1;
        } else if c == 'x' && is_multiply_context(&chars, i) {
            // 'x' as multiplication operator (e.g., "2x3")
            tokens.push(Token {
                text: "x".to_string(),
                token_type: TokenType::Operator,
            });
            i += 1;
        } else if c == '-' {
            tokens.push(Token {
                text: "-".to_string(),
                token_type: TokenType::Operator,
            });
            i += 1;
        } else if c == '=' {
            tokens.push(Token {
                text: "=".to_string(),
                token_type: TokenType::Operator,
            });
            i += 1;
        } else if c.is_alphabetic() || c == '_' {
            // Words: check against registries
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            let lower = word.to_lowercase();

            let token_type = if KEYWORDS.contains(&lower.as_str()) {
                TokenType::Keyword
            } else if FUNCTIONS.contains(&lower.as_str()) {
                TokenType::Function
            } else if is_unit_word(&word) {
                TokenType::Unit
            } else if is_currency_word(&word) {
                TokenType::Currency
            } else if assignment_var.as_ref() == Some(&word) {
                // Variable being defined
                TokenType::Variable
            } else {
                // Unknown word - plain text
                TokenType::Text
            };

            tokens.push(Token {
                text: word,
                token_type,
            });
        } else if c == '(' || c == ')' || c == ',' {
            tokens.push(Token {
                text: c.to_string(),
                token_type: TokenType::Punctuation,
            });
            i += 1;
        } else if c == ' ' || c == '\t' {
            let start = i;
            while i < chars.len() && (chars[i] == ' ' || chars[i] == '\t') {
                i += 1;
            }
            let ws: String = chars[start..i].iter().collect();
            tokens.push(Token {
                text: ws,
                token_type: TokenType::Whitespace,
            });
        } else {
            // Unknown characters (punctuation, etc.)
            tokens.push(Token {
                text: c.to_string(),
                token_type: TokenType::Punctuation,
            });
            i += 1;
        }
    }

    tokens
}

/// Find variable name if line is an assignment (e.g., "tax = 20%" returns Some("tax"))
fn find_assignment_variable(input: &str) -> Option<String> {
    let parts: Vec<&str> = input.splitn(2, '=').collect();
    if parts.len() == 2 {
        let var_part = parts[0].trim();
        // Check it's a valid identifier
        if !var_part.is_empty()
            && var_part
                .chars()
                .next()
                .map(|c| c.is_alphabetic() || c == '_')
                .unwrap_or(false)
            && var_part.chars().all(|c| c.is_alphanumeric() || c == '_')
        {
            return Some(var_part.to_string());
        }
    }
    None
}

/// Check if 'x' at position i is likely a multiplication operator.
/// True if preceded by digit/)/% and followed by digit/(/currency symbol.
/// Skips whitespace when checking context.
fn is_multiply_context(chars: &[char], i: usize) -> bool {
    // Look backwards, skipping whitespace
    let prev_ok = {
        let mut j = i;
        while j > 0 && chars[j - 1] == ' ' {
            j -= 1;
        }
        j > 0 && {
            let p = chars[j - 1];
            p.is_ascii_digit() || p == ')' || p == '%'
        }
    };
    // Look forwards, skipping whitespace
    let next_ok = {
        let mut j = i + 1;
        while j < chars.len() && chars[j] == ' ' {
            j += 1;
        }
        j < chars.len() && {
            let n = chars[j];
            n.is_ascii_digit() || n == '(' || is_currency_symbol(n)
        }
    };
    prev_ok && next_ok
}
