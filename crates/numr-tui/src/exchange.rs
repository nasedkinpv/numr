use anyhow::Result;
use numr_core::types::currency::Currency;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize)]
struct FiatRatesResponse {
    rates: HashMap<String, f64>,
}

/// CoinGecko returns: { "bitcoin": { "usd": 92000 }, "ethereum": { "usd": 3000 }, ... }
type CryptoPricesResponse = HashMap<String, CryptoPrice>;

#[derive(Deserialize)]
struct CryptoPrice {
    usd: f64,
}

/// Fetch exchange rates from multiple sources.
/// Returns rates as HashMap where key is currency code (e.g., "EUR", "BTC").
/// - Fiat rates: "1 USD = X units" (e.g., EUR -> 0.92)
/// - Crypto rates: "1 TOKEN = X USD" (e.g., BTC -> 92000, ETH -> 3000)
pub async fn fetch_rates() -> Result<HashMap<String, f64>> {
    let mut rates = fetch_fiat_rates().await?;

    if let Some(crypto_rates) = fetch_crypto_prices().await {
        rates.extend(crypto_rates);
    }

    Ok(rates)
}

async fn fetch_fiat_rates() -> Result<HashMap<String, f64>> {
    let url = "https://open.er-api.com/v6/latest/USD";
    let response = reqwest::get(url).await?;
    let data: FiatRatesResponse = response.json().await?;
    Ok(data.rates)
}

async fn fetch_crypto_prices() -> Option<HashMap<String, f64>> {
    // Get crypto IDs from the currency registry (single source of truth)
    let crypto_currencies: Vec<_> = Currency::all()
        .filter(|c| c.is_crypto())
        .filter_map(|c| c.coingecko_id().map(|id| (id, c.code())))
        .collect();

    if crypto_currencies.is_empty() {
        return Some(HashMap::new());
    }

    let ids: Vec<&str> = crypto_currencies.iter().map(|(id, _)| *id).collect();
    let url = format!(
        "https://api.coingecko.com/api/v3/simple/price?ids={}&vs_currencies=usd",
        ids.join(",")
    );

    let response = reqwest::get(&url).await.ok()?;
    let data: CryptoPricesResponse = response.json().await.ok()?;

    let mut rates = HashMap::new();
    for (coingecko_id, code) in &crypto_currencies {
        if let Some(price) = data.get(*coingecko_id) {
            rates.insert(code.to_string(), price.usd);
        }
    }

    Some(rates)
}
