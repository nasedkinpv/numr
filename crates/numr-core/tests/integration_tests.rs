use numr_core::{decimal as d, Currency, Engine};

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

    // Test inverse conversion
    let result = engine.eval("€85 in USD");
    assert_eq!(result.as_decimal(), Some(d("100")));
    assert_eq!(result.to_string(), "$100.00");

    // Also test GBP
    engine.set_exchange_rate(Currency::USD, Currency::GBP, d("0.75"));
    let result = engine.eval("$200 in GBP");
    assert_eq!(result.as_decimal(), Some(d("150")));
    assert_eq!(result.to_string(), "£150.00");
}
