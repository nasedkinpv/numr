use numr_core::{decimal, Engine, Value};

fn number(value: Value, expected: &str) {
    assert_eq!(value.as_decimal(), Some(decimal(expected)), "{value:?}");
}

#[test]
fn continuation_commits_the_selected_source_across_empty_and_failed_lines() {
    let mut engine = Engine::new();
    engine.evaluate_document("$10\n# comment\n1 / 0\n\n// comment");
    let before = engine.lines().to_vec();
    assert_eq!(engine.eval_preview("+ $5").to_string(), "$15.00");
    assert_eq!(engine.lines(), before);
    assert!(engine.eval("ANS / 0").is_error());
    assert_eq!(engine.lines()[..before.len()], before);

    let appended = engine.append_lines(["+ $5", "# trailing comment"]);
    assert_eq!(appended[0].value.to_string(), "$15.00");
    assert!(engine.lines()[0].is_continuation_source);
    assert!(engine.lines()[1..]
        .iter()
        .all(|line| !line.is_continuation_source));
    number(engine.sum(), "15");
    assert_eq!(engine.grouped_totals(), vec![appended[0].value.clone()]);
}

#[test]
fn aggregate_can_be_an_answer_source_without_becoming_a_contribution() {
    let mut engine = Engine::new();
    engine.evaluate_document("10\n20\ntotal\n# comment\n1 / 0");
    number(engine.eval_preview("+ 5"), "35");
    assert!(!engine.lines()[2].is_continuation_source);
    number(engine.eval("+ 5"), "35");
    assert!(engine.lines()[2].is_aggregate);
    assert!(engine.lines()[2].is_continuation_source);
    assert!(!engine.lines()[1].is_continuation_source);
    number(engine.sum(), "65");
}

#[test]
fn variables_are_sorted_and_hide_implicit_inputs_at_the_core_boundary() {
    let mut engine = Engine::new();
    let document = engine.evaluate_document("z = 1\na = 2\nm = 3\ntotal\nANS");
    let expected = vec![
        ("a".to_string(), Value::Number(decimal("2"))),
        ("m".to_string(), Value::Number(decimal("3"))),
        ("z".to_string(), Value::Number(decimal("1"))),
    ];
    assert_eq!(document.variables, expected);
    assert_eq!(engine.variables(), expected);
}

#[test]
fn comments_and_discarded_prose_do_not_consume_previous_values() {
    for input in [
        "200 # ANS _ ans total",
        "200 // ANS _ ans total",
        "(200 + 0) explanation ANS _ total",
        "ANS explanation: 200 + 0",
        "_ = 200",
        "ANS = 200",
        "ans = 200",
    ] {
        let mut engine = Engine::new();
        number(engine.eval("100"), "100");
        number(engine.eval(input), "200");
        assert!(!engine.lines()[0].is_continuation_source, "{input}");
        number(engine.eval("total"), "300");
    }
}

#[test]
fn bare_aggregate_ast_is_display_only_even_with_comments_or_parentheses() {
    for input in ["total # subtotal", "total // ANS", "(total)", " total "] {
        let mut engine = Engine::new();
        engine.eval("100");
        number(engine.eval(input), "100");
        assert!(engine.lines()[1].is_aggregate, "{input}");
        number(engine.eval("total"), "100");
        number(engine.sum(), "100");
    }
}

#[test]
fn total_assignments_and_expressions_remain_contributions() {
    for input in ["subtotal = total", "total + 0", "sum(total)", "total = 100"] {
        let mut engine = Engine::new();
        engine.eval("100");
        number(engine.eval(input), "100");
        assert!(!engine.lines()[1].is_aggregate, "{input}");
        number(engine.eval("total"), "200");
    }
}

#[test]
fn nested_answer_references_consume_the_previous_value() {
    for (input, expected) in [
        ("result = sum(ANS, 20)", "120"),
        ("sqrt(ans)", "10"),
        ("20% of _", "20"),
        ("(ANS + 10) * 2", "220"),
        ("ANS in hex", "100"),
    ] {
        let mut engine = Engine::new();
        engine.eval("100");
        number(engine.eval(input), expected);
        assert!(engine.lines()[0].is_continuation_source, "{input}");
        number(engine.sum(), expected);
    }
}

#[test]
fn failed_evaluations_and_continuations_leave_the_source_available() {
    for input in ["/ 0", "ANS / 0", "sum(ANS, missing)", "5 in ANS"] {
        let mut engine = Engine::new();
        engine.eval("100");
        assert!(engine.eval(input).is_error(), "{input}");
        assert!(!engine.lines()[0].is_continuation_source, "{input}");
        number(engine.sum(), "100");
        number(engine.eval("+ 20"), "120");
        assert!(engine.lines()[0].is_continuation_source, "{input}");
        number(engine.sum(), "120");
    }
}

#[test]
fn preview_matches_evaluation_and_preserves_all_document_state() {
    for input in [
        "total",
        "total # subtotal",
        "sum(total, ANS)",
        "20% of total",
        "result = total + ANS",
        "total = 999",
        "ANS = 999",
        "+ total",
        "/ 0",
        "200 # ANS",
        "ANS explanation: 200 + 0",
    ] {
        let mut engine = Engine::new();
        engine.evaluate_document("100\ntotal\n200"); // cached total is now stale
        let lines = engine.lines().to_vec();
        let variables = engine.variables();
        let sum = engine.sum();
        let preview = engine.eval_preview(input);
        assert_eq!(engine.lines(), lines, "{input}");
        assert_eq!(engine.variables(), variables, "{input}");
        assert_eq!(engine.sum(), sum, "{input}");
        number(engine.eval_preview("total"), "300");
        number(engine.eval_preview("ANS"), "200");
        assert_eq!(preview, engine.eval(input), "{input}");
    }
}

#[test]
fn reference_case_matches_case_sensitive_variable_evaluation() {
    let mut engine = Engine::new();
    engine.eval("Ans = 7");
    engine.eval("100");
    number(engine.eval("Ans"), "7");
    assert!(!engine.lines()[1].is_continuation_source);
    number(engine.sum(), "114");

    engine.clear();
    engine.eval("TOTAL = 7");
    engine.eval("100");
    number(engine.eval("TOTAL"), "7");
    assert!(!engine.lines()[2].is_aggregate);
    number(engine.sum(), "114");
    number(engine.eval("total"), "114");
}
