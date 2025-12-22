use numr_core::{decimal as d, Currency, Engine};

/// Ensure displayed values have reasonable precision (no infinite decimals)
fn assert_clean_display(result: &str) {
    // Check that the numeric portion doesn't have excessive decimal places
    // Currency should have max 2 decimal places in display
    // Extract numeric part (remove currency symbols and units)
    let numeric: String = result
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
        .collect();

    if let Some(dot_pos) = numeric.find('.') {
        let decimals = numeric.len() - dot_pos - 1;
        assert!(
            decimals <= 6,
            "Too many decimal places in '{}': {} decimals (from '{}')",
            result,
            decimals,
            numeric
        );
    }
}

#[test]
fn test_decimal_precision_formatting() {
    let mut engine = Engine::new();

    // Set up rates that would cause infinite decimals without proper rounding
    // 1 USD = 100 RUB, so 1 RUB = 0.01 USD (clean)
    engine.set_exchange_rate(Currency::USD, Currency::RUB, d("100"));

    // Simple conversion should be clean
    let result = engine.eval("200 USD + 299 RUB");
    assert!(!result.is_error(), "Should evaluate: {:?}", result);
    let display = result.to_string();
    assert_clean_display(&display);

    // Test with rate that produces repeating decimals (1/3 = 0.333...)
    engine.set_exchange_rate(Currency::USD, Currency::EUR, d("0.333333333"));
    let result = engine.eval("$100 in EUR");
    let display = result.to_string();
    assert_clean_display(&display);

    // Division that could produce infinite decimals
    let result = engine.eval("10 / 3");
    let display = result.to_string();
    assert_clean_display(&display);

    // Percentage calculation
    let result = engine.eval("33.33% of 100");
    let display = result.to_string();
    assert_clean_display(&display);
}

#[test]
fn test_multiplication_operators() {
    let mut engine = Engine::new();

    // Standard asterisk
    assert_eq!(engine.eval("10 * 10").as_decimal(), Some(d("100")));

    // Unicode multiplication sign
    assert_eq!(engine.eval("10 × 10").as_decimal(), Some(d("100")));

    // No spaces: "10x10" parses as implicit multiplication (10 * variable x10)
    // With spaces: "10 x 10" may conflict with suffixed_number grammar
    // Use asterisk or × for reliable multiplication
}

#[test]
fn test_complex_expressions() {
    let mut engine = Engine::new();

    // Arithmetic
    assert_eq!(engine.eval("10 + 20 * 3").as_decimal(), Some(d("70")));
    assert_eq!(engine.eval("(10 + 20) * 3").as_decimal(), Some(d("90")));

    // Variables
    engine.eval("x = 5");
    engine.eval("y = 10");
    assert_eq!(engine.eval("x * y").as_decimal(), Some(d("50")));

    // Units (if supported in core, assume basic unit support exists or is planned)
    // For now, test what we know works from lib.rs examples
    assert_eq!(engine.eval("20% of 150").as_decimal(), Some(d("30")));
}

#[test]
fn test_currency_conversion() {
    let mut engine = Engine::new();

    // Set rate: 1 USD = 0.85 EUR
    engine.set_exchange_rate(Currency::USD, Currency::EUR, d("0.85"));

    // Test conversion
    let result = engine.eval("$100 in EUR");
    assert_eq!(result.as_decimal(), Some(d("85")));
    assert_eq!(result.to_string(), "€85.00");
    assert_clean_display(&result.to_string());

    // Test inverse conversion
    let result = engine.eval("€85 in USD");
    assert_eq!(result.as_decimal(), Some(d("100")));
    assert_eq!(result.to_string(), "$100.00");
    assert_clean_display(&result.to_string());

    // Also test GBP
    engine.set_exchange_rate(Currency::USD, Currency::GBP, d("0.75"));
    let result = engine.eval("$200 in GBP");
    assert_eq!(result.as_decimal(), Some(d("150")));
    assert_eq!(result.to_string(), "£150.00");
    assert_clean_display(&result.to_string());
}

#[test]
fn test_currency_conversion_then_add() {
    let mut engine = Engine::new();

    // Set rates
    engine.set_exchange_rate(Currency::USD, Currency::EUR, d("0.92"));
    engine.set_exchange_rate(Currency::USD, Currency::ILS, d("3.65"));
    engine.set_exchange_rate(Currency::USD, Currency::RUB, d("100"));

    // Conversion then addition should work without parentheses
    // "200 rub in ils + 500 eur" should parse as "(200 rub in ils) + 500 eur"
    // not as "200 rub in (ils + 500 eur)" which is nonsense
    let result = engine.eval("200 rub in ils + 500 eur");
    assert!(
        !result.is_error(),
        "Expected successful eval, got: {:?}",
        result
    );

    // Result should be in ILS (first conversion target)
    // 200 RUB → ILS: 200 / 100 * 3.65 = 7.30 ILS
    // 500 EUR → ILS: 500 / 0.92 * 3.65 = 1983.70 ILS
    // Total: ~1991 ILS
    let amount = result.as_decimal().expect("should have decimal value");
    assert!(
        amount > d("1980") && amount < d("2000"),
        "Expected ~1991 ILS, got {}",
        amount
    );
    // Verify no infinite decimals in display
    assert_clean_display(&result.to_string());
}

#[test]
fn test_inch_unit() {
    let mut engine = Engine::new();

    // "5 inches" is unambiguous - full word for inches
    assert_eq!(engine.eval("5 inches").to_string(), "5 in");

    // "5 in" alone is correctly parsed as 5 inches (not swallowed as empty conversion)
    let result = engine.eval("5 in");
    assert!(
        !result.is_error(),
        "5 in should parse as 5 inches: {:?}",
        result
    );
    assert_eq!(result.to_string(), "5 in");

    // "5 inches to cm" - unit conversion
    let result = engine.eval("5 inches to cm");
    assert!(
        !result.is_error(),
        "5 inches to cm should work: {:?}",
        result
    );
    let amount = result.as_decimal().expect("should have decimal value");
    // 5 inches = 12.7 cm (1 inch = 2.54 cm)
    assert!(
        amount > d("12.6") && amount < d("12.8"),
        "Expected ~12.7 cm, got {}",
        amount
    );
    // Verify clean decimal display
    assert_clean_display(&result.to_string());
}

#[test]
fn test_in_conversion_keyword() {
    let mut engine = Engine::new();

    // Set up exchange rates
    engine.set_exchange_rate(Currency::USD, Currency::EUR, d("0.85"));

    // "$100 in EUR" - "in" is conversion operator
    let result = engine.eval("$100 in EUR");
    assert!(
        !result.is_error(),
        "$100 in EUR should be currency conversion"
    );
    assert_eq!(result.as_decimal(), Some(d("85")));
    assert_clean_display(&result.to_string());

    // "100 USD in EUR" - explicit currency, "in" is conversion
    let result = engine.eval("100 USD in EUR");
    assert_eq!(result.as_decimal(), Some(d("85")));
    assert_clean_display(&result.to_string());
}

#[test]
fn test_to_conversion_keyword() {
    let mut engine = Engine::new();

    engine.set_exchange_rate(Currency::USD, Currency::EUR, d("0.85"));
    engine.set_exchange_rate(Currency::USD, Currency::GBP, d("0.75"));

    // "to" should work same as "in" for conversion
    let result = engine.eval("$100 to EUR");
    assert_eq!(result.as_decimal(), Some(d("85")));
    assert_eq!(result.to_string(), "€85.00");
    assert_clean_display(&result.to_string());

    // "to" with unit conversion
    let result = engine.eval("1 km to m");
    assert_eq!(result.as_decimal(), Some(d("1000")));
    assert_clean_display(&result.to_string());

    // "to" in chained operations - parses left-to-right with same precedence
    // "(($50 to EUR) + $50) to GBP" = (€42.50 + €42.50) to GBP = €85 to GBP = £75
    let result = engine.eval("$50 to EUR + $50 to GBP");
    assert!(
        !result.is_error(),
        "Chained to conversions should work: {:?}",
        result
    );
    let amount = result.as_decimal().expect("should have decimal value");
    assert!(
        amount > d("74") && amount < d("76"),
        "Expected ~75 GBP, got {}",
        amount
    );
    // Verify display has clean decimal precision
    assert_clean_display(&result.to_string());
}
