use anyhow::Result;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize)]
struct FiatRatesResponse {
    rates: HashMap<String, f64>,
}

#[derive(Deserialize)]
struct CryptoPricesResponse {
    bitcoin: Option<BitcoinPrice>,
}

#[derive(Deserialize)]
struct BitcoinPrice {
    usd: f64,
}

/// Fetch exchange rates from multiple sources.
/// Returns rates as HashMap where key is currency code (e.g., "EUR", "BTC").
/// - Fiat rates: "1 USD = X units" (e.g., EUR -> 0.92)
/// - BTC rate: "1 BTC = X USD" (e.g., BTC -> 97000)
pub async fn fetch_rates() -> Result<HashMap<String, f64>> {
    let mut rates = fetch_fiat_rates().await?;

    if let Some(btc_price) = fetch_bitcoin_price().await {
        rates.insert("BTC".to_string(), btc_price);
    }

    Ok(rates)
}

async fn fetch_fiat_rates() -> Result<HashMap<String, f64>> {
    let url = "https://open.er-api.com/v6/latest/USD";
    let response = reqwest::get(url).await?;
    let data: FiatRatesResponse = response.json().await?;
    Ok(data.rates)
}

async fn fetch_bitcoin_price() -> Option<f64> {
    let url = "https://api.coingecko.com/api/v3/simple/price?ids=bitcoin&vs_currencies=usd";
    let response = reqwest::get(url).await.ok()?;
    let data: CryptoPricesResponse = response.json().await.ok()?;
    Some(data.bitcoin?.usd)
}
