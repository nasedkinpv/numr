//! Cryptocurrency calculation tests
//! Tests for Bitcoin and crypto-to-fiat conversions

use numr_core::{decimal as d, Currency, Decimal, Engine};

/// Helper to create an engine with a known BTC rate for consistent tests
fn engine_with_btc_rate(rate: Decimal) -> Engine {
    let mut engine = Engine::new();
    engine.set_exchange_rate(Currency::BTC, Currency::USD, rate);
    engine
}

#[test]
fn test_btc_formats() {
    let mut engine = Engine::new();

    for expression in ["₿1", "1 BTC", "1 btc", "1 bitcoin"] {
        assert_eq!(engine.eval(expression).to_string(), "₿1.00", "{expression}");
    }
}

#[test]
fn test_btc_to_usd_conversion() {
    let mut engine = engine_with_btc_rate(d("95000"));
    let cases = [
        ("₿1 in USD", "95000", "$95000.00"),
        ("0.5 btc in USD", "47500", "$47500.00"),
        ("0.001 BTC in USD", "95", "$95.00"),
        ("0.0001 BTC in USD", "9.5", "$9.50"),
    ];

    for (expression, amount, display) in cases {
        let result = engine.eval(expression);
        assert_eq!(result.as_decimal(), Some(d(amount)), "{expression}");
        assert_eq!(result.to_string(), display, "{expression}");
    }
}

#[test]
fn test_usd_to_btc_conversion() {
    let mut engine = engine_with_btc_rate(d("95000"));

    // Note: Division can introduce tiny precision differences
    let result = engine.eval("$95000 in BTC");
    let btc = result.as_decimal().unwrap();
    assert!(
        (btc - d("1")).abs() < d("0.0000001"),
        "Expected ~1 BTC, got {btc}"
    );
    assert_eq!(result.to_string(), "₿1.00");

    let result = engine.eval("$9500 in btc");
    let btc = result.as_decimal().unwrap();
    assert!(
        (btc - d("0.1")).abs() < d("0.0000001"),
        "Expected ~0.1 BTC, got {btc}"
    );
    assert_eq!(result.to_string(), "₿0.10");

    let result = engine.eval("$950 in bitcoin");
    let btc = result.as_decimal().unwrap();
    assert!(
        (btc - d("0.01")).abs() < d("0.0000001"),
        "Expected ~0.01 BTC, got {btc}"
    );
    assert_eq!(result.to_string(), "₿0.01");
}

#[test]
fn test_small_usd_to_btc_display_precision() {
    let mut engine = engine_with_btc_rate(d("95000"));

    let result = engine.eval("$400 in BTC");
    let btc = result.as_decimal().unwrap();
    let expected = d("400") / d("95000");
    assert!(
        (btc - expected).abs() < d("0.00000001"),
        "Expected ~{expected}, got {btc}"
    );
    assert_eq!(result.to_string(), "₿0.00421053");
}

#[test]
fn test_btc_arithmetic() {
    let mut engine = Engine::new();
    let cases = [
        ("₿0.5 + ₿0.25", "0.75", "₿0.75"),
        ("₿1 - ₿0.3", "0.7", "₿0.70"),
        ("₿0.1 * 5", "0.5", "₿0.50"),
        ("₿1 / 4", "0.25", "₿0.25"),
    ];

    for (expression, amount, display) in cases {
        let result = engine.eval(expression);
        assert_eq!(result.as_decimal(), Some(d(amount)), "{expression}");
        assert_eq!(result.to_string(), display, "{expression}");
    }
}

#[test]
fn test_btc_with_custom_rate() {
    let mut engine = Engine::new();
    engine.set_exchange_rate(Currency::BTC, Currency::USD, d("100000"));

    let result = engine.eval("₿1 in USD");
    assert_eq!(result.as_decimal(), Some(d("100000")));
    assert_eq!(result.to_string(), "$100000.00");

    let result = engine.eval("$50000 in BTC");
    assert_eq!(result.as_decimal(), Some(d("0.5")));
    assert_eq!(result.to_string(), "₿0.50");
}

#[test]
fn test_btc_to_other_currencies() {
    let mut engine = engine_with_btc_rate(d("95000"));
    engine.set_exchange_rate(Currency::USD, Currency::EUR, d("0.92"));

    // BTC to EUR (via USD)
    // 1 BTC = $95000 = €87400
    let result = engine.eval("₿1 in EUR");
    assert_eq!(result.as_decimal(), Some(d("87400")));
    assert_eq!(result.to_string(), "€87400.00");

    // Smaller amount
    let result = engine.eval("0.1 BTC in EUR");
    assert_eq!(result.as_decimal(), Some(d("8740")));
    assert_eq!(result.to_string(), "€8740.00");
}

#[test]
fn test_btc_percentage_operations() {
    let mut engine = Engine::new();

    // 10% of 1 BTC
    let result = engine.eval("10% of ₿1");
    assert_eq!(result.as_decimal(), Some(d("0.1")));
    assert_eq!(result.to_string(), "₿0.10");

    // BTC with percentage increase
    let result = engine.eval("₿1 + 50%");
    assert_eq!(result.as_decimal(), Some(d("1.5")));
    assert_eq!(result.to_string(), "₿1.50");

    // BTC with percentage decrease
    let result = engine.eval("₿2 - 25%");
    assert_eq!(result.as_decimal(), Some(d("1.5")));
    assert_eq!(result.to_string(), "₿1.50");
}

#[test]
fn test_btc_variables() {
    let mut engine = engine_with_btc_rate(d("95000"));

    engine.eval("my_btc = ₿0.25");
    engine.eval("btc_price = 95000");

    let result = engine.eval("my_btc");
    assert_eq!(result.as_decimal(), Some(d("0.25")));
    assert_eq!(result.to_string(), "₿0.25");

    // Convert to USD (0.25 * $95000 = $23750)
    let result = engine.eval("my_btc in USD");
    assert_eq!(result.as_decimal(), Some(d("23750")));
    assert_eq!(result.to_string(), "$23750.00");
}
