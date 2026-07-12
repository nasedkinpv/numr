//! Focused percentage and financial-formula regressions.

use numr_core::Engine;

#[test]
fn percentage_expressions() {
    let cases = [
        ("20% of 150", 30.0),
        ("50% of 80", 40.0),
        ("100 + 10%", 110.0),
        ("110 - 10%", 99.0),
    ];
    let mut engine = Engine::new();

    for (expression, expected) in cases {
        assert_eq!(
            engine.eval(expression).as_f64(),
            Some(expected),
            "{expression}"
        );
    }
}

#[test]
fn percentage_variables_apply_to_accumulated_values() {
    let mut engine = Engine::new();
    engine.eval("tax = 20%");
    engine.eval("item1 = 50");
    engine.eval("item2 = 75");

    assert_eq!(engine.eval("item1 + item2 + tax").as_f64(), Some(150.0));
}

#[test]
fn currency_percentage_expressions() {
    let cases = [
        ("$100 - 10%", "$90.00"),
        ("18% of $85", "$15.30"),
        ("$85 + 18%", "$100.30"),
    ];
    let mut engine = Engine::new();

    for (expression, expected) in cases {
        assert_eq!(
            engine.eval(expression).to_string(),
            expected,
            "{expression}"
        );
    }
}

#[test]
fn stateful_profit_formula() {
    let mut engine = Engine::new();
    engine.eval("cost = 80");
    engine.eval("price = 100");

    assert_eq!(engine.eval("price - cost").as_f64(), Some(20.0));
    assert_eq!(engine.eval("20% of cost").as_f64(), Some(16.0));
}
