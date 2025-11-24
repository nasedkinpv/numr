use numr_core::{Engine, Value};

#[test]
fn test_division_by_zero() {
    let mut engine = Engine::new();
    let result = engine.eval("100 / 0");
    assert!(matches!(result, Value::Error(_)));
}

#[test]
fn test_recursive_definition() {
    let mut engine = Engine::new();
    engine.eval("x = 10");
    let _result = engine.eval("x = x + 1");
    // Depending on implementation, this might be allowed (redefinition) or error (recursive ref during eval)
    // If it's redefinition: x becomes 11.
    // If it's recursive: error.
    // Let's assume redefinition is allowed in this language design,
    // but if we did "x = x + 1" without prior x, it should fail.

    let mut engine2 = Engine::new();
    let result2 = engine2.eval("y = y + 1");
    assert!(matches!(result2, Value::Error(_)));
}

#[test]
fn test_case_sensitivity() {
    let mut engine = Engine::new();
    engine.eval("Var = 10");
    engine.eval("var = 20");

    let res1 = engine.eval("Var");
    let res2 = engine.eval("var");

    // Assuming case sensitivity
    assert_eq!(res1.as_f64(), Some(10.0));
    assert_eq!(res2.as_f64(), Some(20.0));
}

#[test]
fn test_unit_mismatch() {
    let mut engine = Engine::new();
    let result = engine.eval("10 kg + 5 meters");
    assert!(matches!(result, Value::Error(_)));
}

#[test]
fn test_floating_point_precision() {
    let mut engine = Engine::new();
    // 0.1 + 0.2 is notoriously 0.30000000000000004
    let result = engine.eval("0.1 + 0.2");
    if let Some(val) = result.as_f64() {
        assert!((val - 0.3).abs() < 1e-10);
    } else {
        panic!("Expected number");
    }
}

#[test]
fn test_parentheses_with_variables() {
    let mut engine = Engine::new();
    engine.eval("x = 10");
    engine.eval("y = 5");

    // (x + y) * 2 = 30
    let res1 = engine.eval("(x + y) * 2");
    assert_eq!(res1.as_f64(), Some(30.0));

    // x + (y * 2) = 20
    let res2 = engine.eval("x + (y * 2)");
    assert_eq!(res2.as_f64(), Some(20.0));

    // Nested: ((x + y) * 2) / 3 = 10
    let res3 = engine.eval("((x + y) * 2) / 3");
    assert_eq!(res3.as_f64(), Some(10.0));
}

#[test]
fn test_parentheses_with_units() {
    let mut engine = Engine::new();

    // (1 km + 500 m) in m = 1500 m
    // Note: The engine might return a value with unit.
    // We need to check if the string representation or underlying value is correct.
    // Assuming as_f64 returns the magnitude if it's a unit value, or we might need to check string.
    // Let's check the string output for unit correctness or if we can access unit.
    // Since we don't have easy access to Unit struct here without importing, let's rely on string or magnitude if converted.

    // 1 km = 1000 m. 1000 + 500 = 1500.
    let res = engine.eval("(1 km + 500 m) in m");
    // The result should be 1500 (magnitude).
    assert_eq!(res.as_f64(), Some(1500.0));
}

#[test]
fn test_complex_unit_conversions() {
    let mut engine = Engine::new();

    // Temperature: (100 C to F)
    // 100 * 9/5 + 32 = 180 + 32 = 212
    let res_temp = engine.eval("100 C in F");
    assert_eq!(res_temp.as_f64(), Some(212.0));

    // Weight: 1 kg in g = 1000
    let res_weight = engine.eval("1 kg in g");
    assert_eq!(res_weight.as_f64(), Some(1000.0));

    // Mixed with math: (1 kg * 2) in g = 2000
    let res_mixed = engine.eval("(1 kg * 2) in g");
    assert_eq!(res_mixed.as_f64(), Some(2000.0));
}
