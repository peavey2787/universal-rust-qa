use super::*;
use crate::test_support::{cleanup, discover};

#[test]
fn logical_cyclomatic_and_cognitive_metrics_cover_control_flow() {
    let source = r#"
// comment
fn f(x: bool) {
    if x && true {
        for _ in 0..1 {
            while false {}
        }
    } else {
        match x {
            true => (),
            false => (),
        }
    }
}
"#;
    assert!(logical_loc(source) >= 3);
    assert!(cyclomatic(source) >= 6);
    assert_eq!(cyclomatic("if left || right {}"), 3);
    assert!(cognitive(source) >= 4);
    assert_eq!(logical_loc("// only\n\n"), 0);
}

#[test]
fn attribute_limits_override_global_cc_only_when_parseable() {
    let config = QaConfig::default();
    assert_eq!(effective_cc_limit_for_attributes(&[], &config), config.metrics.cyclomatic);
    assert_eq!(
        effective_cc_limit_for_attributes(&["qa_attr :: allow ( cc = 40 )".into()], &config),
        40
    );
    assert_eq!(
        effective_cc_limit_for_attributes(
            &["qa_attr :: allow ( cc = nope )".into(), "qa_attr :: allow ( cc = 17 )".into()],
            &config,
        ),
        17
    );
    assert_eq!(parse_limit("qa_attr :: allow(cc=21, loc=90)", "cc"), Some(21));
    assert_eq!(parse_limit("qa_attr :: allow(loc=90)", "cc"), None);
}

#[test]
fn findings_enforce_loc_cc_and_cognitive_thresholds() {
    let (root, source) =
        discover(&[("src/lib.rs", "fn complex(a: bool) { if a { if a { if a {} } } }\n")]);
    let function = source.functions.iter().find(|f| f.name == "complex").unwrap();
    let mut config = QaConfig::default();
    config.metrics.function_loc = 1;
    config.metrics.cyclomatic = 1;
    config.metrics.cognitive = 1;
    let found = findings(function, &config, 10, 4, 4);
    let ids = found.iter().map(|f| f.rule_id.as_str()).collect::<Vec<_>>();
    assert!(ids.contains(&"QA-METRIC-001"));
    assert!(ids.contains(&"QA-METRIC-002"));
    assert!(ids.contains(&"QA-SPRAWL-002"));
    cleanup(&root);
}

#[test]
fn metric_arithmetic_and_threshold_boundaries_are_exact() {
    assert_eq!(cyclomatic("fn f() {}"), 1);
    assert_eq!(cyclomatic("if ready {}"), 2);
    assert_eq!(cyclomatic("if left && right {}"), 3);
    assert_eq!(cyclomatic("match x {\nA => 1,\nB => 2,\n}"), 3);

    assert_eq!(cognitive("if ready {\n}\n"), 1);
    assert_eq!(cognitive("if ready {\nwhile more {\n}\n}\n"), 3);

    let (root, source) = discover(&[("src/lib.rs", "fn exact() {}\n")]);
    let function = source.functions.iter().find(|function| function.name == "exact").unwrap();
    let config = QaConfig::default();
    assert!(
        findings(
            function,
            &config,
            config.metrics.function_loc,
            config.metrics.cyclomatic,
            config.metrics.cognitive,
        )
        .is_empty()
    );
    let over_loc = findings(
        function,
        &config,
        config.metrics.function_loc + 1,
        config.metrics.cyclomatic,
        config.metrics.cognitive,
    );
    assert_eq!(over_loc.iter().filter(|finding| finding.rule_id == "QA-SPRAWL-002").count(), 1);
    let over_cc = findings(
        function,
        &config,
        config.metrics.function_loc,
        config.metrics.cyclomatic + 1,
        config.metrics.cognitive,
    );
    assert_eq!(over_cc.iter().filter(|finding| finding.rule_id == "QA-METRIC-001").count(), 1);
    let over_cognitive = findings(
        function,
        &config,
        config.metrics.function_loc,
        config.metrics.cyclomatic,
        config.metrics.cognitive + 1,
    );
    assert_eq!(
        over_cognitive.iter().filter(|finding| finding.rule_id == "QA-METRIC-002").count(),
        1
    );
    cleanup(&root);
}

#[test]
fn attribute_limit_requires_both_allow_marker_and_requested_key() {
    let config = QaConfig::default();
    assert_eq!(
        effective_cc_limit_for_attributes(&["qa_attr :: something ( cc = 99 )".into()], &config),
        config.metrics.cyclomatic
    );
    assert_eq!(
        effective_cc_limit_for_attributes(&["qa_attr :: allow ( loc = 99 )".into()], &config),
        config.metrics.cyclomatic
    );
    assert_eq!(
        effective_cc_limit_for_attributes(&["qa_attr :: allow ( cc = 99 )".into()], &config),
        99
    );
}
