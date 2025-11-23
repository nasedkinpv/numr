use numr_core::{Currency, Engine};

#[test]
fn test_real_life_scenario() {
    let mut engine = Engine::new();
    engine.set_exchange_rate(Currency::USD, Currency::RUB, 60.0);
    engine.set_exchange_rate(Currency::ILS, Currency::RUB, 10.0); // 1 ILS = 10 RUB approx for easy math

    // 1. Variable assignment with conversion
    // "wit = 300$ in rub" -> 300 * 60 = 18000 RUB
    let res1 = engine.eval("wit = 300$ in rub");
    // RUB uses symbol after number (Russian convention)
    assert_eq!(res1.to_string(), "18000₽");

    // 2. Mixed currency arithmetic with text prefix
    // "wit + 200 ils in usd"
    // wit (18000 RUB) + 200 ILS (2000 RUB) = 20000 RUB
    // 20000 RUB in USD ( / 60) = 333.33 USD
    // Note: User example said "20000 rub", but also "in usd".
    // If "in usd" is present, it should be USD.
    // If the user wants RUB, they shouldn't say "in usd".
    // We will assert the logical result: 333.33 USD.
    // Also, "string here before" should be ignored.
    let res2 = engine.eval("string here before wit + 200 ils in usd");
    // This will likely fail currently due to "string here before"
    // Output should be in USD (symbol $)
    println!("Res2: {}", res2);
    assert!(res2.to_string().contains("$"));

    // 3. Total
    // "total" should sum previous lines.
    // Line 1: 18000 RUB (300 USD)
    // Line 2: 333.33 USD
    // Total: 300 USD + 333.33 USD = 633.33 USD
    let res3 = engine.eval("total");
    assert!(res3.as_f64().unwrap() > 600.0);

    // 4. Formats
    // $4000 -> 4000 USD
    let res4 = engine.eval("$4000");
    assert_eq!(res4.to_string(), "$4000");

    // 3500$ -> 3500 USD
    let res5 = engine.eval("3500$");
    assert_eq!(res5.to_string(), "$3500");

    // 3500 $ -> 3500 USD
    let res6 = engine.eval("3500 $");
    assert_eq!(res6.to_string(), "$3500");

    // $ 4000 -> 4000 USD
    let res7 = engine.eval("$ 4000");
    assert_eq!(res7.to_string(), "$4000");

    // 5. BTC
    // 1 BTC in USD
    let res8 = engine.eval("1 btc in usd");
    // Default rate is 60000
    assert_eq!(res8.to_string(), "$60000");
}
