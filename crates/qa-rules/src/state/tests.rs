use super::*;
use crate::test_support::{cleanup, discover, ids};

#[test]
fn transition_rules_cover_wildcards_panics_and_async_mutation() {
    let (root, source) = discover(&[(
        "src/lib.rs",
        r#"
#[qa_attr::critical_state]
fn bad(s: u8) -> Result<u8, ()> { match s { 0 => Ok(1), _ => panic!("bad") } }
#[qa_attr::critical_state]
async fn crossing(mut state: u8) -> Result<u8, ()> { state = 1; work().await; Err(()) }
async fn work() {}
"#,
    )]);
    let mut findings = Vec::new();
    analyze(&source, &QaConfig::default(), &mut findings);
    let found = ids(&findings);
    assert!(found.iter().filter(|id| **id == "QA-STATE-001").count() >= 2);
    assert!(found.contains(&"QA-STATE-004"));
    assert!(found.contains(&"QA-STATE-007"));
    cleanup(&root);
}

#[test]
fn state_type_contracts_report_missing_roundtrip_restart_reachability_and_terminal_exit() {
    let (root, source) = discover(&[(
        "src/lib.rs",
        r#"
#[qa_attr::critical_state]
enum Session { Init, #[terminal] Done, Never }
fn step(s: Session) -> Result<Session, ()> { match s { Session::Init => Ok(Session::Done), Session::Done => Ok(Session::Init), Session::Never => Err(()) } }
"#,
    )]);
    let mut findings = Vec::new();
    analyze(&source, &QaConfig::default(), &mut findings);
    let found = ids(&findings);
    assert!(found.contains(&"QA-STATE-002"));
    assert!(found.contains(&"QA-STATE-006"));
    assert!(found.contains(&"QA-STATE-005"));
    cleanup(&root);
}

#[test]
fn recognized_contract_tests_and_explicit_reject_paths_suppress_missing_contract_findings() {
    let (root, source) = discover(&[(
        "src/lib.rs",
        r#"
#[qa_attr::critical_state]
enum Session { Init, #[terminal] Done }
#[qa_attr::critical_state]
fn step(s: Session) -> Result<Session, ()> { match s { Session::Init => Ok(Session::Done), _ => Err(()) } }
#[test]
fn session_roundtrip_property() { let _ = Session::Init; let encoded = encode(); let _ = decode(encoded); assert!(true == true); }
#[test]
fn session_restart_restore() { let _ = Session::Init; assert_eq!(restore(), restore()); }
fn encode()->u8{0} fn decode(_:u8)->Session{Session::Init} fn restore()->Session{Session::Init}
"#,
    )]);
    let mut findings = Vec::new();
    analyze(&source, &QaConfig::default(), &mut findings);
    let found = ids(&findings);
    assert!(!found.contains(&"QA-STATE-002"));
    assert!(!found.contains(&"QA-STATE-006"));
    assert!(!found.contains(&"QA-STATE-001"));
    cleanup(&root);
}

#[test]
fn terminal_arm_rejection_is_scoped_to_the_terminal_variant() {
    let code = "match s { Session::Done => Ok(Session::Init), Session::Never => Err(()) }";
    assert_eq!(terminal_arm_rejects(code, "Done"), Some(false));
    assert_eq!(terminal_arm_rejects(code, "Never"), Some(true));
    assert_eq!(terminal_arm_rejects(code, "Missing"), None);
}

#[test]
fn helper_predicates_cover_positive_and_negative_cases() {
    assert!(mutates_state_before_await("self.state = Ready; work().await"));
    assert!(!mutates_state_before_await("work().await; self.state = Ready;"));
    let (root, source) = discover(&[(
        "src/lib.rs",
        "#[qa_attr::state_machine] enum S { A }\n#[qa_attr::state_machine] fn f(){}\n",
    )]);
    assert!(is_state_type(&source.types[0]));
    assert!(is_state_function(&source.functions[0]));
    cleanup(&root);
}

#[test]
fn async_atomicity_requires_async_await_and_preawait_mutation_together() {
    let (root, source) = discover(&[(
        "src/lib.rs",
        r#"
#[qa_attr::critical_state]
async fn all(mut state:u8) { state = 1; work().await; }
#[qa_attr::critical_state]
async fn await_only(state:u8) { let _=state; work().await; }
#[qa_attr::critical_state]
async fn mutation_only(mut state:u8) { state = 1; }
async fn work() {}
"#,
    )]);
    let mut findings = Vec::new();
    let all = source.functions.iter().find(|function| function.name == "all").unwrap();
    check_async_atomicity(all, &sanitize(&all.source), &mut findings);
    assert_eq!(ids(&findings), vec!["QA-STATE-007"]);

    findings.clear();
    let await_only =
        source.functions.iter().find(|function| function.name == "await_only").unwrap();
    check_async_atomicity(await_only, &sanitize(&await_only.source), &mut findings);
    assert!(findings.is_empty());

    let mutation_only =
        source.functions.iter().find(|function| function.name == "mutation_only").unwrap();
    check_async_atomicity(mutation_only, &sanitize(&mutation_only.source), &mut findings);
    assert!(findings.is_empty());
    cleanup(&root);
}

#[test]
fn state_predicates_reject_ordinary_types_and_functions() {
    let (root, source) = discover(&[(
        "src/lib.rs",
        "enum Ordinary { A }\nfn ordinary() {}\n#[qa_attr::state_machine] enum State { A }\n#[qa_attr::state_machine] fn step() {}\n",
    )]);
    let ordinary_type = source.types.iter().find(|ty| ty.name == "Ordinary").unwrap();
    let state_type = source.types.iter().find(|ty| ty.name == "State").unwrap();
    assert!(!is_state_type(ordinary_type));
    assert!(is_state_type(state_type));
    let ordinary_fn = source.functions.iter().find(|function| function.name == "ordinary").unwrap();
    let state_fn = source.functions.iter().find(|function| function.name == "step").unwrap();
    assert!(!is_state_function(ordinary_fn));
    assert!(is_state_function(state_fn));
    cleanup(&root);
}

#[test]
fn variant_reachability_ignores_test_only_references_and_marks_only_unused_variants() {
    let (root, source) = discover(&[(
        "src/lib.rs",
        r#"
#[qa_attr::critical_state]
enum Session { Used, TestOnly, Missing }
fn production() { let _ = Session::Used; }
#[test]
fn only_test() { let _ = Session::TestOnly; }
"#,
    )]);
    let ty = source.types.iter().find(|ty| ty.name == "Session").unwrap();
    let mut findings = Vec::new();
    check_variant_reachability(&source, ty, &mut findings);
    let messages = findings.iter().map(|finding| finding.message.as_str()).collect::<Vec<_>>();
    assert_eq!(findings.len(), 2);
    assert!(messages.iter().any(|message| message.contains("TestOnly")));
    assert!(messages.iter().any(|message| message.contains("Missing")));
    assert!(!messages.iter().any(|message| message.contains("Used")));
    cleanup(&root);
}

#[test]
fn roundtrip_and_restart_detectors_cover_exact_threshold_and_either_restart_signal() {
    let (root, source) = discover(&[(
        "src/lib.rs",
        r#"
enum Session { A }
#[test] fn one_roundtrip_token() { let _ = Session::A; encode(); }
#[test] fn exact_roundtrip_pair() { let _ = Session::A; encode(); decode(); }
#[test] fn restart_in_name() { let _ = Session::A; }
#[test] fn ordinary_name() { let _ = Session::A; restore(); }
fn encode(){} fn decode(){} fn restore(){}
"#,
    )]);
    let tests =
        source.functions.iter().filter(|function| function.is_test).cloned().collect::<Vec<_>>();
    let one = WorkspaceSource {
        functions: tests
            .iter()
            .filter(|function| function.name == "one_roundtrip_token")
            .cloned()
            .collect(),
        ..WorkspaceSource::default()
    };
    assert!(!has_roundtrip_test(&one, "Session"));

    let pair = WorkspaceSource {
        functions: tests
            .iter()
            .filter(|function| function.name == "exact_roundtrip_pair")
            .cloned()
            .collect(),
        ..WorkspaceSource::default()
    };
    assert!(has_roundtrip_test(&pair, "Session"));

    let restart_name = WorkspaceSource {
        functions: tests
            .iter()
            .filter(|function| function.name == "restart_in_name")
            .cloned()
            .collect(),
        ..WorkspaceSource::default()
    };
    assert!(has_restart_test(&restart_name, "Session"));

    let restart_body = WorkspaceSource {
        functions: tests
            .iter()
            .filter(|function| function.name == "ordinary_name")
            .cloned()
            .collect(),
        ..WorkspaceSource::default()
    };
    assert!(has_restart_test(&restart_body, "Session"));
    cleanup(&root);
}

#[test]
fn terminal_and_reachability_filters_ignore_test_only_mentions() {
    let (root, source) = discover(&[(
        "src/lib.rs",
        r#"
#[qa_attr::critical_state]
enum Session { Used, #[terminal] Done }
fn production() { let _ = Session::Used; }
#[test]
fn test_only_terminal() {
    match Session::Done {
        Session::Done => Session::Used,
        _ => Session::Done,
    };
}
"#,
    )]);
    let ty = source.types.iter().find(|ty| ty.name == "Session").unwrap();

    let mut reachability = Vec::new();
    check_variant_reachability(&source, ty, &mut reachability);
    assert_eq!(reachability.len(), 1);
    assert!(reachability[0].message.contains("Done"));

    let mut terminal = Vec::new();
    check_terminal_variant(&source, "Done", &mut terminal);
    assert!(terminal.is_empty());
    cleanup(&root);
}
