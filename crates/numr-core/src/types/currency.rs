//! Currency definitions and handling
//!
//! To add a new currency, simply add an entry to the CURRENCIES array.
//! All parsing, display, and highlighting will automatically pick it up.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Currency metadata - single source of truth for each currency
pub struct CurrencyDef {
    /// The currency enum variant
    pub currency: Currency,
    /// Display symbol (e.g., "$", "€")
    pub symbol: &'static str,
    /// ISO 4217 code (e.g., "USD", "EUR")
    pub code: &'static str,
    /// All accepted aliases for parsing (lowercase)
    pub aliases: &'static [&'static str],
    /// Whether symbol appears after the number (e.g., "100₽" vs "$100")
    pub symbol_after: bool,
}

/// Complete registry of all supported currencies.
/// To add a new currency: add enum variant and add entry here.
pub static CURRENCIES: &[CurrencyDef] = &[
    CurrencyDef {
        currency: Currency::USD,
        symbol: "$",
        code: "USD",
        aliases: &["$", "usd", "dollars"],
        symbol_after: false,
    },
    CurrencyDef {
        currency: Currency::EUR,
        symbol: "€",
        code: "EUR",
        aliases: &["€", "eur", "euros"],
        symbol_after: false,
    },
    CurrencyDef {
        currency: Currency::GBP,
        symbol: "£",
        code: "GBP",
        aliases: &["£", "gbp", "pounds"],
        symbol_after: false,
    },
    CurrencyDef {
        currency: Currency::JPY,
        symbol: "¥",
        code: "JPY",
        aliases: &["¥", "jpy"],
        symbol_after: false,
    },
    CurrencyDef {
        currency: Currency::RUB,
        symbol: "₽",
        code: "RUB",
        aliases: &["₽", "rub", "rubles"],
        symbol_after: true,
    },
    CurrencyDef {
        currency: Currency::ILS,
        symbol: "₪",
        code: "ILS",
        aliases: &["₪", "ils"],
        symbol_after: false,
    },
    CurrencyDef {
        currency: Currency::BTC,
        symbol: "₿",
        code: "BTC",
        aliases: &["₿", "btc", "bitcoin"],
        symbol_after: false,
    },
];

/// Supported currencies
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Currency {
    USD,
    EUR,
    GBP,
    JPY,
    RUB,
    ILS,
    BTC,
}

impl Currency {
    /// Get the currency definition
    pub fn def(&self) -> &'static CurrencyDef {
        CURRENCIES
            .iter()
            .find(|d| d.currency == *self)
            .expect("All currencies must have definitions")
    }

    /// Get the currency symbol
    pub fn symbol(&self) -> &'static str {
        self.def().symbol
    }

    /// Get the ISO 4217 code
    pub fn code(&self) -> &'static str {
        self.def().code
    }

    /// Check if symbol appears after the number
    pub fn symbol_after(&self) -> bool {
        self.def().symbol_after
    }

    /// Get all currency symbols (for UI highlighting)
    pub fn all_symbols() -> impl Iterator<Item = &'static str> {
        CURRENCIES.iter().map(|d| d.symbol)
    }

    /// Get all currency codes (for UI highlighting)
    pub fn all_codes() -> impl Iterator<Item = &'static str> {
        CURRENCIES.iter().map(|d| d.code)
    }

    /// Get all currency aliases (for UI highlighting)
    pub fn all_aliases() -> impl Iterator<Item = &'static str> {
        CURRENCIES.iter().flat_map(|d| d.aliases.iter().copied())
    }

    /// Parse currency from string (symbol or code)
    pub fn parse(s: &str) -> Option<Currency> {
        let lower = s.to_lowercase();
        CURRENCIES
            .iter()
            .find(|d| {
                d.symbol == s
                    || d.code.eq_ignore_ascii_case(s)
                    || d.aliases.iter().any(|a| *a == lower || *a == s)
            })
            .map(|d| d.currency)
    }

    /// Iterator over all currencies
    pub fn all() -> impl Iterator<Item = Currency> {
        CURRENCIES.iter().map(|d| d.currency)
    }
}

impl fmt::Display for Currency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.code())
    }
}

impl std::str::FromStr for Currency {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Currency::parse(s).ok_or_else(|| format!("Unknown currency: {}", s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_currencies() {
        assert_eq!(Currency::parse("$"), Some(Currency::USD));
        assert_eq!(Currency::parse("USD"), Some(Currency::USD));
        assert_eq!(Currency::parse("usd"), Some(Currency::USD));
        assert_eq!(Currency::parse("dollars"), Some(Currency::USD));
        assert_eq!(Currency::parse("€"), Some(Currency::EUR));
        assert_eq!(Currency::parse("₿"), Some(Currency::BTC));
        assert_eq!(Currency::parse("bitcoin"), Some(Currency::BTC));
    }

    #[test]
    fn test_all_currencies_have_defs() {
        for currency in Currency::all() {
            let def = currency.def();
            assert!(!def.symbol.is_empty());
            assert!(!def.code.is_empty());
            assert!(!def.aliases.is_empty());
        }
    }
}
