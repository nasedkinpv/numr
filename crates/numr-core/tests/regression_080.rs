use numr_core::{Decimal, Engine, Value};

fn unit_value(value: &Value) -> (Decimal, &str) {
    match value {
        Value::WithCompoundUnit { amount, unit } => (*amount, &unit.symbol),
        other => panic!("expected a compound unit value, got {other:?}"),
    }
}

#[test]
fn physical_values_use_one_public_representation() {
    let mut engine = Engine::new();

    for expression in ["1 km", "1 km in m", "2 * 3 hours", "90deg to rad"] {
        assert!(
            matches!(engine.eval(expression), Value::WithCompoundUnit { .. }),
            "{expression} must produce the canonical unit representation"
        );
    }
}

#[test]
fn grouped_unit_totals_are_sorted_and_use_the_last_display_unit() {
    let mut engine = Engine::new();
    engine.evaluate_document("1 km\n2 kg\n3 s\n500 m");

    let totals = engine.grouped_totals();
    assert_eq!(totals.len(), 3);
    assert_eq!(unit_value(&totals[0]), (Decimal::from(3), "s"));
    assert_eq!(unit_value(&totals[1]), (Decimal::from(2), "kg"));
    assert_eq!(unit_value(&totals[2]), (Decimal::from(1_500), "m"));
}

#[test]
fn grouped_affine_units_convert_each_value_before_summing() {
    let mut engine = Engine::new();
    engine.evaluate_document("68 F\n86 F");

    let totals = engine.grouped_totals();
    assert_eq!(totals.len(), 1);
    assert_eq!(unit_value(&totals[0]), (Decimal::from(154), "°F"));

    engine.evaluate_document("20 C\n68 F");
    let totals = engine.grouped_totals();
    assert_eq!(totals.len(), 1);
    assert_eq!(unit_value(&totals[0]), (Decimal::from(136), "°F"));

    engine.evaluate_document("20 C\n68 F\n20 C");
    let totals = engine.grouped_totals();
    assert_eq!(totals.len(), 1);
    assert_eq!(totals[0].to_string(), "60 °C");
}
