use super::*;
use crate::test_support::{cleanup, discover, ids};

#[test]
fn performance_rules_detect_false_sharing_vector_contract_and_hot_output() {
    let (root, source) = discover(&[(
        "src/lib.rs",
        r#"
use std::sync::{Mutex, RwLock};
struct Shared { a: Mutex<u8>, b: RwLock<u8> }
#[qa_attr::vectorize_expected]
fn scalar() { let x = 1; let _=x; }
#[qa_attr::hot_path]
fn hot() { println!("hot"); }
"#,
    )]);
    let mut config = QaConfig::default();
    config.performance.enabled = true;
    let mut findings = Vec::new();
    analyze(&source, &config, &mut findings);
    let found = ids(&findings);
    assert!(found.contains(&"QA-PERF-001"));
    assert!(found.contains(&"QA-PERF-002"));
    assert!(found.contains(&"QA-PERF-005"));
    cleanup(&root);
}

#[test]
fn padded_shared_type_loop_vector_contract_and_quiet_hot_path_are_accepted() {
    let (root, source) = discover(&[(
        "src/lib.rs",
        r#"
use std::sync::{Mutex, RwLock};
#[repr(align(64))]
struct Shared { a: Mutex<u8>, b: RwLock<u8> }
#[qa_attr::vectorize_expected]
fn vector() { for x in 0..4 { let _=x; } }
#[qa_attr::hot_path]
fn hot() { let x=1; let _=x; }
"#,
    )]);
    let mut config = QaConfig::default();
    config.performance.enabled = true;
    let mut findings = Vec::new();
    analyze(&source, &config, &mut findings);
    let found = ids(&findings);
    assert!(!found.contains(&"QA-PERF-001"));
    assert!(!found.contains(&"QA-PERF-002"));
    assert!(!found.contains(&"QA-PERF-005"));
    cleanup(&root);
}

#[test]
fn one_shared_field_is_below_the_false_sharing_threshold() {
    let (root, source) = discover(&[(
        "src/lib.rs",
        "struct Single { one: std::sync::atomic::AtomicU64, value: u64 }\n",
    )]);
    let mut config = QaConfig::default();
    config.performance.enabled = true;
    let mut findings = Vec::new();
    analyze(&source, &config, &mut findings);
    assert!(!findings.iter().any(|finding| finding.rule_id == "QA-PERF-001"));
    cleanup(&root);
}
