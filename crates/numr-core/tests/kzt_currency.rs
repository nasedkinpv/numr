//! PR #20: tenge must work through the shared currency catalog and rate adapter.
use numr_core::{
    catalog::currency_catalog, decimal, Currency, DecimalPlaces, DisplayOptions, Engine,
};
use std::collections::HashMap;

#[test]
fn tenge_symbols_codes_and_aliases_share_one_currency() {
    let mut engine = Engine::new();
    for expression in ["₸150", "150₸", "150 KZT", "150 kzt", "150 tenge"] {
        let result = engine.eval(expression);
        assert_eq!(result.as_decimal(), Some(decimal("150")), "{expression}");
        assert_eq!(result.to_string(), "150.00₸", "{expression}");
    }
    let catalog = currency_catalog();
    let kzt = catalog.iter().find(|entry| entry.code == "KZT").unwrap();
    assert_eq!(kzt.symbol, "₸");
    assert!(!kzt.is_crypto);
    assert_eq!("tenge".parse::<Currency>().unwrap(), Currency::KZT);
}

#[test]
fn tenge_conversions_use_updated_rates_in_both_directions() {
    let mut engine = Engine::new();
    assert_eq!(engine.eval("$1 in KZT").as_decimal(), Some(decimal("450")));
    engine
        .apply_raw_rates(&HashMap::from([
            ("KZT".into(), decimal("500")),
            ("EUR".into(), decimal("0.8")),
        ]))
        .unwrap();
    for (expression, expected) in [
        ("$2 in KZT", "1000"),
        ("1000 KZT in USD", "2"),
        ("1000 tenge in EUR", "1.6"),
        ("€1 to KZT", "625"),
        ("100 KZT + $1", "600"),
    ] {
        assert_eq!(
            engine.eval(expression).as_decimal(),
            Some(decimal(expected)),
            "{expression}"
        );
    }
}

#[test]
fn tenge_precision_and_document_totals_preserve_amounts() {
    let mut engine = Engine::new();
    let document = engine.evaluate_document("price = 1.234567 KZT\nprice * 2");
    let options = DisplayOptions {
        decimals: Some(DecimalPlaces::new(4).unwrap()),
    };
    assert_eq!(
        document.lines[0].value.format_with_options(options),
        "1.2346₸"
    );
    assert_eq!(
        document.lines[1].value.as_decimal(),
        Some(decimal("2.469134"))
    );
    assert_eq!(document.totals[0].format_with_options(options), "3.7037₸");
}
