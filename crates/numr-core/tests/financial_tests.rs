//! Financial calculation tests
//! Tests for percentages, taxes, discounts, and financial formulas

use numr_core::Engine;

#[test]
fn test_percentage_of_value() {
    let mut engine = Engine::new();

    // Basic percentage of
    let result = engine.eval("20% of 150");
    assert_eq!(result.as_f64(), Some(30.0));

    let result = engine.eval("15% of 200");
    assert_eq!(result.as_f64(), Some(30.0));

    let result = engine.eval("50% of 80");
    assert_eq!(result.as_f64(), Some(40.0));
}

#[test]
fn test_tax_calculations() {
    let mut engine = Engine::new();

    // Define tax rate
    engine.eval("tax = 20%");

    // Price + tax
    let result = engine.eval("100 + tax");
    assert_eq!(result.as_f64(), Some(120.0));

    // Multiple items with tax
    engine.eval("item1 = 50");
    engine.eval("item2 = 75");
    let result = engine.eval("item1 + item2 + tax");
    assert_eq!(result.as_f64(), Some(150.0)); // 125 + 25 = 150
}

#[test]
fn test_discount_calculations() {
    let mut engine = Engine::new();

    // 10% discount on $100
    let result = engine.eval("$100 - 10%");
    assert_eq!(result.to_string(), "$90");

    // 25% discount
    engine.eval("discount = 25%");
    let result = engine.eval("$200 - discount");
    assert_eq!(result.to_string(), "$150");
}

#[test]
fn test_tip_calculations() {
    let mut engine = Engine::new();

    // Restaurant bill with tip
    engine.eval("bill = $85");
    engine.eval("tip = 18%");

    // Calculate tip amount
    let result = engine.eval("18% of $85");
    assert_eq!(result.to_string(), "$15.3");

    // Total with tip
    let result = engine.eval("bill + tip");
    assert_eq!(result.to_string(), "$100.3");
}

#[test]
fn test_profit_margin() {
    let mut engine = Engine::new();

    // Cost and selling price
    engine.eval("cost = 80");
    engine.eval("price = 100");

    // Profit
    let result = engine.eval("price - cost");
    assert_eq!(result.as_f64(), Some(20.0));

    // Markup percentage (20% of cost)
    let result = engine.eval("20% of cost");
    assert_eq!(result.as_f64(), Some(16.0));
}

#[test]
fn test_compound_percentages() {
    let mut engine = Engine::new();

    // Apply multiple percentage changes
    // Start with 100, add 10%, then subtract 10%
    // 100 + 10% = 110
    let result = engine.eval("100 + 10%");
    let value = result.as_f64().unwrap();
    assert!((value - 110.0).abs() < 0.001);

    // 110 - 10% = 99 (not back to 100!)
    let result = engine.eval("110 - 10%");
    let value = result.as_f64().unwrap();
    assert!((value - 99.0).abs() < 0.001);
}

#[test]
fn test_budget_tracking() {
    let mut engine = Engine::new();

    // Monthly budget scenario
    engine.eval("salary = $5000");
    engine.eval("rent = $1500");
    engine.eval("utilities = $200");
    engine.eval("groceries = $400");
    engine.eval("transport = $150");

    // Savings target: 20% of salary
    let result = engine.eval("20% of salary");
    assert_eq!(result.to_string(), "$1000");
}

#[test]
fn test_investment_returns() {
    let mut engine = Engine::new();

    // Investment with 8% return
    engine.eval("principal = $10000");
    engine.eval("return_rate = 8%");

    // Calculate return
    let result = engine.eval("8% of principal");
    assert_eq!(result.to_string(), "$800");

    // New balance after one year
    let result = engine.eval("principal + return_rate");
    assert_eq!(result.to_string(), "$10800");
}
