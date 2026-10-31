use numr_core::{decimal, Decimal, Engine, Value};

#[test]
fn round_accepts_decimal_places_and_midpoint_to_even_ties() {
    for (expression, expected) in [
        ("round(1.234567, 4)", "1.2346"),
        ("round(1.234567, 0)", "1"),
        ("round(2.345, 2)", "2.34"),
        ("round(2.355, 2)", "2.36"),
        ("round(-2.345, 2)", "-2.34"),
        ("round(-2.355, 2)", "-2.36"),
        ("round(9.99999, 4)", "10"),
        ("round(-0.001, 2)", "0"),
        ("ROUND(1.234567, 4)", "1.2346"),
        ("round(1.234567, 2 + 2)", "1.2346"),
        ("round(1 / 3, 28)", "0.3333333333333333333333333333"),
    ] {
        assert_eq!(
            Engine::new().eval(expression).as_decimal(),
            Some(decimal(expected)),
            "{expression}"
        );
    }
    let expression = format!("round({}, 28)", Decimal::MAX);
    assert_eq!(
        Engine::new().eval(&expression).as_decimal(),
        Some(Decimal::MAX)
    );
}

#[test]
fn round_rejects_invalid_precision_without_panicking() {
    for precision in [
        "-1",
        "29",
        "1.5",
        "4294967296",
        "79228162514264337593543950335",
        "$2",
        "2 kg",
        "2%",
    ] {
        let expression = format!("round(1.234567, {precision})");
        let value = Engine::new().eval(&expression);
        assert_eq!(
            value.to_string(),
            "Error: round precision must be an integer between 0 and 28",
            "{expression}"
        );
    }
    for expression in [
        "round()",
        "round(1, 2, 3)",
        "round(missing, 2)",
        "round(1, missing)",
        "round(1, 1 / 0)",
    ] {
        assert!(Engine::new().eval(expression).is_error(), "{expression}");
    }
}

#[test]
fn round_changes_stored_results_not_only_their_display() {
    let mut engine = Engine::new();
    engine.eval("places = 4");
    let value = engine.eval("rounded = round(1.234567, places)");
    assert_eq!(value, Value::Number(decimal("1.2346")));
    assert_eq!(value.to_string(), "1.23");
    assert_eq!(
        engine.eval("rounded * 10000").as_decimal(),
        Some(decimal("12346"))
    );
    assert_eq!(
        engine.eval("round(1 / 3, 2) * 3").as_decimal(),
        Some(decimal("0.99"))
    );
}

#[test]
fn round_without_precision_preserves_existing_numeric_coercion() {
    for (input, expected) in [
        ("2.5", "2"),
        ("3.5", "4"),
        ("-2.5", "-2"),
        ("3.7", "4"),
        ("$3.7", "4"),
        ("2 km in m", "2000"),
    ] {
        let mut engine = Engine::new();
        let original = engine.eval(&format!("round({input})"));
        assert_eq!(original, Value::Number(decimal(expected)), "{input}");
        assert_eq!(
            engine.eval(&format!("round({input}, 0)")),
            original,
            "{input}"
        );
    }
}
