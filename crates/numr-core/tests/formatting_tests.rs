use numr_core::Engine;

#[test]
fn test_unit_rounding() {
    let mut engine = Engine::new();

    // 1 mile in km is 1.609344
    // 100 miles in km is 160.9344
    let res = engine.eval("100 mi in km");

    // Currently, this likely returns "160.9344 km"
    // We want it to be rounded, e.g., "160.93 km" or similar if max 2 decimals is enforced

    // Let's check the string representation
    let res_str = res.to_string();
    println!("Result: {}", res_str);

    // This assertion expects the CURRENT behavior (failure if we expect rounding)
    // or we can assert the DESIRED behavior and see it fail.
    // Let's assert the desired behavior to confirm it fails.
    assert_eq!(res_str, "160.93 km");
}

#[test]
fn test_currency_rounding() {
    let mut engine = Engine::new();
    let res = engine.eval("$10 / 3"); // 3.33333... USD
    assert_eq!(res.to_string(), "$3.33");
}

fn display(engine: &mut Engine, input: &str, decimals: u32) -> String {
    engine
        .eval(input)
        .format_with_options(numr_core::DisplayOptions {
            decimals: Some(numr_core::DecimalPlaces::new(decimals).unwrap()),
        })
}

#[test]
fn configurable_precision_applies_to_all_decimal_values() {
    let mut engine = Engine::new();
    assert_eq!(display(&mut engine, "152/150", 4), "1.0133");
    assert_eq!(display(&mut engine, "(-152)/150", 4), "-1.0133");
    assert_eq!(display(&mut engine, "100 mi in km", 4), "160.9344 km");
    assert_eq!(display(&mut engine, "$10 / 3", 4), "$3.3333");
    assert_eq!(display(&mut engine, "1.234567%", 4), "1.2346%");
    assert_eq!(display(&mut engine, "1.234567°", 4), "1.2346°");
    assert_eq!(display(&mut engine, "22 to hex", 4), "0x16");
    assert_eq!(display(&mut engine, "22 to bin", 4), "0b10110");
    assert_eq!(display(&mut engine, "# comment", 4), "");
}

#[test]
fn precision_boundaries_and_rounding() {
    let mut engine = Engine::new();
    assert_eq!(display(&mut engine, "1.9", 0), "2");
    assert_eq!(display(&mut engine, "(-1.9)", 0), "-2");
    assert_eq!(display(&mut engine, "$1.9", 0), "$2");
    assert_eq!(display(&mut engine, "0.001", 2), "0");
    assert_eq!(display(&mut engine, "(-0.001)", 2), "0");
    assert_eq!(display(&mut engine, "9.99999", 4), "10");
    assert_eq!(
        display(&mut engine, "1/3", 28),
        format!("0.{}", "3".repeat(28))
    );
    assert!(numr_core::DecimalPlaces::new(u32::MAX).is_err());
}

#[test]
fn display_options_do_not_round_stored_results_or_variables() {
    let mut engine = Engine::new();
    let result = engine.eval("a = 152/150");
    let amount = result.as_decimal().unwrap();
    assert_eq!(
        result.format_with_options(numr_core::DisplayOptions {
            decimals: Some(numr_core::DecimalPlaces::new(0).unwrap())
        }),
        "1"
    );
    assert_eq!(result.as_decimal(), Some(amount));
    assert_eq!(
        engine.eval("a * 150").as_decimal(),
        Some(numr_core::Decimal::from(152))
    );
    assert_eq!(
        result.format_with_options(numr_core::DisplayOptions {
            decimals: Some(numr_core::DecimalPlaces::new(4).unwrap())
        }),
        "1.0133"
    );
    assert_eq!(result.to_string(), "1.01");
}

#[test]
fn omitted_precision_preserves_existing_display_rules() {
    let mut engine = Engine::new();
    for (input, expected) in [
        ("152/150", "1.01"),
        ("0.001", "0.001"),
        ("$1", "$1.00"),
        ("1 BTC / 3", "₿0.33333333"),
        ("1.23456%", "1.23%"),
        ("1.23456°", "1.23°"),
        ("1.23456 kg", "1.23 kg"),
        ("22 to hex", "0x16"),
        ("22 to bin", "0b10110"),
        ("# comment", ""),
        ("1/0", "Error: division by zero"),
    ] {
        let result = engine.eval(input);
        assert_eq!(
            result.format_with_options(numr_core::DisplayOptions::default()),
            expected,
            "{input}"
        );
        assert_eq!(result.to_string(), expected, "{input}");
    }
    assert_eq!(engine.eval("0.001").to_string(), "0.001");
    assert_eq!(engine.eval("$1").to_string(), "$1.00");
}

#[test]
fn rounding_ties_and_negative_zero_follow_decimal_rules() {
    use numr_core::{decimal, Currency, DisplayOptions, Value};
    let whole = DisplayOptions {
        decimals: Some(numr_core::DecimalPlaces::new(0).unwrap()),
    };
    for (input, expected) in [
        ("2.5", "2"),
        ("3.5", "4"),
        ("-2.5", "-2"),
        ("-3.5", "-4"),
        ("-0.1", "0"),
    ] {
        assert_eq!(
            Value::Number(decimal(input)).format_with_options(whole),
            expected
        );
    }
    assert_eq!(
        Value::currency(decimal("-0.001"), Currency::USD).format_with_options(DisplayOptions {
            decimals: Some(numr_core::DecimalPlaces::new(2).unwrap())
        }),
        "$0.00"
    );
    let max = Value::Number(numr_core::Decimal::MAX);
    assert_eq!(
        max.format_with_options(DisplayOptions {
            decimals: Some(numr_core::DecimalPlaces::new(28).unwrap())
        }),
        max.to_string()
    );
    let percentage = Value::Percentage(numr_core::Decimal::MAX);
    assert_eq!(
        percentage.format_with_options(DisplayOptions {
            decimals: Some(numr_core::DecimalPlaces::new(28).unwrap())
        }),
        percentage.to_string()
    );
}

#[test]
fn explicit_currency_precision_overrides_registry_default() {
    use numr_core::{decimal, Currency, DisplayOptions, Value};
    let btc = Value::currency(decimal("0.12345678"), Currency::BTC);
    assert_eq!(btc.to_string(), "₿0.12345678");
    assert_eq!(
        btc.format_with_options(DisplayOptions {
            decimals: Some(numr_core::DecimalPlaces::new(4).unwrap())
        }),
        "₿0.1235"
    );
    let dollars = Value::currency(decimal("1.50000"), Currency::USD);
    assert_eq!(
        dollars.format_with_options(DisplayOptions {
            decimals: Some(numr_core::DecimalPlaces::new(4).unwrap())
        }),
        "$1.50"
    );
}

#[test]
fn decimal_places_is_validated_at_construction_and_deserialization() {
    use numr_core::DecimalPlaces;
    for value in [0, 4, DecimalPlaces::MAX] {
        let precision = DecimalPlaces::new(value).unwrap();
        assert_eq!(precision.get(), value);
        assert_eq!(u32::from(precision), value);
        assert_eq!(DecimalPlaces::try_from(value).unwrap(), precision);
        let json = serde_json::to_string(&precision).unwrap();
        assert_eq!(
            serde_json::from_str::<DecimalPlaces>(&json).unwrap(),
            precision
        );
    }
    for value in [29, u32::MAX] {
        assert!(DecimalPlaces::new(value).is_err());
        assert!(DecimalPlaces::try_from(value).is_err());
    }
    for json in ["-1", "29", "1.5", "4294967296", "\"4\"", "null"] {
        assert!(serde_json::from_str::<DecimalPlaces>(json).is_err());
    }
}
