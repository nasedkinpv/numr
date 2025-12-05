//! Test that the example.numr file evaluates correctly
//!
//! This test uses a real-world example file to drive development (TDD).
//! Tests define EXPECTED behavior - features that don't work yet are in KNOWN_ISSUES.

use numr_core::{decimal as d, Engine, Value};

const EXAMPLE_FILE: &str = include_str!("../../../example.numr");

/// Known issues / features to implement
/// These lines from example.numr currently don't produce correct results
const KNOWN_ISSUES: &[&str] = &[
    // All known issues resolved!
];

fn create_engine() -> Engine {
    let mut engine = Engine::new();
    // Set exchange rates for testing (normally fetched from API)
    engine.set_exchange_rate(
        numr_core::Currency::USD,
        numr_core::Currency::ILS,
        d("3.65"),
    );
    engine.set_exchange_rate(
        numr_core::Currency::USD,
        numr_core::Currency::EUR,
        d("0.92"),
    );
    engine.set_exchange_rate(numr_core::Currency::USD, numr_core::Currency::RUB, d("92"));
    engine.set_exchange_rate(
        numr_core::Currency::BTC,
        numr_core::Currency::USD,
        d("60000"),
    );
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
                && !trimmed.starts_with("//")
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
    // total_usd = checking(4250) + savings(12800) = 17050
    let total_usd = engine.eval("total_usd");
    assert!(
        total_usd.as_decimal().unwrap() > d("15000"),
        "total_usd should be > 15000, got {total_usd:?}"
    );

    let total_expenses = engine.eval("total_expenses");
    assert!(
        total_expenses.as_decimal().unwrap() > d("2000"),
        "total_expenses should be > 2000, got {total_expenses:?}"
    );

    let week_hours = engine.eval("week_hours");
    assert!(
        (week_hours.as_decimal().unwrap() - d("35")).abs() < d("0.1"),
        "week_hours should be 35, got {week_hours:?}"
    );

    let liquid = engine.eval("liquid");
    assert!(
        liquid.as_decimal().unwrap() > d("50000"),
        "liquid should be > 50000, got {liquid:?}"
    );
}

#[test]
fn test_example_file_comma_separated_values() {
    let mut engine = create_engine();

    for line in EXAMPLE_FILE.lines() {
        if !is_known_issue(line) {
            engine.eval(line);
        }
    }

    // Verify comma-separated numbers are parsed correctly from example.numr
    // Only some values use commas (like copy-pasted from bank statements)

    // Bank account with comma (copy-pasted from bank): $4,250
    let checking = engine.eval("checking");
    assert_eq!(checking.as_decimal(), Some(d("4250")));
    assert_eq!(checking.to_string(), "$4250.00");

    // ILS account with comma (copy-pasted): ₪45,000
    let ils_account = engine.eval("ils_account");
    assert_eq!(ils_account.as_decimal(), Some(d("45000")));
    assert_eq!(ils_account.to_string(), "₪45000.00");

    // Groceries with comma (copy-pasted from receipt): ₪4,200
    let groceries = engine.eval("groceries");
    assert_eq!(groceries.as_decimal(), Some(d("4200")));
    assert_eq!(groceries.to_string(), "₪4200.00");

    // Marathon distance with decimal: 42.195 km
    let marathon = engine.eval("marathon");
    assert_eq!(marathon.as_decimal(), Some(d("42.195")));
    assert!(marathon.to_string().contains("km"));
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
    // checking(4250) + savings(12800) = 17050
    let total_usd = engine.eval("total_usd").as_decimal().unwrap();
    assert!(
        (total_usd - d("17050")).abs() < d("1"),
        "total_usd = {total_usd}"
    );

    // === ILS account ===
    // 45000 ILS in USD = 45000 / 3.65 ≈ 12328.77
    let ils_in_usd = engine.eval("ils_in_usd").as_decimal().unwrap();
    assert!(
        (ils_in_usd - d("12328.77")).abs() < d("1"),
        "ils_in_usd = {ils_in_usd}"
    );

    // === Liquid assets ===
    // total_usd(17050) + ils_in_usd(12328.77) + btc_wallet(0.42 * 60000 = 25200)
    // = 17050 + 12328.77 + 25200 = 54578.77
    let liquid = engine.eval("liquid").as_decimal().unwrap();
    assert!(
        (liquid - d("54578.77")).abs() < d("10"),
        "liquid = {liquid}"
    );

    // === Net worth ===
    // liquid + stocks_vanguard(28400 eur -> usd) - debt_alex(3500 rub -> usd)
    // 54578.77 + (28400/0.92) - (3500/92) ≈ 54578.77 + 30869.57 - 38.04 ≈ 85410
    let net_worth = engine.eval("net_worth").as_decimal().unwrap();
    assert!(
        (net_worth - d("85410")).abs() < d("50"),
        "net_worth = {net_worth}"
    );

    // === Time tracking ===
    // week_hours: 6.5h + 8h + 7.5h + 8h + 5h = 35h
    let week_hours = engine.eval("week_hours").as_decimal().unwrap();
    assert!(
        (week_hours - d("35")).abs() < d("0.01"),
        "week_hours = {week_hours}"
    );

    // overtime: 35h - 40h = -5h
    let overtime = engine.eval("overtime").as_decimal().unwrap();
    assert!(
        (overtime - d("-5")).abs() < d("0.01"),
        "overtime = {overtime}"
    );

    // === Debts ===
    // net_debt: 3500 rub - 120 usd (converts to first currency RUB)
    // = 3500 - (120 * 92) = 3500 - 11040 = -7540 rub
    let net_debt = engine.eval("net_debt").as_decimal().unwrap();
    assert!(
        (net_debt - d("-7540")).abs() < d("1"),
        "net_debt = {net_debt}"
    );

    // === Expenses ===
    // monthly_fixed: 1850 usd + 900 ils + 90 ils + 70 ils + 50 usd
    // = 1900 USD + 1060 ILS = 1900 + (1060/3.65) ≈ 2190.41 USD
    let monthly_fixed = engine.eval("monthly_fixed").as_decimal().unwrap();
    assert!(
        (monthly_fixed - d("2190.41")).abs() < d("1"),
        "monthly_fixed = {monthly_fixed}"
    );

    // total_expenses ≈ 3430.1 USD
    let total_expenses = engine.eval("total_expenses").as_decimal().unwrap();
    assert!(
        (total_expenses - d("3430.1")).abs() < d("1"),
        "total_expenses = {total_expenses}"
    );

    // === Runway ===
    // (liquid / total_expenses) in months = 54578.77 / 3430.1 ≈ 15.91 months
    let runway = engine.eval("runway").as_decimal().unwrap();
    assert!((runway - d("15.91")).abs() < d("0.5"), "runway = {runway}");

    // === Freelance ===
    // techcorp: 45h * $85 = $3825 gross, - 25% tax = $2868.75 net
    let techcorp_net = engine.eval("techcorp_net").as_decimal().unwrap();
    assert!(
        (techcorp_net - d("2868.75")).abs() < d("0.01"),
        "techcorp_net = {techcorp_net}"
    );

    // startup_total: 2200 + 400 = 2600 USD
    let startup_total = engine.eval("startup_total").as_decimal().unwrap();
    assert!(
        (startup_total - d("2600")).abs() < d("0.01"),
        "startup_total = {startup_total}"
    );

    // monthly_income: techcorp_net + startup_total + saas_mrr = 2868.75 + 2600 + 340 = 5808.75
    let monthly_income = engine.eval("monthly_income").as_decimal().unwrap();
    assert!(
        (monthly_income - d("5808.75")).abs() < d("0.01"),
        "monthly_income = {monthly_income}"
    );

    // saas_annual: 340 * 12 = 4080
    let saas_annual = engine.eval("saas_annual").as_decimal().unwrap();
    assert!(
        (saas_annual - d("4080")).abs() < d("0.01"),
        "saas_annual = {saas_annual}"
    );

    // hosting_annual: (127 + 48) * 12 = 175 * 12 = 2100
    let hosting_annual = engine.eval("hosting_annual").as_decimal().unwrap();
    assert!(
        (hosting_annual - d("2100")).abs() < d("0.01"),
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

    assert_eq!(result.as_decimal(), Some(d("3825")));
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

    assert_eq!(result.as_decimal(), Some(d("4080")));
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

    let miles = result.as_decimal().unwrap();
    assert!(
        (miles - d("5903.4")).abs() < d("1"),
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
    assert_eq!(runway.as_decimal(), Some(d("10")));

    // Also works with inline calculation
    let result = engine.eval("(500 usd / 50 usd) in months");
    assert!(
        !result.is_error(),
        "Inline ratio to unit should work: {result}"
    );
    assert_eq!(result.as_decimal(), Some(d("10")));
}

// ============================================================================
// MIXED UNIT ARITHMETIC TESTS
// ============================================================================

#[test]
fn test_mixed_unit_arithmetic() {
    let mut engine = create_engine();

    // Mixed unit arithmetic: 1 km + 500 m = 1.5 km
    let result = engine.eval("1 km + 500 m");
    assert!(!result.is_error(), "Should add compatible units: {result}");
    assert_eq!(result.as_decimal(), Some(d("1.5")));
}

#[test]
fn test_percentage_increase_decrease() {
    let mut engine = create_engine();

    // Percentage increase: 100 + 10% = 110
    let result = engine.eval("100 + 10%");
    assert_eq!(result.as_decimal(), Some(d("110")));

    // Percentage decrease: 100 - 20% = 80
    let result = engine.eval("100 - 20%");
    assert_eq!(result.as_decimal(), Some(d("80")));
}

// ============================================================================
// COMMA-SEPARATED NUMBER TESTS
// ============================================================================

#[test]
fn test_comma_separated_numbers() {
    let mut engine = create_engine();

    // Comma-separated numbers: $4,250 should parse as $4250
    let result = engine.eval("$4,250");
    assert_eq!(result.as_decimal(), Some(d("4250")));
    assert_eq!(result.to_string(), "$4250.00");

    // Larger numbers
    let result = engine.eval("1,234,567");
    assert_eq!(result.as_decimal(), Some(d("1234567")));

    // Arithmetic with comma-separated numbers
    let result = engine.eval("$1,000 + $2,500");
    assert_eq!(result.as_decimal(), Some(d("3500")));
    assert_eq!(result.to_string(), "$3500.00");

    // With decimals
    let result = engine.eval("$1,234.56");
    assert_eq!(result.as_decimal(), Some(d("1234.56")));
    assert_eq!(result.to_string(), "$1234.56");
}
