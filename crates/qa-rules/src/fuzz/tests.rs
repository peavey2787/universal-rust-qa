use super::*;
use crate::test_support::{cleanup, discover, ids};

#[test]
fn fuzz_analysis_tracks_targets_missing_critical_parsers_and_property_tests() {
    let (root, source) = discover(&[
        (
            "src/lib.rs",
            "#[qa_attr::critical_parser] fn parse(data:&[u8]){let _=data;}\n#[qa_attr::critical_parser] fn missing(data:&[u8]){let _=data;}\n#[test] #[qa_attr::proptest] fn prop(){ assert_ne!(parse as usize, 0); }\n",
        ),
        ("fuzz/fuzz_targets/parser.rs", "fuzz_target!(|data: &[u8]| { parse(data); });\n"),
        ("fuzz/fuzz_targets/vacuous.rs", "fuzz_target!(|data: &[u8]| { let _=data; });\n"),
    ]);
    let mut findings = Vec::new();
    let out = analyze(&source, &QaConfig::default(), &mut findings);
    assert_eq!(out.targets.len(), 2);
    assert_eq!(out.critical_missing, 1);
    assert_eq!(out.property_test_count, 1);
    assert!(out.targets.iter().any(|t| t.reaches_production));
    assert!(out.targets.iter().any(|t| !t.reaches_production));
    let found = ids(&findings);
    assert!(found.contains(&"QA-FUZZ-001"));
    assert!(found.contains(&"QA-FUZZ-004"));
    let missing =
        findings.iter().filter(|finding| finding.rule_id == "QA-FUZZ-001").collect::<Vec<_>>();
    assert_eq!(missing.len(), 1);
    assert!(missing[0].message.contains("missing"));
    let vacuous =
        findings.iter().filter(|finding| finding.rule_id == "QA-FUZZ-004").collect::<Vec<_>>();
    assert_eq!(vacuous.len(), 1);
    assert!(vacuous[0].path.as_deref().is_some_and(|path| path.contains("vacuous.rs")));
    cleanup(&root);
}

#[test]
fn vacuous_target_policy_can_be_disabled() {
    let (root, source) = discover(&[(
        "fuzz/fuzz_targets/vacuous.rs",
        "fuzz_target!(|data: &[u8]| { let _=data; });\n",
    )]);
    let mut config = QaConfig::default();
    config.fuzz.reject_vacuous_targets = false;
    let mut findings = Vec::new();
    let out = analyze(&source, &config, &mut findings);
    assert_eq!(out.targets.len(), 1);
    assert!(findings.is_empty());
    cleanup(&root);
}

#[test]
fn fuzz_target_and_property_test_signals_require_the_exact_context() {
    let (root, source) = discover(&[
        ("fuzz/fuzz_targets/real.rs", "fuzz_target!(|data: &[u8]| { let _ = parse(data); });\n"),
        ("fuzz/fuzz_targets/not_target.rs", "fn helper() {}\n"),
        ("src/fake.rs", "fn ordinary() { let _ = \"fuzz_target!(\"; }\n"),
        (
            "tests/properties.rs",
            "#[test] fn property_case(){ proptest::proptest!(|(x in 0u8..10)| assert!(x < 10)); }\n",
        ),
        ("src/not_test.rs", "fn property_helper(){ let _ = \"proptest!\"; }\n"),
    ]);
    let mut config = QaConfig::default();
    config.fuzz.build_targets = false;
    config.fuzz.require_critical_parser_target = false;
    let mut findings = Vec::new();
    let evidence = analyze(&source, &config, &mut findings);
    assert_eq!(evidence.targets.len(), 1);
    assert!(evidence.targets[0].path.contains("real.rs"));
    assert_eq!(evidence.property_test_count, 1);
    cleanup(&root);
}

#[test]
fn critical_parser_target_matching_uses_the_target_file_not_a_different_file() {
    let (root, mut source) = discover(&[
        (
            "src/lib.rs",
            "#[qa_attr::critical_parser] fn parse_packet(data:&[u8]) { let _ = data; }\n",
        ),
        ("src/unrelated.rs", "fn unrelated() {}\n"),
        ("fuzz/fuzz_targets/parser.rs", "fuzz_target!(|data: &[u8]| { parse_packet(data); });\n"),
    ]);
    source.files.retain(|file| !file.path.ends_with("src/lib.rs"));
    let mut findings = Vec::new();
    let evidence = analyze(&source, &QaConfig::default(), &mut findings);
    assert_eq!(evidence.critical_missing, 0);
    assert!(!findings.iter().any(|finding| finding.rule_id == "QA-FUZZ-001"));
    cleanup(&root);
}
