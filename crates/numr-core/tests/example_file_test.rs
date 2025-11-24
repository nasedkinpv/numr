//! Test that the example.numr file evaluates correctly
//!
//! This test uses a real-world example file to drive development (TDD).
//! Tests define EXPECTED behavior - features that don't work yet are in KNOWN_ISSUES.

use numr_core::{Engine, Value};

const EXAMPLE_FILE: &str = include_str!("../../../example.numr");

/// Known issues / features to implement
/// These lines from example.numr currently don't produce correct results
const KNOWN_ISSUES: &[&str] = &[
    // All known issues resolved!
];

fn create_engine() -> Engine {
    let mut engine = Engine::new();
    // Set exchange rates for testing (normally fetched from API)
    engine.set_exchange_rate(numr_core::Currency::USD, numr_core::Currency::ILS, 3.65);
    engine.set_exchange_rate(numr_core::Currency::USD, numr_core::Currency::EUR, 0.92);
    engine.set_exchange_rate(numr_core::Currency::USD, numr_core::Currency::RUB, 92.0);
    engine.set_exchange_rate(numr_core::Currency::BTC, numr_core::Currency::USD, 60000.0);
    engine
}

fn is_known_issue(line: &str) -> bool {
    let trimmed = line.trim();
    KNOWN_ISSUES.contains(&trimmed)
}

// ============================================================================
// EXAMPLE FILE INTEGRATION TESTS
// ============================================================================

#[test]
fn test_example_file_parses_without_errors() {
    let mut engine = create_engine();
    let mut errors = Vec::new();

    for (line_num, line) in EXAMPLE_FILE.lines().enumerate() {
        if is_known_issue(line) {
            continue;
        }

        let result = engine.eval(line);
        if let Value::Error(msg) = result {
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

    for line in EXAMPLE_FILE.lines() {
        if !is_known_issue(line) {
            engine.eval(line);
        }
    }

    // Key variables should be defined with reasonable values
    let total_usd = engine.eval("total_usd");
    assert!(
        total_usd.as_f64().unwrap() > 20000.0,
        "total_usd should be > 20000, got {total_usd:?}"
    );

    let total_expenses = engine.eval("total_expenses");
    assert!(
        total_expenses.as_f64().unwrap() > 2000.0,
        "total_expenses should be > 2000, got {total_expenses:?}"
    );

    let week_hours = engine.eval("week_hours");
    assert!(
        (week_hours.as_f64().unwrap() - 35.0).abs() < 0.1,
        "week_hours should be 35, got {week_hours:?}"
    );

    let liquid = engine.eval("liquid");
    assert!(
        liquid.as_f64().unwrap() > 50000.0,
        "liquid should be > 50000, got {liquid:?}"
    );
}

#[test]
fn test_example_file_math_correctness() {
    let mut engine = create_engine();

    for line in EXAMPLE_FILE.lines() {
        if !is_known_issue(line) {
            engine.eval(line);
        }
    }

    // === Bank accounts ===
    // checking(4250) + savings(12800) + emergency_fund(8500 converted ils->usd)
    let total_usd = engine.eval("total_usd").as_f64().unwrap();
    assert!((total_usd - 25550.0).abs() < 1.0, "total_usd = {total_usd}");

    // === ILS account ===
    // 45000 ILS in USD = 45000 / 3.65 ≈ 12328.77
    let ils_in_usd = engine.eval("ils_in_usd").as_f64().unwrap();
    assert!(
        (ils_in_usd - 12328.77).abs() < 1.0,
        "ils_in_usd = {ils_in_usd}"
    );

    // === Liquid assets ===
    // total_usd(25550) + ils_in_usd(12328.77) + btc_wallet(0.42 * 60000 = 25200)
    // = 25550 + 12328.77 + 25200 = 63078.77
    let liquid = engine.eval("liquid").as_f64().unwrap();
    assert!((liquid - 63078.77).abs() < 10.0, "liquid = {liquid}");

    // === Net worth ===
    // liquid + stocks_vanguard(28400 eur -> usd) - debt_alex(3500 rub -> usd)
    // 63078.77 + (28400/0.92) - (3500/92) ≈ 63078.77 + 30869.57 - 38.04 ≈ 93910
    let net_worth = engine.eval("net_worth").as_f64().unwrap();
    assert!(
        (net_worth - 93910.0).abs() < 50.0,
        "net_worth = {net_worth}"
    );

    // === Time tracking ===
    // week_hours: 6.5h + 8h + 7.5h + 8h + 5h = 35h
    let week_hours = engine.eval("week_hours").as_f64().unwrap();
    assert!(
        (week_hours - 35.0).abs() < 0.01,
        "week_hours = {week_hours}"
    );

    // overtime: (35h - 40h) in min = -5h = -300 min
    let overtime = engine.eval("overtime").as_f64().unwrap();
    assert!((overtime - (-300.0)).abs() < 0.01, "overtime = {overtime}");

    // === Debts ===
    // net_debt: 3500 rub - 120 usd (converts to first currency RUB)
    // = 3500 - (120 * 92) = 3500 - 11040 = -7540 rub
    let net_debt = engine.eval("net_debt").as_f64().unwrap();
    assert!((net_debt - (-7540.0)).abs() < 1.0, "net_debt = {net_debt}");

    // === Expenses ===
    // monthly_fixed: 1850 usd + 900 ils + 90 ils + 70 ils + 50 usd
    // = 1900 USD + 1060 ILS = 1900 + (1060/3.65) ≈ 2190.41 USD
    let monthly_fixed = engine.eval("monthly_fixed").as_f64().unwrap();
    assert!(
        (monthly_fixed - 2190.41).abs() < 1.0,
        "monthly_fixed = {monthly_fixed}"
    );

    // total_expenses ≈ 3430.1 USD
    let total_expenses = engine.eval("total_expenses").as_f64().unwrap();
    assert!(
        (total_expenses - 3430.1).abs() < 1.0,
        "total_expenses = {total_expenses}"
    );

    // === Runway ===
    // (liquid / total_expenses) in months = 63078.77 / 3430.1 ≈ 18.39 months
    let runway = engine.eval("runway").as_f64().unwrap();
    assert!((runway - 18.39).abs() < 0.5, "runway = {runway}");

    // === Freelance ===
    // techcorp: 45h * $85 = $3825 gross, 25% tax = $956.25, net = $2868.75
    let techcorp_gross = engine.eval("techcorp_gross").as_f64().unwrap();
    assert!(
        (techcorp_gross - 3825.0).abs() < 0.01,
        "techcorp_gross = {techcorp_gross}"
    );

    let techcorp_net = engine.eval("techcorp_net").as_f64().unwrap();
    assert!(
        (techcorp_net - 2868.75).abs() < 0.01,
        "techcorp_net = {techcorp_net}"
    );

    // startup_total: 2200 + 400 = 2600 USD
    let startup_total = engine.eval("startup_total").as_f64().unwrap();
    assert!(
        (startup_total - 2600.0).abs() < 0.01,
        "startup_total = {startup_total}"
    );

    // monthly_income: techcorp_net + startup_total + saas_mrr = 2868.75 + 2600 + 340 = 5808.75
    let monthly_income = engine.eval("monthly_income").as_f64().unwrap();
    assert!(
        (monthly_income - 5808.75).abs() < 0.01,
        "monthly_income = {monthly_income}"
    );

    // saas_annual: 340 * 12 = 4080
    let saas_annual = engine.eval("saas_annual").as_f64().unwrap();
    assert!(
        (saas_annual - 4080.0).abs() < 0.01,
        "saas_annual = {saas_annual}"
    );

    // hosting_annual: (127 + 48) * 12 = 175 * 12 = 2100
    let hosting_annual = engine.eval("hosting_annual").as_f64().unwrap();
    assert!(
        (hosting_annual - 2100.0).abs() < 0.01,
        "hosting_annual = {hosting_annual}"
    );
}

// ============================================================================
// UNIT MULTIPLICATION TESTS
// ============================================================================

/// hours * currency_rate should produce currency, not hours
#[test]
fn test_unit_times_currency() {
    let mut engine = create_engine();

    // 45 hours * $85/hour = $3825 (not "3825 h")
    let result = engine.eval("45h * 85 usd");
    assert!(!result.is_error(), "Should not error: {result}");

    // Result should be currency
    let formatted = result.to_string();
    assert!(
        formatted.contains('$') || formatted.to_lowercase().contains("usd"),
        "45h * 85 usd should produce USD, got: {formatted}"
    );

    assert!(
        (result.as_f64().unwrap() - 3825.0).abs() < 0.01,
        "45h * 85 usd = {result}"
    );
}

/// Multiplying currency by "N months" should just multiply the numbers
#[test]
fn test_currency_times_months() {
    let mut engine = create_engine();

    // $340 * 12 months = $4080
    let result = engine.eval("340 usd * 12 months");
    assert!(
        !result.is_error(),
        "340 usd * 12 months should work: {result}"
    );

    assert!(
        (result.as_f64().unwrap() - 4080.0).abs() < 0.01,
        "340 usd * 12 months = {result}"
    );
}

// ============================================================================
// UNIT CONVERSION TESTS
// ============================================================================

/// Unit conversion from variable (e.g., "flight_km in miles")
#[test]
fn test_variable_unit_conversion() {
    let mut engine = create_engine();

    engine.eval("flight_km = 9500 km");

    // 9500 km ≈ 5903.4 miles
    let result = engine.eval("flight_km in miles");
    assert!(
        !result.is_error(),
        "Unit conversion from variable should work: {result}"
    );

    let miles = result.as_f64().unwrap();
    assert!(
        (miles - 5903.4).abs() < 1.0,
        "9500 km should be ~5903 miles, got {miles}"
    );
}

/// Currency ratio to unit conversion (e.g., "(usd / usd) in months")
#[test]
fn test_ratio_to_unit_conversion() {
    let mut engine = create_engine();

    // Currency ratio to unit (runway calculation pattern)
    engine.eval("liquid = 1000 usd");
    engine.eval("expenses = 100 usd");
    let runway = engine.eval("(liquid / expenses) in months");
    assert!(
        !runway.is_error(),
        "Currency ratio to unit should work: {runway}"
    );
    assert!(
        (runway.as_f64().unwrap() - 10.0).abs() < 0.01,
        "runway = {runway}"
    );

    // Also works with inline calculation
    let result = engine.eval("(500 usd / 50 usd) in months");
    assert!(
        !result.is_error(),
        "Inline ratio to unit should work: {result}"
    );
    assert!(
        (result.as_f64().unwrap() - 10.0).abs() < 0.01,
        "result = {result}"
    );
}

// ============================================================================
// FUTURE EDGE CASES (ignored - TDD targets)
// ============================================================================

#[test]
#[ignore = "Edge cases for future development"]
fn test_future_edge_cases() {
    let mut engine = create_engine();

    // Comma-separated numbers: $4,250
    let result = engine.eval("$4,250");
    assert!(!result.is_error(), "Should parse comma-separated numbers");

    // Mixed unit arithmetic: 1 km + 500 m = 1.5 km or 1500 m
    let result = engine.eval("1 km + 500 m");
    assert!(!result.is_error(), "Should add compatible units");

    // Percentage increase: 100 + 10% = 110
    let result = engine.eval("100 + 10%");
    assert_eq!(result.as_f64().unwrap(), 110.0);

    // Percentage decrease: 100 - 20% = 80
    let result = engine.eval("100 - 20%");
    assert_eq!(result.as_f64().unwrap(), 80.0);
}
