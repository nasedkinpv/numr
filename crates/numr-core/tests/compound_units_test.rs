//! Test that the compound_units.numr file evaluates correctly
//!
//! This test validates compound unit operations (m², km/h, etc.)

use numr_core::{Engine, Value};

const COMPOUND_UNITS_FILE: &str = include_str!("../../../compound_units.numr");

fn create_engine() -> Engine {
    Engine::new()
}

#[test]
fn test_compound_units_file_parses_without_errors() {
    let mut engine = create_engine();
    let mut errors = Vec::new();

    for (line_num, line) in COMPOUND_UNITS_FILE.lines().enumerate() {
        let result = engine.eval(line);
        if let Value::Error(msg) = result {
            let trimmed = line.trim();
            if !trimmed.is_empty() && !trimmed.starts_with('#') && !trimmed.starts_with("//") {
                errors.push(format!(
                    "Line {}: '{}' -> Error: {}",
                    line_num + 1,
                    line,
                    msg
                ));
            }
        }
    }

    if !errors.is_empty() {
        panic!(
            "compound_units.numr has {} errors:\n{}",
            errors.len(),
            errors.join("\n")
        );
    }
}

#[test]
fn test_area_calculations() {
    let mut engine = create_engine();

    // 5 m * 10 m = 50 m²
    engine.eval("length = 5 m");
    engine.eval("width = 10 m");
    let result = engine.eval("area = length * width");
    assert_eq!(result.to_string(), "50 m²");
}

#[test]
fn test_speed_calculations() {
    let mut engine = create_engine();

    // 100 km / 2 h = 50 km/h
    engine.eval("distance = 100 km");
    engine.eval("time = 2 h");
    let result = engine.eval("speed = distance / time");
    assert_eq!(result.to_string(), "50 km/h");
}

#[test]
fn test_dimensionless_result() {
    let mut engine = create_engine();

    // 25 km / 100 km = 0.25 (plain number)
    let result = engine.eval("25 km / 100 km");
    assert!(matches!(result, Value::Number(_)));
    assert_eq!(result.as_f64(), Some(0.25));
}

#[test]
fn test_compound_unit_parsing() {
    let mut engine = create_engine();

    // Direct input of compound units
    let result = engine.eval("50 kph");
    assert_eq!(result.to_string(), "50 km/h");

    let result = engine.eval("100 m2");
    assert_eq!(result.to_string(), "100 m²");

    let result = engine.eval("10 mps");
    assert_eq!(result.to_string(), "10 m/s");
}

#[test]
fn test_compound_unit_conversion() {
    let mut engine = create_engine();

    // 50 km/h in m/s ≈ 13.89 m/s
    let result = engine.eval("50 kph in mps");
    let val = result.as_f64().unwrap();
    assert!((val - 13.89).abs() < 0.01);
}

#[test]
fn test_compound_unit_addition() {
    let mut engine = create_engine();

    // 12 m² + 15 m² = 27 m²
    engine.eval("wall1 = 4 m * 3 m");
    engine.eval("wall2 = 5 m * 3 m");
    let result = engine.eval("total = wall1 + wall2");
    assert_eq!(result.to_string(), "27 m²");
}

#[test]
fn test_speed_times_time_gives_distance() {
    let mut engine = create_engine();

    // 50 km/h * 2 h = 100 km
    let result = engine.eval("50 kph * 2 h");
    assert_eq!(result.to_string(), "100 km");
}

#[test]
fn test_mixed_speed_units() {
    let mut engine = create_engine();

    // 100 km/h + 30 mph (converts to km/h)
    engine.eval("car = 100 kph");
    engine.eval("bike = 30 mph");
    let result = engine.eval("total = car + bike");
    // 30 mph ≈ 48.28 km/h, total ≈ 148.28 km/h
    let val = result.as_f64().unwrap();
    assert!((val - 148.28).abs() < 0.1);
}

#[test]
fn test_compound_unit_totals() {
    let mut engine = create_engine();

    engine.eval("room1 = 20 m2");
    engine.eval("room2 = 15 m2");
    engine.eval("room3 = 25 m2");

    let totals = engine.grouped_totals();
    assert_eq!(totals.len(), 1);
    assert_eq!(totals[0].to_string(), "60 m²");
}
