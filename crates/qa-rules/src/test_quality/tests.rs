use super::*;
use crate::test_support::{cleanup, discover, ids};

#[test]
fn test_quality_distinguishes_assertions_tautologies_reachability_and_randomness() {
    let fixture = [
        "fn production(x:u8)->u8{x+1}\n#[test] fn no_assert(){ production(1); }\n",
        "#[test] fn tautology(){ production(1); assert!(",
        "true); }\n#[test] fn self_equal(){ production(1); assert_",
        "eq!(production(1), production(1)); }\n#[test] fn unreachable(){ assert_eq!(1,2); }\n",
        "#[test] fn random(){ production(1); let _=std::time::SystemTime::",
        "now(); assert_ne!(1,2); }\n#[test] fn good(){ assert_eq!(production(1),2); }\n",
    ]
    .concat();
    let (root, source) = discover(&[("src/lib.rs", fixture.as_str())]);
    let mut findings = Vec::new();
    let invalid = analyze(&source, &QaConfig::default(), &mut findings);
    assert_eq!(invalid, 5);
    let found = ids(&findings);
    assert!(found.contains(&"QA-TEST-001"));
    assert!(found.contains(&"QA-TEST-002"));
    assert!(found.contains(&"QA-TEST-003"));
    assert!(found.contains(&"QA-TEST-005"));
    cleanup(&root);
}

#[test]
fn explicit_test_kind_and_policy_switches_take_allow_paths() {
    let fixture = [
        "fn production(){}\n#[test] #[should_panic] fn explicit(){ production(); panic!(\"expected\"); }\n",
        "#[test] fn loose(){ assert!(",
        "true); let _=std::time::SystemTime::",
        "now(); }\n",
    ]
    .concat();
    let (root, source) = discover(&[("src/lib.rs", fixture.as_str())]);
    let mut config = QaConfig::default();
    config.tests.reject_tautological_assertions = false;
    config.tests.reject_unseeded_randomness = false;
    config.tests.require_production_reachability = false;
    let mut findings = Vec::new();
    let invalid = analyze(&source, &config, &mut findings);
    assert_eq!(invalid, 0);
    cleanup(&root);
}

#[test]
fn self_eq_parser_handles_spacing_and_non_assertions() {
    let equal = ["assert_", "eq!(value, value);"].concat();
    let compact_equal = ["  assert_", "eq!(x,x)"].concat();
    let different = ["assert_", "eq!(x,y);"].concat();
    let nested_equal = ["assert_", "eq!(production(1), production(1));"].concat();
    assert!(self_eq(&equal));
    assert!(self_eq(&compact_equal));
    assert!(self_eq(&nested_equal));
    assert!(!self_eq(&different));
    assert!(!self_eq("let x = 1;"));
}

#[test]
fn tautology_scan_ignores_assertion_text_inside_fixture_literals() {
    let source = r##"#[test]
fn fixture_holder() {
    let _fixture = r#"#[test] fn nested(){ assert_eq!(same(), same()); assert!(true); }"#;
    assert_eq!(production(1), 2);
}"##;
    assert!(!tautological_assertion(source));
    assert!(tautological_assertion(
        "#[test] fn real_tautology(){ assert_eq!(production(1), production(1)); }"
    ));
}

#[test]
fn assert_eq_parser_respects_nested_and_message_commas() {
    assert!(self_eq("assert_eq!(pair(1, 2), pair(1, 2));"));
    assert!(self_eq("assert_eq!(x, x, \"message\");"));
    assert!(!self_eq("assert_eq!(pair(1, 2), pair(1, 3));"));
}
