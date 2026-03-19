//! Exchange rate fetching (requires "fetch" feature)
//!
//! This module provides async functions to fetch exchange rates from external APIs.
//! It's gated behind the "fetch" feature to keep numr-core WASM-compatible by default.

use crate::types::Currency;
use rust_decimal::Decimal;
use serde::Deserialize;
use std::collections::HashMap;
use std::time::Duration;

pub const DEFAULT_FIAT_RATES_URL: &str = "https://open.er-api.com/v6/latest/USD";
pub const DEFAULT_CRYPTO_RATES_URL: &str = "https://api.coingecko.com/api/v3/simple/price";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchConfig {
    pub fiat_rates_url: String,
    pub crypto_rates_url: String,
    pub coingecko_api_key: Option<String>,
}

impl Default for FetchConfig {
    fn default() -> Self {
        Self {
            fiat_rates_url: DEFAULT_FIAT_RATES_URL.to_string(),
            crypto_rates_url: DEFAULT_CRYPTO_RATES_URL.to_string(),
            coingecko_api_key: None,
        }
    }
}

#[derive(Deserialize)]
struct FiatRatesResponse {
    rates: HashMap<String, Decimal>,
}

/// CoinGecko returns: { "bitcoin": { "usd": 92000 }, "ethereum": { "usd": 3000 }, ... }
type CryptoPricesResponse = HashMap<String, CryptoPrice>;

#[derive(Deserialize)]
struct CryptoPrice {
    #[serde(default)]
    usd: Option<Decimal>,
}

/// Result of a rate fetch, including any partial-failure warnings
pub struct FetchResult {
    pub rates: HashMap<String, Decimal>,
    /// Warning message if some rates (e.g., crypto) failed to fetch
    pub warning: Option<String>,
}

/// Fetch exchange rates from multiple sources.
/// Returns rates as HashMap where key is currency code (e.g., "EUR", "BTC").
/// - Fiat rates: "1 USD = X units" (e.g., EUR -> 0.92)
/// - Crypto rates: "1 TOKEN = X USD" (e.g., BTC -> 92000, ETH -> 3000)
pub async fn fetch_rates() -> Result<FetchResult, String> {
    fetch_rates_with_config(&FetchConfig::default()).await
}

/// Fetch exchange rates using custom API endpoints and optional credentials.
pub async fn fetch_rates_with_config(config: &FetchConfig) -> Result<FetchResult, String> {
    let client = reqwest::Client::builder()
        .user_agent("numr")
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))?;
    let mut rates = fetch_fiat_rates(&client, &config.fiat_rates_url).await?;

    let warning = match fetch_crypto_prices(&client, config).await {
        Ok(crypto_rates) => {
            rates.extend(crypto_rates);
            None
        }
        Err(e) => Some(format!("crypto rates unavailable: {e}")),
    };

    Ok(FetchResult { rates, warning })
}

async fn fetch_fiat_rates(
    client: &reqwest::Client,
    url: &str,
) -> Result<HashMap<String, Decimal>, String> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch fiat rates: {e}"))?
        .error_for_status()
        .map_err(|e| format!("Fiat rates API error: {e}"))?;
    let data: FiatRatesResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse fiat rates: {e}"))?;
    Ok(data.rates)
}

async fn fetch_crypto_prices(
    client: &reqwest::Client,
    config: &FetchConfig,
) -> Result<HashMap<String, Decimal>, String> {
    // Get crypto IDs from the currency registry (single source of truth)
    let crypto_currencies: Vec<_> = Currency::all()
        .filter(|c| c.is_crypto())
        .filter_map(|c| c.coingecko_id().map(|id| (id, c.code())))
        .collect();

    if crypto_currencies.is_empty() {
        return Ok(HashMap::new());
    }

    let ids: Vec<&str> = crypto_currencies.iter().map(|(id, _)| *id).collect();
    let response = build_crypto_prices_request(client, config, &ids)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch crypto prices: {e}"))?
        .error_for_status()
        .map_err(|e| format!("Crypto prices API error: {e}"))?;
    let text = response
        .text()
        .await
        .map_err(|e| format!("Failed to read crypto response: {e}"))?;
    let data: CryptoPricesResponse =
        serde_json::from_str(&text).map_err(|e| format!("Failed to parse crypto prices: {e}"))?;

    let mut rates = HashMap::new();
    for (coingecko_id, code) in &crypto_currencies {
        if let Some(price) = data.get(*coingecko_id) {
            if let Some(usd) = price.usd {
                rates.insert(code.to_string(), usd);
            }
        }
    }

    Ok(rates)
}

fn build_crypto_prices_request(
    client: &reqwest::Client,
    config: &FetchConfig,
    ids: &[&str],
) -> reqwest::RequestBuilder {
    let mut request = client
        .get(&config.crypto_rates_url)
        .query(&[("ids", ids.join(",")), ("vs_currencies", "usd".to_string())]);

    if let Some(api_key) = config.coingecko_api_key.as_deref() {
        if let Some(header_name) = coingecko_api_key_header(&config.crypto_rates_url) {
            request = request.header(header_name, api_key);
        }
    }

    request
}

fn coingecko_api_key_header(url: &str) -> Option<&'static str> {
    let url = reqwest::Url::parse(url).ok()?;

    match url.host_str() {
        Some("api.coingecko.com") => Some("x-cg-demo-api-key"),
        Some("pro-api.coingecko.com") => Some("x-cg-pro-api-key"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use std::collections::HashMap;
    use std::str::FromStr;

    fn request_query(request: &reqwest::Request) -> HashMap<String, String> {
        request
            .url()
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect()
    }

    #[test]
    fn test_fetch_config_defaults_match_public_constants() {
        let config = FetchConfig::default();
        assert_eq!(config.fiat_rates_url, DEFAULT_FIAT_RATES_URL);
        assert_eq!(config.crypto_rates_url, DEFAULT_CRYPTO_RATES_URL);
        assert!(config.coingecko_api_key.is_none());
    }

    #[test]
    fn test_fiat_rates_response_parsing() {
        let json = r#"{"rates":{"EUR":0.92,"GBP":0.79,"JPY":149.5,"RUB":92}}"#;
        let response: FiatRatesResponse = serde_json::from_str(json).unwrap();

        assert_eq!(
            response.rates.get("EUR"),
            Some(&Decimal::from_str("0.92").unwrap())
        );
        assert_eq!(
            response.rates.get("GBP"),
            Some(&Decimal::from_str("0.79").unwrap())
        );
        assert_eq!(
            response.rates.get("JPY"),
            Some(&Decimal::from_str("149.5").unwrap())
        );
        assert_eq!(response.rates.get("RUB"), Some(&Decimal::from(92)));
    }

    #[test]
    fn test_crypto_prices_response_parsing() {
        let json = r#"{"bitcoin":{"usd":95000},"ethereum":{"usd":3000},"solana":{"usd":140}}"#;
        let response: CryptoPricesResponse = serde_json::from_str(json).unwrap();

        assert_eq!(
            response.get("bitcoin").unwrap().usd,
            Some(Decimal::from(95000))
        );
        assert_eq!(
            response.get("ethereum").unwrap().usd,
            Some(Decimal::from(3000))
        );
        assert_eq!(
            response.get("solana").unwrap().usd,
            Some(Decimal::from(140))
        );
    }

    #[test]
    fn test_crypto_prices_missing_usd() {
        // CoinGecko may return empty objects for some tokens
        let json = r#"{"bitcoin":{"usd":95000},"unknown_token":{}}"#;
        let response: CryptoPricesResponse = serde_json::from_str(json).unwrap();

        assert_eq!(
            response.get("bitcoin").unwrap().usd,
            Some(Decimal::from(95000))
        );
        assert_eq!(response.get("unknown_token").unwrap().usd, None);
    }

    #[test]
    fn test_crypto_currencies_have_coingecko_ids() {
        // Verify that crypto currencies have CoinGecko IDs for fetching
        let crypto_currencies: Vec<_> = Currency::all()
            .filter(|c| c.is_crypto())
            .filter_map(|c| c.coingecko_id().map(|id| (id, c.code())))
            .collect();

        // Should have at least BTC, ETH, SOL
        assert!(
            crypto_currencies.len() >= 3,
            "Expected at least 3 crypto currencies with CoinGecko IDs, got {}",
            crypto_currencies.len()
        );

        let ids: Vec<&str> = crypto_currencies.iter().map(|(id, _)| *id).collect();
        assert!(ids.contains(&"bitcoin"));
        assert!(ids.contains(&"ethereum"));
        assert!(ids.contains(&"solana"));
    }

    #[test]
    fn test_empty_fiat_rates_response() {
        let json = r#"{"rates":{}}"#;
        let response: FiatRatesResponse = serde_json::from_str(json).unwrap();
        assert!(response.rates.is_empty());
    }

    #[test]
    fn test_empty_crypto_prices_response() {
        let json = r#"{}"#;
        let response: CryptoPricesResponse = serde_json::from_str(json).unwrap();
        assert!(response.is_empty());
    }

    #[test]
    fn test_coingecko_api_key_header_detection() {
        assert_eq!(
            coingecko_api_key_header("https://api.coingecko.com/api/v3/simple/price"),
            Some("x-cg-demo-api-key")
        );
        assert_eq!(
            coingecko_api_key_header("https://pro-api.coingecko.com/api/v3/simple/price"),
            Some("x-cg-pro-api-key")
        );
        assert_eq!(
            coingecko_api_key_header("https://example.com/api/v3/simple/price"),
            None
        );
    }

    #[test]
    fn test_build_crypto_prices_request_adds_query_params() {
        let client = reqwest::Client::new();
        let request = build_crypto_prices_request(&client, &FetchConfig::default(), &["bitcoin"])
            .build()
            .unwrap();

        let query = request_query(&request);
        assert_eq!(query.get("ids").map(String::as_str), Some("bitcoin"));
        assert_eq!(query.get("vs_currencies").map(String::as_str), Some("usd"));
    }

    #[test]
    fn test_build_crypto_prices_request_uses_demo_api_key_header() {
        let client = reqwest::Client::new();
        let config = FetchConfig {
            coingecko_api_key: Some("demo-key".to_string()),
            ..FetchConfig::default()
        };
        let request = build_crypto_prices_request(&client, &config, &["bitcoin"])
            .build()
            .unwrap();

        assert_eq!(
            request
                .headers()
                .get("x-cg-demo-api-key")
                .and_then(|value| value.to_str().ok()),
            Some("demo-key")
        );
    }

    #[test]
    fn test_build_crypto_prices_request_uses_pro_api_key_header() {
        let client = reqwest::Client::new();
        let config = FetchConfig {
            crypto_rates_url: "https://pro-api.coingecko.com/api/v3/simple/price".to_string(),
            coingecko_api_key: Some("pro-key".to_string()),
            ..FetchConfig::default()
        };
        let request = build_crypto_prices_request(&client, &config, &["bitcoin"])
            .build()
            .unwrap();

        assert_eq!(
            request
                .headers()
                .get("x-cg-pro-api-key")
                .and_then(|value| value.to_str().ok()),
            Some("pro-key")
        );
    }

    #[test]
    fn test_build_crypto_prices_request_skips_api_key_for_non_coingecko_url() {
        let client = reqwest::Client::new();
        let config = FetchConfig {
            crypto_rates_url: "https://example.com/simple/price".to_string(),
            coingecko_api_key: Some("proxy-key".to_string()),
            ..FetchConfig::default()
        };
        let request = build_crypto_prices_request(&client, &config, &["bitcoin"])
            .build()
            .unwrap();

        assert!(request.headers().get("x-cg-demo-api-key").is_none());
        assert!(request.headers().get("x-cg-pro-api-key").is_none());
    }
}
