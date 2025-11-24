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
