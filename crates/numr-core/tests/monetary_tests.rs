//! Monetary calculation tests
//! Tests for currency conversions, multi-currency arithmetic, and formatting

use numr_core::{Currency, Engine};

#[test]
fn test_currency_formats() {
    let mut engine = Engine::new();

    // USD formats
    assert_eq!(engine.eval("$100").to_string(), "$100.00");
    assert_eq!(engine.eval("100 USD").to_string(), "$100.00");
    assert_eq!(engine.eval("100$").to_string(), "$100.00");
    assert_eq!(engine.eval("100 dollars").to_string(), "$100.00");

    // EUR formats
    assert_eq!(engine.eval("€50").to_string(), "€50.00");
    assert_eq!(engine.eval("50 EUR").to_string(), "€50.00");
    assert_eq!(engine.eval("50 eur").to_string(), "€50.00");
    assert_eq!(engine.eval("50 euros").to_string(), "€50.00");

    // GBP formats
    assert_eq!(engine.eval("£75").to_string(), "£75.00");
    assert_eq!(engine.eval("75 GBP").to_string(), "£75.00");
    assert_eq!(engine.eval("75 pounds").to_string(), "£75.00");

    // RUB formats (symbol after number in Russian convention)
    assert_eq!(engine.eval("₽100").to_string(), "100.00₽");
    assert_eq!(engine.eval("100 RUB").to_string(), "100.00₽");
    assert_eq!(engine.eval("100 rubles").to_string(), "100.00₽");

    // BTC formats
    assert_eq!(engine.eval("₿1").to_string(), "₿1.00");
    assert_eq!(engine.eval("1 BTC").to_string(), "₿1.00");
    assert_eq!(engine.eval("1 bitcoin").to_string(), "₿1.00");
}

#[test]
fn test_currency_arithmetic_same() {
    let mut engine = Engine::new();

    // Addition of same currency
    let result = engine.eval("$100 + $50");
    assert_eq!(result.to_string(), "$150.00");

    // Subtraction of same currency
    let result = engine.eval("€200 - €75");
    assert_eq!(result.to_string(), "€125.00");

    // Multiplication with scalar
    let result = engine.eval("$25 * 4");
    assert_eq!(result.to_string(), "$100.00");

    // Division with scalar
    let result = engine.eval("£100 / 4");
    assert_eq!(result.to_string(), "£25.00");
}

#[test]
fn test_usd_to_eur_conversion() {
    let mut engine = Engine::new();
    engine.set_exchange_rate(Currency::USD, Currency::EUR, 0.92);

    let result = engine.eval("$100 in EUR");
    assert_eq!(result.to_string(), "€92.00");

    let result = engine.eval("$1000 in eur");
    assert_eq!(result.to_string(), "€920.00");
}

#[test]
fn test_eur_to_usd_conversion() {
    let mut engine = Engine::new();
    engine.set_exchange_rate(Currency::USD, Currency::EUR, 0.92);

    // Inverse rate should work automatically
    let result = engine.eval("€92 in USD");
    assert!(result.to_string().starts_with("$"));
    let amount = result.as_f64().unwrap();
    assert!((amount - 100.0).abs() < 0.1);
}

#[test]
fn test_usd_to_gbp_conversion() {
    let mut engine = Engine::new();
    engine.set_exchange_rate(Currency::USD, Currency::GBP, 0.79);

    let result = engine.eval("$100 in GBP");
    assert_eq!(result.to_string(), "£79.00");

    let result = engine.eval("$500 in pounds");
    assert_eq!(result.to_string(), "£395.00");
}

#[test]
fn test_usd_to_jpy_conversion() {
    let mut engine = Engine::new();
    engine.set_exchange_rate(Currency::USD, Currency::JPY, 149.5);

    let result = engine.eval("$100 in JPY");
    assert_eq!(result.to_string(), "¥14950.00");

    let result = engine.eval("$10 in jpy");
    assert_eq!(result.to_string(), "¥1495.00");
}

#[test]
fn test_multi_currency_arithmetic() {
    let mut engine = Engine::new();
    engine.set_exchange_rate(Currency::USD, Currency::EUR, 0.92);

    // Adding different currencies converts to left currency
    let result = engine.eval("$100 + €46");
    // €46 = $50, so $100 + $50 = $150
    assert_eq!(result.to_string(), "$150.00");
}

#[test]
fn test_currency_with_variables() {
    let mut engine = Engine::new();
    engine.set_exchange_rate(Currency::USD, Currency::EUR, 0.92);

    engine.eval("income = $5000");
    engine.eval("expenses = €1000");

    // Variable with currency
    let result = engine.eval("income");
    assert_eq!(result.to_string(), "$5000.00");

    // Convert variable
    let result = engine.eval("income in EUR");
    assert_eq!(result.to_string(), "€4600.00");
}

#[test]
fn test_currency_percentage_operations() {
    let mut engine = Engine::new();

    // 15% of $200
    let result = engine.eval("15% of $200");
    assert_eq!(result.to_string(), "$30.00");

    // $100 + 20%
    let result = engine.eval("$100 + 20%");
    assert_eq!(result.to_string(), "$120.00");

    // €500 - 10%
    let result = engine.eval("€500 - 10%");
    assert_eq!(result.to_string(), "€450.00");
}

#[test]
fn test_rub_and_ils_currencies() {
    let mut engine = Engine::new();
    engine.set_exchange_rate(Currency::USD, Currency::RUB, 92.0);
    engine.set_exchange_rate(Currency::USD, Currency::ILS, 3.65);

    // USD to RUB
    let result = engine.eval("$100 in RUB");
    assert_eq!(result.to_string(), "9200.00₽");

    // USD to ILS
    let result = engine.eval("$100 in ILS");
    assert_eq!(result.to_string(), "₪365.00");

    // RUB formatting (symbol after number)
    let result = engine.eval("₽5000");
    assert_eq!(result.to_string(), "5000.00₽");

    // ILS formatting
    let result = engine.eval("₪100");
    assert_eq!(result.to_string(), "₪100.00");
}

#[test]
fn test_travel_expense_scenario() {
    let mut engine = Engine::new();
    engine.set_exchange_rate(Currency::USD, Currency::EUR, 0.92);
    engine.set_exchange_rate(Currency::USD, Currency::GBP, 0.79);

    // Travel expenses in different currencies
    engine.eval("flight = $800");
    engine.eval("hotel_paris = €500");
    engine.eval("hotel_london = £300");

    // Individual amounts
    assert_eq!(engine.eval("flight").to_string(), "$800.00");
    assert_eq!(engine.eval("hotel_paris").to_string(), "€500.00");
    assert_eq!(engine.eval("hotel_london").to_string(), "£300.00");

    // Convert all to USD for total
    let paris_usd = engine.eval("hotel_paris in USD");
    assert!(paris_usd.as_f64().unwrap() > 500.0); // €500 > $500

    let london_usd = engine.eval("hotel_london in USD");
    assert!(london_usd.as_f64().unwrap() > 300.0); // £300 > $300
}

#[test]
fn test_all_currencies_have_default_rates() {
    let mut engine = Engine::new();

    // All supported currencies now have default fallback rates
    // PLN should convert successfully
    let result = engine.eval("$100 in PLN");
    assert!(result.as_f64().is_some());

    // Same-currency operations work
    assert_eq!(engine.eval("$100 + $50").to_string(), "$150.00");
    assert_eq!(engine.eval("€200 * 2").to_string(), "€400.00");

    // EUR conversion works with fallback rate
    let result = engine.eval("$100 in EUR");
    assert!(result.as_f64().is_some());

    // Crypto conversions also work with defaults
    let result = engine.eval("1 ETH in USD");
    assert!(result.as_f64().is_some());
}

#[test]
fn test_crypto_currency_formats() {
    let mut engine = Engine::new();

    // ETH formats
    assert_eq!(engine.eval("Ξ1").to_string(), "Ξ1.00");
    assert_eq!(engine.eval("1 ETH").to_string(), "Ξ1.00");
    assert_eq!(engine.eval("1 ethereum").to_string(), "Ξ1.00");

    // SOL formats
    assert_eq!(engine.eval("◎10").to_string(), "◎10.00");
    assert_eq!(engine.eval("10 SOL").to_string(), "◎10.00");
    assert_eq!(engine.eval("10 solana").to_string(), "◎10.00");

    // USDC/USDT (stablecoins)
    assert_eq!(engine.eval("100 USDC").to_string(), "USDC100.00");
    assert_eq!(engine.eval("₮100").to_string(), "₮100.00");
    assert_eq!(engine.eval("100 USDT").to_string(), "₮100.00");
}

#[test]
fn test_crypto_to_usd_conversion() {
    let mut engine = Engine::new();
    // Crypto rates: "1 TOKEN = X USD"
    engine.set_exchange_rate(Currency::ETH, Currency::USD, 3000.0);
    engine.set_exchange_rate(Currency::SOL, Currency::USD, 140.0);
    engine.set_exchange_rate(Currency::BTC, Currency::USD, 92000.0);

    // ETH to USD
    let result = engine.eval("1 ETH in USD");
    assert_eq!(result.to_string(), "$3000.00");

    // SOL to USD
    let result = engine.eval("10 SOL in USD");
    assert_eq!(result.to_string(), "$1400.00");

    // BTC to USD
    let result = engine.eval("0.5 BTC in USD");
    assert_eq!(result.to_string(), "$46000.00");
}

#[test]
fn test_usd_to_crypto_conversion() {
    let mut engine = Engine::new();
    engine.set_exchange_rate(Currency::ETH, Currency::USD, 3000.0);

    // USD to ETH (inverse rate)
    let result = engine.eval("$6000 in ETH");
    assert_eq!(result.to_string(), "Ξ2.00");
}
