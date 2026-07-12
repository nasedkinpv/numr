use std::collections::HashMap;

use numr_core::{
    cache::RateCache, catalog, Currency, Decimal, Engine, EvalError, ParseError, Value,
};

#[test]
fn arithmetic_overflow_is_a_typed_value() {
    let mut engine = Engine::new();
    let result = engine.eval("79228162514264337593543950335 * 2");

    assert!(matches!(
        result.as_error(),
        Some(EvalError::Overflow { .. })
    ));
    assert!(matches!(
        engine.eval("factorial(100)").as_error(),
        Some(EvalError::Overflow { .. })
    ));
    assert!(matches!(
        engine
            .eval("79228162514264337593543950335 m * 2")
            .as_error(),
        Some(EvalError::Overflow { .. })
    ));
}

#[test]
fn parser_rejects_wide_and_deep_expressions_before_evaluation() {
    let wide = vec!["1"; 5_000].join(" + ");
    assert!(matches!(
        numr_core::parse_line(&wide),
        Err(ParseError::InputTooLong { .. } | ParseError::TooComplex { .. })
    ));

    let deep = format!("{}1{}", "(".repeat(129), ")".repeat(129));
    assert!(matches!(
        numr_core::parse_line(&deep),
        Err(ParseError::TooDeep { .. })
    ));
}

#[test]
fn failed_continuation_does_not_consume_its_source() {
    let mut engine = Engine::new();
    engine.eval("$100");
    assert!(engine.eval("_ + missing").is_error());

    assert!(!engine.lines()[0].is_continuation_source);
    assert_eq!(
        engine.grouped_totals()[0].as_decimal(),
        Some(Decimal::from(100))
    );
}

#[test]
fn aggregate_queries_are_idempotent() {
    let mut engine = Engine::new();
    engine.eval("10");
    engine.eval("20");

    assert_eq!(engine.eval("total").as_decimal(), Some(Decimal::from(30)));
    assert_eq!(engine.eval("total").as_decimal(), Some(Decimal::from(30)));
    assert_eq!(engine.sum().as_decimal(), Some(Decimal::from(30)));
}

#[test]
fn currency_and_dimensionless_unit_semantics_are_explicit() {
    let mut engine = Engine::new();

    assert_eq!(engine.eval("$100 / $50"), Value::Number(Decimal::from(2)));
    assert!(matches!(
        engine.eval("$2 * $3").as_error(),
        Some(EvalError::InvalidOperands(_))
    ));
    assert_eq!(
        engine.eval("1 km / 1 m"),
        Value::Number(Decimal::from(1_000))
    );
}

#[test]
fn engine_construction_is_pure_and_cache_io_is_explicit() {
    let _engine = Engine::new();

    let unique = format!(
        "numr-core-rates-{}-{}.json",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    );
    let path = std::env::temp_dir().join(unique);
    let mut rates = HashMap::new();
    rates.insert("EUR".to_string(), Decimal::new(92, 2));

    let cache = RateCache::default();
    cache.save_to_path(&path, &rates).unwrap();
    let mut loaded = RateCache::default();
    assert!(loaded.load_from_path(&path).unwrap());
    assert_eq!(
        loaded.get_rate(Currency::USD, Currency::EUR),
        Some(Decimal::new(92, 2))
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn shared_document_and_catalog_contracts_are_stable() {
    let mut engine = Engine::new();
    let document = engine.evaluate_document("b = 2\na = 1\na + b");

    assert_eq!(document.lines.len(), 3);
    assert_eq!(document.variables[0].0, "a");
    assert_eq!(document.variables[1].0, "b");
    assert!(catalog::BUILTIN_FUNCTIONS.contains(&"sqrt"));
    assert!(catalog::KEYWORDS.contains(&"to"));
    assert!(catalog::currency_catalog()
        .iter()
        .any(|currency| currency.code == "BTC" && currency.coingecko_id == Some("bitcoin")));
}
