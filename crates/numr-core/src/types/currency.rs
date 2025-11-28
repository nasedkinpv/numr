//! Currency definitions and handling
//!
//! # Adding a new currency
//!
//! 1. Add enum variant to `Currency`
//! 2. Add entry to `CURRENCIES` array with all metadata
//! 3. If it has a single-char Unicode symbol, add to `grammar.pest` currency_symbol rule
//!
//! That's it! Parsing, display, highlighting, and exchange rate fetching
//! will automatically pick up the new currency from the registry.

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
    /// Whether this is a cryptocurrency (affects exchange rate handling)
    pub is_crypto: bool,
    /// CoinGecko API ID for fetching prices (crypto only)
    pub coingecko_id: Option<&'static str>,
}

/// Complete registry of all supported currencies.
/// To add a new currency: add enum variant and add entry here.
pub static CURRENCIES: &[CurrencyDef] = &[
    // === Fiat Currencies ===
    CurrencyDef {
        currency: Currency::USD,
        symbol: "$",
        code: "USD",
        aliases: &["$", "usd", "dollars"],
        symbol_after: false,
        is_crypto: false,
        coingecko_id: None,
    },
    CurrencyDef {
        currency: Currency::EUR,
        symbol: "€",
        code: "EUR",
        aliases: &["€", "eur", "euros"],
        symbol_after: false,
        is_crypto: false,
        coingecko_id: None,
    },
    CurrencyDef {
        currency: Currency::GBP,
        symbol: "£",
        code: "GBP",
        aliases: &["£", "gbp", "pounds"],
        symbol_after: false,
        is_crypto: false,
        coingecko_id: None,
    },
    CurrencyDef {
        currency: Currency::JPY,
        symbol: "¥",
        code: "JPY",
        aliases: &["¥", "jpy", "yen"],
        symbol_after: false,
        is_crypto: false,
        coingecko_id: None,
    },
    CurrencyDef {
        currency: Currency::CHF,
        symbol: "CHF",
        code: "CHF",
        aliases: &["chf", "francs"],
        symbol_after: false,
        is_crypto: false,
        coingecko_id: None,
    },
    CurrencyDef {
        currency: Currency::CNY,
        symbol: "¥",
        code: "CNY",
        aliases: &["cny", "rmb", "yuan"],
        symbol_after: false,
        is_crypto: false,
        coingecko_id: None,
    },
    CurrencyDef {
        currency: Currency::CAD,
        symbol: "C$",
        code: "CAD",
        aliases: &["cad"],
        symbol_after: false,
        is_crypto: false,
        coingecko_id: None,
    },
    CurrencyDef {
        currency: Currency::AUD,
        symbol: "A$",
        code: "AUD",
        aliases: &["aud"],
        symbol_after: false,
        is_crypto: false,
        coingecko_id: None,
    },
    CurrencyDef {
        currency: Currency::INR,
        symbol: "₹",
        code: "INR",
        aliases: &["₹", "inr", "rupees"],
        symbol_after: false,
        is_crypto: false,
        coingecko_id: None,
    },
    CurrencyDef {
        currency: Currency::KRW,
        symbol: "₩",
        code: "KRW",
        aliases: &["₩", "krw", "won"],
        symbol_after: false,
        is_crypto: false,
        coingecko_id: None,
    },
    CurrencyDef {
        currency: Currency::RUB,
        symbol: "₽",
        code: "RUB",
        aliases: &["₽", "rub", "rubles"],
        symbol_after: true,
        is_crypto: false,
        coingecko_id: None,
    },
    CurrencyDef {
        currency: Currency::ILS,
        symbol: "₪",
        code: "ILS",
        aliases: &["₪", "ils", "shekels"],
        symbol_after: false,
        is_crypto: false,
        coingecko_id: None,
    },
    CurrencyDef {
        currency: Currency::PLN,
        symbol: "zł",
        code: "PLN",
        aliases: &["zł", "pln", "zloty"],
        symbol_after: true,
        is_crypto: false,
        coingecko_id: None,
    },
    CurrencyDef {
        currency: Currency::UAH,
        symbol: "₴",
        code: "UAH",
        aliases: &["₴", "uah", "hryvnia"],
        symbol_after: false,
        is_crypto: false,
        coingecko_id: None,
    },
    // === Cryptocurrencies ===
    CurrencyDef {
        currency: Currency::BTC,
        symbol: "₿",
        code: "BTC",
        aliases: &["₿", "btc", "bitcoin"],
        symbol_after: false,
        is_crypto: true,
        coingecko_id: Some("bitcoin"),
    },
    CurrencyDef {
        currency: Currency::ETH,
        symbol: "Ξ",
        code: "ETH",
        aliases: &["Ξ", "eth", "ethereum", "ether"],
        symbol_after: false,
        is_crypto: true,
        coingecko_id: Some("ethereum"),
    },
    CurrencyDef {
        currency: Currency::SOL,
        symbol: "◎",
        code: "SOL",
        aliases: &["◎", "sol", "solana"],
        symbol_after: false,
        is_crypto: true,
        coingecko_id: Some("solana"),
    },
    CurrencyDef {
        currency: Currency::USDT,
        symbol: "₮",
        code: "USDT",
        aliases: &["₮", "usdt", "tether"],
        symbol_after: false,
        is_crypto: true,
        coingecko_id: Some("tether"),
    },
    CurrencyDef {
        currency: Currency::USDC,
        symbol: "USDC",
        code: "USDC",
        aliases: &["usdc"],
        symbol_after: false,
        is_crypto: true,
        coingecko_id: Some("usd-coin"),
    },
    CurrencyDef {
        currency: Currency::BNB,
        symbol: "BNB",
        code: "BNB",
        aliases: &["bnb", "binance"],
        symbol_after: false,
        is_crypto: true,
        coingecko_id: Some("binancecoin"),
    },
    CurrencyDef {
        currency: Currency::XRP,
        symbol: "XRP",
        code: "XRP",
        aliases: &["xrp", "ripple"],
        symbol_after: false,
        is_crypto: true,
        coingecko_id: Some("ripple"),
    },
    CurrencyDef {
        currency: Currency::ADA,
        symbol: "₳",
        code: "ADA",
        aliases: &["₳", "ada", "cardano"],
        symbol_after: false,
        is_crypto: true,
        coingecko_id: Some("cardano"),
    },
    CurrencyDef {
        currency: Currency::DOGE,
        symbol: "Ð",
        code: "DOGE",
        aliases: &["Ð", "doge", "dogecoin"],
        symbol_after: false,
        is_crypto: true,
        coingecko_id: Some("dogecoin"),
    },
    CurrencyDef {
        currency: Currency::DOT,
        symbol: "DOT",
        code: "DOT",
        aliases: &["dot", "polkadot"],
        symbol_after: false,
        is_crypto: true,
        coingecko_id: Some("polkadot"),
    },
    CurrencyDef {
        currency: Currency::LTC,
        symbol: "Ł",
        code: "LTC",
        aliases: &["Ł", "ltc", "litecoin"],
        symbol_after: false,
        is_crypto: true,
        coingecko_id: Some("litecoin"),
    },
    CurrencyDef {
        currency: Currency::LINK,
        symbol: "LINK",
        code: "LINK",
        aliases: &["link", "chainlink"],
        symbol_after: false,
        is_crypto: true,
        coingecko_id: Some("chainlink"),
    },
    CurrencyDef {
        currency: Currency::AVAX,
        symbol: "AVAX",
        code: "AVAX",
        aliases: &["avax", "avalanche"],
        symbol_after: false,
        is_crypto: true,
        coingecko_id: Some("avalanche-2"),
    },
    CurrencyDef {
        currency: Currency::MATIC,
        symbol: "MATIC",
        code: "MATIC",
        aliases: &["matic", "polygon"],
        symbol_after: false,
        is_crypto: true,
        coingecko_id: Some("polygon-ecosystem-token"),
    },
    CurrencyDef {
        currency: Currency::TON,
        symbol: "TON",
        code: "TON",
        aliases: &["ton", "toncoin"],
        symbol_after: false,
        is_crypto: true,
        coingecko_id: Some("the-open-network"),
    },
];

/// Supported currencies
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Currency {
    // Fiat
    USD,
    EUR,
    GBP,
    JPY,
    CHF,
    CNY,
    CAD,
    AUD,
    INR,
    KRW,
    RUB,
    ILS,
    PLN,
    UAH,
    // Crypto
    BTC,
    ETH,
    SOL,
    USDT,
    USDC,
    BNB,
    XRP,
    ADA,
    DOGE,
    DOT,
    LTC,
    LINK,
    AVAX,
    MATIC,
    TON,
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

    /// Check if this is a cryptocurrency (vs fiat)
    pub fn is_crypto(&self) -> bool {
        self.def().is_crypto
    }

    /// Get CoinGecko API ID (for crypto price fetching)
    pub fn coingecko_id(&self) -> Option<&'static str> {
        self.def().coingecko_id
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
        Currency::parse(s).ok_or_else(|| format!("Unknown currency: {s}"))
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
