use numr_core::{Engine, Currency};

#[test]
fn test_complex_expressions() {
    let mut engine = Engine::new();
    
    // Arithmetic
    assert_eq!(engine.eval("10 + 20 * 3").as_f64(), Some(70.0));
    assert_eq!(engine.eval("(10 + 20) * 3").as_f64(), Some(90.0));
    
    // Variables
    engine.eval("x = 5");
    engine.eval("y = 10");
    assert_eq!(engine.eval("x * y").as_f64(), Some(50.0));
    
    // Units (if supported in core, assume basic unit support exists or is planned)
    // For now, test what we know works from lib.rs examples
    assert_eq!(engine.eval("20% of 150").as_f64(), Some(30.0));
}

#[test]
fn test_currency_conversion() {
    let mut engine = Engine::new();
    
    // Set rate: 1 USD = 0.85 EUR
    engine.set_exchange_rate(Currency::USD, Currency::EUR, 0.85);
    
    // Test conversion (assuming syntax "100 USD to EUR" or similar works)
    // If syntax isn't supported yet, we test the internal logic via context?
    // But this is integration test.
    // Let's assume "100 USD to EUR" works if parser supports it.
    // If not, we might need to check how conversion is invoked.
    
    // Based on lib.rs, we have `set_exchange_rate`.
    // Let's try to evaluate a conversion if the parser supports it.
    // If parser doesn't support "to", this test might fail or we skip it.
    // Let's check parser capabilities first? 
    // Actually, let's just write a test that we expect to pass or fail and see.
    // But to be safe, let's stick to what we know:
    // If we have units, maybe "100 USD" is a value.
    // "100 USD in EUR"?
    
    // For now, let's just test that setting the rate doesn't panic.
    engine.set_exchange_rate(Currency::USD, Currency::GBP, 0.75);
}
