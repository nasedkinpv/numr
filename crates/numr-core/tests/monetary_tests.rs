//! Monetary calculation tests
//! Tests for currency conversions, multi-currency arithmetic, and formatting

use numr_core::{catalog::currency_catalog, decimal as d, Currency, Engine};

#[test]
fn test_currency_formats() {
    let mut engine = Engine::new();
    let cases = [
        ("$100", "$100.00"),
        ("100 USD", "$100.00"),
        ("100$", "$100.00"),
        ("100 dollars", "$100.00"),
        ("€50", "€50.00"),
        ("50 eur", "€50.00"),
        ("50 euros", "€50.00"),
        ("£75", "£75.00"),
        ("75 pounds", "£75.00"),
        ("₽100", "100.00₽"),
        ("100 rubles", "100.00₽"),
        ("₿1", "₿1.00"),
        ("1 bitcoin", "₿1.00"),
    ];

    for (expression, expected) in cases {
        assert_eq!(
            engine.eval(expression).to_string(),
            expected,
            "{expression}"
        );
    }
}

#[test]
fn test_currency_arithmetic_same() {
    let mut engine = Engine::new();

    // Addition of same currency - test with Decimal precision
    let result = engine.eval("$100 + $50");
    assert_eq!(result.as_decimal(), Some(d("150")));
    assert_eq!(result.to_string(), "$150.00");

    // Subtraction of same currency
    let result = engine.eval("€200 - €75");
    assert_eq!(result.as_decimal(), Some(d("125")));
    assert_eq!(result.to_string(), "€125.00");

    // Multiplication with scalar
    let result = engine.eval("$25 * 4");
    assert_eq!(result.as_decimal(), Some(d("100")));
    assert_eq!(result.to_string(), "$100.00");

    // Division with scalar
    let result = engine.eval("£100 / 4");
    assert_eq!(result.as_decimal(), Some(d("25")));
    assert_eq!(result.to_string(), "£25.00");
}

#[test]
fn test_direct_currency_conversions() {
    let mut engine = Engine::new();
    engine.set_exchange_rate(Currency::USD, Currency::EUR, d("0.92"));
    engine.set_exchange_rate(Currency::USD, Currency::GBP, d("0.79"));
    engine.set_exchange_rate(Currency::USD, Currency::JPY, d("149.5"));
    let cases = [
        ("$100 in EUR", "92", "€92.00"),
        ("$500 in pounds", "395", "£395.00"),
        ("$10 in jpy", "1495", "¥1495.00"),
    ];

    for (expression, amount, display) in cases {
        let result = engine.eval(expression);
        assert_eq!(result.as_decimal(), Some(d(amount)), "{expression}");
        assert_eq!(result.to_string(), display, "{expression}");
    }
}

#[test]
fn test_reverse_currency_conversion_is_exact_for_reciprocal_case() {
    let mut engine = Engine::new();
    engine.set_exchange_rate(Currency::USD, Currency::EUR, d("0.92"));

    assert_eq!(engine.eval("€92 in USD").as_decimal(), Some(d("100")));
}

#[test]
fn test_multi_currency_arithmetic() {
    let mut engine = Engine::new();
    engine.set_exchange_rate(Currency::USD, Currency::EUR, d("0.92"));

    // Adding different currencies converts to left currency
    let result = engine.eval("$100 + €46");
    // €46 = $50, so $100 + $50 = $150
    assert_eq!(result.as_decimal(), Some(d("150")));
    assert_eq!(result.to_string(), "$150.00");
}

#[test]
fn test_currency_with_variables() {
    let mut engine = Engine::new();
    engine.set_exchange_rate(Currency::USD, Currency::EUR, d("0.92"));

    engine.eval("income = $5000");
    engine.eval("expenses = €1000");

    // Variable with currency
    let result = engine.eval("income");
    assert_eq!(result.as_decimal(), Some(d("5000")));
    assert_eq!(result.to_string(), "$5000.00");

    // Convert variable
    let result = engine.eval("income in EUR");
    assert_eq!(result.as_decimal(), Some(d("4600")));
    assert_eq!(result.to_string(), "€4600.00");
}

#[test]
fn test_currency_percentage_operations() {
    let mut engine = Engine::new();

    // 15% of $200
    let result = engine.eval("15% of $200");
    assert_eq!(result.as_decimal(), Some(d("30")));
    assert_eq!(result.to_string(), "$30.00");

    // $100 + 20%
    let result = engine.eval("$100 + 20%");
    assert_eq!(result.as_decimal(), Some(d("120")));
    assert_eq!(result.to_string(), "$120.00");

    // €500 - 10%
    let result = engine.eval("€500 - 10%");
    assert_eq!(result.as_decimal(), Some(d("450")));
    assert_eq!(result.to_string(), "€450.00");
}

#[test]
fn test_rub_and_ils_currencies() {
    let mut engine = Engine::new();
    engine.set_exchange_rate(Currency::USD, Currency::RUB, d("92"));
    engine.set_exchange_rate(Currency::USD, Currency::ILS, d("3.65"));

    // USD to RUB
    let result = engine.eval("$100 in RUB");
    assert_eq!(result.as_decimal(), Some(d("9200")));
    assert_eq!(result.to_string(), "9200.00₽");

    // USD to ILS
    let result = engine.eval("$100 in ILS");
    assert_eq!(result.as_decimal(), Some(d("365")));
    assert_eq!(result.to_string(), "₪365.00");

    // RUB formatting (symbol after number)
    let result = engine.eval("₽5000");
    assert_eq!(result.as_decimal(), Some(d("5000")));
    assert_eq!(result.to_string(), "5000.00₽");

    // ILS formatting
    let result = engine.eval("₪100");
    assert_eq!(result.as_decimal(), Some(d("100")));
    assert_eq!(result.to_string(), "₪100.00");
}

#[test]
fn test_all_currencies_have_default_rates() {
    let mut engine = Engine::new();

    for currency in currency_catalog() {
        let expression = format!("1 {} in USD", currency.code);
        assert!(
            engine.eval(&expression).as_decimal().is_some(),
            "{expression}"
        );
    }
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
    engine.set_exchange_rate(Currency::ETH, Currency::USD, d("3000"));
    engine.set_exchange_rate(Currency::SOL, Currency::USD, d("140"));
    engine.set_exchange_rate(Currency::BTC, Currency::USD, d("92000"));

    // ETH to USD
    let result = engine.eval("1 ETH in USD");
    assert_eq!(result.as_decimal(), Some(d("3000")));
    assert_eq!(result.to_string(), "$3000.00");

    // SOL to USD
    let result = engine.eval("10 SOL in USD");
    assert_eq!(result.as_decimal(), Some(d("1400")));
    assert_eq!(result.to_string(), "$1400.00");

    // BTC to USD
    let result = engine.eval("0.5 BTC in USD");
    assert_eq!(result.as_decimal(), Some(d("46000")));
    assert_eq!(result.to_string(), "$46000.00");
}

#[test]
fn test_usd_to_crypto_conversion() {
    let mut engine = Engine::new();
    engine.set_exchange_rate(Currency::ETH, Currency::USD, d("3000"));

    // USD to ETH (inverse rate)
    // Note: Division can introduce tiny precision differences
    let result = engine.eval("$6000 in ETH");
    let eth = result.as_decimal().unwrap();
    assert!(
        (eth - d("2")).abs() < d("0.0000001"),
        "Expected ~2 ETH, got {eth}"
    );
    assert_eq!(result.to_string(), "Ξ2.00");
}

// ============================================================================
// CURRENCY PROPAGATION TESTS
// These tests document expected behavior for mixing plain numbers with currency
// ============================================================================

#[test]
fn test_plain_number_plus_currency_propagates() {
    let mut engine = Engine::new();

    // Plain numbers + currency should result in currency
    // The currency should propagate from whichever operand has it

    // Case 1: plain + plain + currency (user's example: 1000 + 1000 RUB)
    let result = engine.eval("500 + 500 + 1000 RUB");
    assert_eq!(result.as_decimal(), Some(d("2000")));
    assert_eq!(result.to_string(), "2000.00₽");

    // Case 2: plain + currency (simpler case)
    let result = engine.eval("1000 + 1000 RUB");
    assert_eq!(result.as_decimal(), Some(d("2000")));
    assert_eq!(result.to_string(), "2000.00₽");

    // Case 3: currency on left side (should already work)
    let result = engine.eval("1000 RUB + 500");
    assert_eq!(result.as_decimal(), Some(d("1500")));
    assert_eq!(result.to_string(), "1500.00₽");

    // Case 4: currency in the middle of expression
    let result = engine.eval("100 + 200 USD + 300");
    assert_eq!(result.as_decimal(), Some(d("600")));
    assert_eq!(result.to_string(), "$600.00");

    // Case 5: subtraction with plain number
    let result = engine.eval("2000 - 500 RUB");
    assert_eq!(result.as_decimal(), Some(d("1500")));
    assert_eq!(result.to_string(), "1500.00₽");
}

#[test]
fn test_plain_number_times_currency() {
    let mut engine = Engine::new();

    // Multiplication: plain × currency = currency
    let result = engine.eval("3 * 100 USD");
    assert_eq!(result.as_decimal(), Some(d("300")));
    assert_eq!(result.to_string(), "$300.00");

    // Division: currency / plain = currency
    let result = engine.eval("300 USD / 3");
    assert_eq!(result.as_decimal(), Some(d("100")));
    assert_eq!(result.to_string(), "$100.00");

    // plain / currency = plain (dimensionless ratio)
    let result = engine.eval("300 / 100 USD");
    // This case is ambiguous - currently returns plain number
    assert!(result.as_decimal().is_some());
}

// ============================================================================
// UNIT + CURRENCY INCOMPATIBILITY TESTS
// These test that adding/subtracting units and currency produces an error
// ============================================================================

#[test]
fn test_unit_plus_currency_errors() {
    let mut engine = Engine::new();

    // Adding time units to currency should error - incompatible types
    let result = engine.eval("5 hours + 100 RUB");
    assert!(
        result.is_error(),
        "hours + currency should error, got: {result}"
    );

    // Same for other units
    let result = engine.eval("10 kg + $50");
    assert!(
        result.is_error(),
        "kg + currency should error, got: {result}"
    );

    // Currency + unit (reversed order)
    let result = engine.eval("$100 + 5 hours");
    assert!(
        result.is_error(),
        "currency + hours should error, got: {result}"
    );

    // Subtraction should also error
    let result = engine.eval("100 RUB - 2 hours");
    assert!(
        result.is_error(),
        "currency - hours should error, got: {result}"
    );
}

#[test]
fn test_unit_times_currency_allowed() {
    let mut engine = Engine::new();

    // Multiplication IS allowed (rate calculation: hours × rate = money)
    let result = engine.eval("8 hours * $50");
    assert!(!result.is_error());
    assert_eq!(result.as_decimal(), Some(d("400")));
    assert_eq!(result.to_string(), "$400.00");

    // Currency × unit (reversed)
    let result = engine.eval("$85 * 45h");
    assert!(!result.is_error());
    assert_eq!(result.as_decimal(), Some(d("3825")));
    assert_eq!(result.to_string(), "$3825.00");

    // Months × currency (subscription calc)
    let result = engine.eval("12 months * 340 usd");
    assert!(!result.is_error());
    assert_eq!(result.as_decimal(), Some(d("4080")));
    assert_eq!(result.to_string(), "$4080.00");
}

#[test]
fn test_complex_rub_calculation_with_percentage() {
    let mut engine = Engine::new();
    engine.set_exchange_rate(Currency::RUB, Currency::ILS, d("0.039")); // ~1 RUB = 0.039 ILS

    // Test: (525000 rub + (75 000 rub * 1)) - 7%
    // Do arithmetic in same currency first, convert at end if needed
    // 1. 525000 + 75000 = 600000 RUB
    // 2. 600000 - 7% = 600000 * 0.93 = 558000 RUB

    // Test number with space (75 000)
    let result = engine.eval("75 000 rub");
    assert!(!result.is_error(), "75 000 rub failed: {result}");
    assert_eq!(result.as_decimal(), Some(d("75000")));

    // Test multiplication with 'x' operator
    let result = engine.eval("75000 rub x 1");
    assert!(!result.is_error(), "75000 rub x 1 failed: {result}");
    assert_eq!(result.as_decimal(), Some(d("75000")));

    // Test multiplication with '*' operator and space-separated number
    let result = engine.eval("75 000 rub * 1");
    assert!(!result.is_error(), "75 000 rub * 1 failed: {result}");
    assert_eq!(result.as_decimal(), Some(d("75000")));

    // Test parenthesized inner expression with space-separated number
    let result = engine.eval("(75 000 rub x 1)");
    assert!(!result.is_error(), "(75 000 rub x 1) failed: {result}");
    assert_eq!(result.as_decimal(), Some(d("75000")));

    // Test addition of same currency
    let result = engine.eval("525000 rub + 75000 rub");
    assert!(!result.is_error(), "addition failed: {result}");
    assert_eq!(result.as_decimal(), Some(d("600000")));

    // Test full expression: (525000 rub + (75 000 rub * 1)) - 7%
    let result = engine.eval("(525000 rub + (75 000 rub * 1)) - 7%");
    assert!(!result.is_error(), "Full expression failed: {result}");
    // 525000 + 75000 = 600000
    // 600000 - 7% = 558000
    assert_eq!(result.as_decimal(), Some(d("558000")));
    assert_eq!(result.to_string(), "558000.00₽");

    // Test with 'x' operator
    let result = engine.eval("(525 000 rub + (75 000 rub x 1)) - 7%");
    assert!(
        !result.is_error(),
        "Full expression with x failed: {result}"
    );
    assert_eq!(result.as_decimal(), Some(d("558000")));

    // Then convert to ILS at the end if needed
    let result = engine.eval("558000 rub in ils");
    assert!(!result.is_error(), "conversion failed: {result}");
    // 558000 * 0.039 = 21762
    assert_eq!(result.as_decimal(), Some(d("21762")));
    assert_eq!(result.to_string(), "₪21762.00");
}
