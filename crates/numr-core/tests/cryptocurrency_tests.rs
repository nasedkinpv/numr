//! Cryptocurrency calculation tests
//! Tests for Bitcoin and crypto-to-fiat conversions

use numr_core::{Currency, Engine};

#[test]
fn test_btc_formats() {
    let mut engine = Engine::new();

    // Different BTC input formats
    assert_eq!(engine.eval("₿1").to_string(), "₿1.00");
    assert_eq!(engine.eval("1 BTC").to_string(), "₿1.00");
    assert_eq!(engine.eval("1 btc").to_string(), "₿1.00");
    assert_eq!(engine.eval("1 bitcoin").to_string(), "₿1.00");
}

#[test]
fn test_btc_to_usd_conversion() {
    let mut engine = Engine::new();
    // Default rate is 60000 from the registry

    let result = engine.eval("₿1 in USD");
    assert_eq!(result.to_string(), "$60000.00");

    let result = engine.eval("1 BTC in usd");
    assert_eq!(result.to_string(), "$60000.00");

    let result = engine.eval("0.5 btc in USD");
    assert_eq!(result.to_string(), "$30000.00");
}

#[test]
fn test_btc_fractional_amounts() {
    let mut engine = Engine::new();
    // BTC price = $60000

    // Small fractions (satoshi-level thinking)
    let result = engine.eval("0.001 BTC in USD");
    assert_eq!(result.to_string(), "$60.00");

    let result = engine.eval("0.0001 BTC in USD");
    assert_eq!(result.to_string(), "$6.00");

    let result = engine.eval("0.01 btc in usd");
    assert_eq!(result.to_string(), "$600.00");
}

#[test]
fn test_usd_to_btc_conversion() {
    let mut engine = Engine::new();
    // Default BTC rate = $60000

    let result = engine.eval("$60000 in BTC");
    assert_eq!(result.to_string(), "₿1.00");

    let result = engine.eval("$6000 in btc");
    assert_eq!(result.to_string(), "₿0.10");

    let result = engine.eval("$600 in bitcoin");
    assert_eq!(result.to_string(), "₿0.01");
}

#[test]
fn test_btc_arithmetic() {
    let mut engine = Engine::new();

    // BTC addition
    let result = engine.eval("₿0.5 + ₿0.25");
    assert_eq!(result.to_string(), "₿0.75");

    // BTC subtraction
    let result = engine.eval("₿1 - ₿0.3");
    assert_eq!(result.to_string(), "₿0.70");

    // BTC multiplication
    let result = engine.eval("₿0.1 * 5");
    assert_eq!(result.to_string(), "₿0.50");

    // BTC division
    let result = engine.eval("₿1 / 4");
    assert_eq!(result.to_string(), "₿0.25");
}

#[test]
fn test_btc_with_custom_rate() {
    let mut engine = Engine::new();
    engine.set_exchange_rate(Currency::BTC, Currency::USD, 100000.0);

    let result = engine.eval("₿1 in USD");
    assert_eq!(result.to_string(), "$100000.00");

    let result = engine.eval("$50000 in BTC");
    assert_eq!(result.to_string(), "₿0.50");
}

#[test]
fn test_btc_to_other_currencies() {
    let mut engine = Engine::new();
    // BTC = $60000 (default)
    engine.set_exchange_rate(Currency::USD, Currency::EUR, 0.92);

    // BTC to EUR (via USD)
    // 1 BTC = $60000 = €55200
    let result = engine.eval("₿1 in EUR");
    assert_eq!(result.to_string(), "€55200.00");

    // Smaller amount
    let result = engine.eval("0.1 BTC in EUR");
    assert_eq!(result.to_string(), "€5520.00");
}

#[test]
fn test_crypto_portfolio_tracking() {
    let mut engine = Engine::new();
    // Default BTC = $60000

    // Portfolio holdings
    engine.eval("btc_holdings = ₿0.5");
    engine.eval("usd_cash = $10000");

    // Check BTC value
    let result = engine.eval("btc_holdings in USD");
    assert_eq!(result.to_string(), "$30000.00");

    // Total portfolio value would need manual addition
    // btc_holdings in USD = $30000
    // usd_cash = $10000
    // Total = $40000
}

#[test]
fn test_dca_scenario() {
    let mut engine = Engine::new();
    // Dollar Cost Averaging scenario

    // Weekly investment
    engine.eval("weekly_investment = $100");

    // At current rate ($60000/BTC)
    let result = engine.eval("weekly_investment in BTC");
    let btc_amount = result.as_f64().unwrap();
    assert!((btc_amount - 0.001666).abs() < 0.001);

    // Monthly (4 weeks)
    let result = engine.eval("$400 in BTC");
    let monthly_btc = result.as_f64().unwrap();
    assert!((monthly_btc - 0.006666).abs() < 0.001);
}

#[test]
fn test_btc_percentage_operations() {
    let mut engine = Engine::new();

    // 10% of 1 BTC
    let result = engine.eval("10% of ₿1");
    assert_eq!(result.to_string(), "₿0.10");

    // BTC with percentage increase
    let result = engine.eval("₿1 + 50%");
    assert_eq!(result.to_string(), "₿1.50");

    // BTC with percentage decrease
    let result = engine.eval("₿2 - 25%");
    assert_eq!(result.to_string(), "₿1.50");
}

#[test]
fn test_btc_variables() {
    let mut engine = Engine::new();

    engine.eval("my_btc = ₿0.25");
    engine.eval("btc_price = 60000");

    let result = engine.eval("my_btc");
    assert_eq!(result.to_string(), "₿0.25");

    // Convert to USD
    let result = engine.eval("my_btc in USD");
    assert_eq!(result.to_string(), "$15000.00");
}

#[test]
fn test_profit_loss_scenario() {
    let mut engine = Engine::new();

    // Bought 0.1 BTC at $50000
    engine.eval("purchase_price = 50000");
    engine.eval("btc_amount = 0.1");
    engine.eval("cost_basis = 5000"); // 0.1 * 50000

    // Current price $60000 (default rate)
    // Current value = 0.1 * 60000 = $6000
    let result = engine.eval("0.1 BTC in USD");
    assert_eq!(result.to_string(), "$6000.00");

    // Profit = $6000 - $5000 = $1000 (20% gain)
}

#[test]
fn test_mixed_crypto_fiat() {
    let mut engine = Engine::new();

    // Scenario: Have some BTC and some USD
    engine.eval("crypto = ₿0.1");
    engine.eval("fiat = $5000");

    // Value of crypto in USD
    let crypto_value = engine.eval("crypto in USD");
    assert_eq!(crypto_value.to_string(), "$6000.00");

    // Can calculate total: $6000 + $5000 = $11000
    let result = engine.eval("$6000 + fiat");
    assert_eq!(result.to_string(), "$11000.00");
}
