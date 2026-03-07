use numr_core::{decimal as d, Engine};

#[test]
fn test_decimal_to_hex() {
    let mut engine = Engine::new();

    let result = engine.eval("22 to hex");
    assert_eq!(result.as_decimal(), Some(d("22")));
    assert_eq!(result.to_string(), "0x16");
}

#[test]
fn test_decimal_to_binary() {
    let mut engine = Engine::new();

    let result = engine.eval("22 to bin");
    assert_eq!(result.as_decimal(), Some(d("22")));
    assert_eq!(result.to_string(), "0b10110");
}

#[test]
fn test_negative_decimal_to_hex() {
    let mut engine = Engine::new();

    let result = engine.eval("-42 to hex");
    assert_eq!(result.as_decimal(), Some(d("-42")));
    assert_eq!(result.to_string(), "-0x2a");
}

#[test]
fn test_base_conversion_works_with_continuation() {
    let mut engine = Engine::new();

    assert_eq!(engine.eval("22").to_string(), "22");
    assert_eq!(engine.eval("to hex").to_string(), "0x16");
    assert_eq!(engine.eval("+ 1").as_decimal(), Some(d("23")));
}

#[test]
fn test_base_conversion_variable_roundtrip() {
    let mut engine = Engine::new();

    assert_eq!(engine.eval("mask = 255 to hex").to_string(), "0xff");
    assert_eq!(engine.eval("mask").to_string(), "0xff");
    assert_eq!(engine.eval("mask + 1").as_decimal(), Some(d("256")));
    assert_eq!(engine.eval("mask to bin").to_string(), "0b11111111");
}

#[test]
fn test_base_conversion_requires_integer() {
    let mut engine = Engine::new();

    let result = engine.eval("10.5 to hex");
    assert!(result.is_error());
    assert_eq!(
        result.to_string(),
        "Error: Base conversion requires an integer"
    );
}

#[test]
fn test_base_conversion_rejects_units() {
    let mut engine = Engine::new();

    let result = engine.eval("5 km to hex");
    assert!(result.is_error());
    assert_eq!(
        result.to_string(),
        "Error: Base conversion requires a plain integer number"
    );
}
