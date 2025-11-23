//! Test that the example.numr file evaluates correctly
//!
//! This test uses a real-world example file to drive development.
//! Features that don't work yet are tracked in `test_known_issues`.

use numr_core::{Engine, Value};

const EXAMPLE_FILE: &str = include_str!("../../../example.numr");

/// Known issues / features to implement
/// These lines from example.numr currently fail but should work
const KNOWN_ISSUES: &[&str] = &[
    // Unit conversion from variable doesn't work yet
    "flight_miles = flight_km in miles",
];

fn create_engine() -> Engine {
    let mut engine = Engine::new();
    // Set exchange rates for testing (normally fetched from API)
    engine.set_exchange_rate(numr_core::Currency::USD, numr_core::Currency::ILS, 3.7);
    engine.set_exchange_rate(numr_core::Currency::USD, numr_core::Currency::EUR, 0.92);
    engine.set_exchange_rate(numr_core::Currency::BTC, numr_core::Currency::USD, 95000.0);
    engine
}

fn is_known_issue(line: &str) -> bool {
    let trimmed = line.trim();
    KNOWN_ISSUES.iter().any(|&issue| trimmed == issue)
}

#[test]
fn test_example_file_parses_without_errors() {
    let mut engine = create_engine();
    let mut errors = Vec::new();

    for (line_num, line) in EXAMPLE_FILE.lines().enumerate() {
        // Skip known issues
        if is_known_issue(line) {
            continue;
        }

        let result = engine.eval(line);
        if let Value::Error(msg) = result {
            // Skip lines that are just prose text (no operators or numbers)
            let trimmed = line.trim();
            if !trimmed.is_empty()
                && !trimmed.starts_with('#')
                && (trimmed.contains('=') || trimmed.chars().any(|c| c.is_ascii_digit()))
            {
                errors.push(format!("Line {}: '{}' -> {}", line_num + 1, line, msg));
            }
        }
    }

    if !errors.is_empty() {
        panic!("Example file has errors:\n{}", errors.join("\n"));
    }
}

#[test]
fn test_example_file_variables_resolve() {
    let mut engine = create_engine();

    // Evaluate all lines (skip known issues)
    for line in EXAMPLE_FILE.lines() {
        if !is_known_issue(line) {
            engine.eval(line);
        }
    }

    // Check key variables are defined and have reasonable values
    let total_usd = engine.eval("total_usd");
    assert!(
        total_usd.as_f64().unwrap() > 20000.0,
        "total_usd should be > 20000"
    );

    let total_expenses = engine.eval("total_expenses");
    assert!(
        total_expenses.as_f64().unwrap() > 2000.0,
        "total_expenses should be > 2000"
    );

    let week_hours = engine.eval("week_hours");
    assert!(
        (week_hours.as_f64().unwrap() - 35.0).abs() < 0.1,
        "week_hours should be 35"
    );

    let runway = engine.eval("runway");
    assert!(runway.as_f64().unwrap() > 1.0, "runway should be > 1 month");
}

#[test]
fn test_example_file_math_correctness() {
    let mut engine = create_engine();

    // Evaluate all lines (skip known issues)
    for line in EXAMPLE_FILE.lines() {
        if !is_known_issue(line) {
            engine.eval(line);
        }
    }

    // Verify specific calculations

    // checking + savings + emergency_fund = 4250 + 12800 + 8500 = 25550
    let total_usd = engine.eval("total_usd").as_f64().unwrap();
    assert!(
        (total_usd - 25550.0).abs() < 0.01,
        "total_usd = {}",
        total_usd
    );

    // techcorp: 45 * 85 = 3825 gross, 25% tax = 956.25, net = 2868.75
    let techcorp_net = engine.eval("techcorp_net").as_f64().unwrap();
    assert!(
        (techcorp_net - 2868.75).abs() < 0.01,
        "techcorp_net = {}",
        techcorp_net
    );

    // week_hours: 6.5 + 8 + 7.5 + 8 + 5 = 35
    let week_hours = engine.eval("week_hours").as_f64().unwrap();
    assert!(
        (week_hours - 35.0).abs() < 0.01,
        "week_hours = {}",
        week_hours
    );

    // overtime: 35 - 40 = -5
    let overtime = engine.eval("overtime").as_f64().unwrap();
    assert!((overtime - (-5.0)).abs() < 0.01, "overtime = {}", overtime);

    // net_debt: 350 - 120 = 230
    let net_debt = engine.eval("net_debt").as_f64().unwrap();
    assert!((net_debt - 230.0).abs() < 0.01, "net_debt = {}", net_debt);

    // monthly_fixed: 1850 + 180 + 65 + 45 + 50 = 2190
    let monthly_fixed = engine.eval("monthly_fixed").as_f64().unwrap();
    assert!(
        (monthly_fixed - 2190.0).abs() < 0.01,
        "monthly_fixed = {}",
        monthly_fixed
    );

    // total_expenses: 2190 + (420 + 89) = 2699
    let total_expenses = engine.eval("total_expenses").as_f64().unwrap();
    assert!(
        (total_expenses - 2699.0).abs() < 0.01,
        "total_expenses = {}",
        total_expenses
    );
}

/// Test known issues - these should fail until features are implemented
/// Run with: cargo test known_issues -- --ignored
#[test]
#[ignore = "Unit conversion from variable not yet implemented"]
fn test_known_issues_unit_conversion_from_variable() {
    let mut engine = create_engine();

    // First set up the variable
    engine.eval("flight_km = 9500 km");

    // This should convert the variable's unit value to miles
    // 9500 km ≈ 5903.4 miles
    let result = engine.eval("flight_km in miles");
    assert!(
        !result.is_error(),
        "Unit conversion from variable should work: {}",
        result
    );

    let miles = result.as_f64().unwrap();
    assert!(
        (miles - 5903.4).abs() < 1.0,
        "9500 km should be ~5903 miles, got {}",
        miles
    );
}

/// Edge cases we want to support
#[test]
#[ignore = "Edge cases for future development"]
fn test_edge_cases_to_implement() {
    let mut engine = create_engine();

    // Comma-separated numbers: $4,250
    let result = engine.eval("$4,250");
    assert!(!result.is_error(), "Should parse comma-separated numbers");

    // Currency symbol after number with space: 100 $
    let result = engine.eval("100 $");
    assert_eq!(result.to_string(), "$100");

    // Mixed unit arithmetic: 1 km + 500 m
    let result = engine.eval("1 km + 500 m");
    assert!(!result.is_error(), "Should add compatible units");

    // Percentage increase: 100 + 10%
    let result = engine.eval("100 + 10%");
    assert_eq!(result.as_f64().unwrap(), 110.0);

    // Negative percentage: 100 - 20%
    let result = engine.eval("100 - 20%");
    assert_eq!(result.as_f64().unwrap(), 80.0);
}
