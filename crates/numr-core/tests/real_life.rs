use numr_core::{decimal as d, Currency, Engine};

#[test]
fn test_real_life_scenario() {
    let mut engine = Engine::new();
    engine.set_exchange_rate(Currency::USD, Currency::RUB, d("60"));
    engine.set_exchange_rate(Currency::ILS, Currency::RUB, d("10")); // 1 ILS = 10 RUB approx for easy math

    // 1. Variable assignment with conversion
    // "wit = 300$ in rub" -> 300 * 60 = 18000 RUB
    let res1 = engine.eval("wit = 300$ in rub");
    // RUB uses symbol after number (Russian convention)
    assert_eq!(res1.as_decimal(), Some(d("18000")));
    assert_eq!(res1.to_string(), "18000.00₽");

    // 2. Mixed currency arithmetic with text prefix
    // "wit + 200 ils in usd"
    // wit (18000 RUB) + 200 ILS (2000 RUB) = 20000 RUB
    // 20000 RUB in USD ( / 60) = 333.33 USD
    // Also, "string here before" should be ignored.
    let res2 = engine.eval("string here before wit + 200 ils in usd");
    // Output should be in USD (symbol $)
    println!("Res2: {res2}");
    assert!(res2.to_string().contains("$"));

    // 3. Total
    // "total" should sum previous lines.
    let res3 = engine.eval("total");
    assert!(res3.as_decimal().unwrap() > d("600"));

    // 4. Formats
    // $4000 -> 4000 USD
    let res4 = engine.eval("$4000");
    assert_eq!(res4.as_decimal(), Some(d("4000")));
    assert_eq!(res4.to_string(), "$4000.00");

    // 3500$ -> 3500 USD
    let res5 = engine.eval("3500$");
    assert_eq!(res5.as_decimal(), Some(d("3500")));
    assert_eq!(res5.to_string(), "$3500.00");

    // 3500 $ -> 3500 USD
    let res6 = engine.eval("3500 $");
    assert_eq!(res6.as_decimal(), Some(d("3500")));
    assert_eq!(res6.to_string(), "$3500.00");

    // $ 4000 -> 4000 USD
    let res7 = engine.eval("$ 4000");
    assert_eq!(res7.as_decimal(), Some(d("4000")));
    assert_eq!(res7.to_string(), "$4000.00");

    // 5. BTC
    // Set explicit rate for test consistency
    engine.set_exchange_rate(Currency::BTC, Currency::USD, d("95000"));
    let res8 = engine.eval("1 btc in usd");
    assert_eq!(res8.as_decimal(), Some(d("95000")));
    assert_eq!(res8.to_string(), "$95000.00");

    // 6. Other currencies
    // 100 EUR in USD (Rate 0.92 USD -> EUR => 1 EUR = 1/0.92 USD = 1.087 USD)
    let res9 = engine.eval("100 eur in usd");
    assert!(res9.to_string().contains("$"));
    assert!(res9.as_decimal().is_some());

    // 100 JPY in USD (Rate 150 USD -> JPY)
    let res10 = engine.eval("100 jpy in usd");
    assert!(res10.to_string().contains("$"));
    assert!(res10.as_decimal().is_some());
}

#[test]
fn test_mixed_currency_with_trailing_text() {
    let mut engine = Engine::new();
    // Set explicit rates for consistent test results
    engine.set_exchange_rate(Currency::USD, Currency::RUB, d("100")); // 1 USD = 100 RUB
    engine.set_exchange_rate(Currency::USD, Currency::EUR, d("0.9")); // 1 USD = 0.9 EUR

    // Basic multi-currency conversion works
    // 10000 RUB = $100, + $1000 = $1100
    // $1100 in EUR = €990
    let result = engine.eval("10000 rubles + 1000 usd in eur");
    assert_eq!(result.as_decimal(), Some(d("990")));
    assert_eq!(result.to_string(), "€990.00");

    // Trailing prose text after conversion is ignored
    let result = engine.eval("10000 rubles + 1000 usd in eur and some text here without comment");
    assert_eq!(result.as_decimal(), Some(d("990")));
    assert_eq!(result.to_string(), "€990.00");
}
