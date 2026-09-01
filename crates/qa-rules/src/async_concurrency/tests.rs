use super::*;
use crate::test_support::{cleanup, discover, ids};

#[test]
fn async_rules_detect_blocking_detached_lock_cancellation_relaxed_static_and_drop_panics() {
    let (root, source) = discover(&[(
        "src/lib.rs",
        r#"
static mut COUNTER: u32 = 0;
#[qa_attr::critical_async]
async fn critical(lock: std::sync::Mutex<u8>) { let guard = lock.lock().unwrap(); std::thread::sleep(std::time::Duration::from_millis(1)); tokio::spawn(async {}); work().await; drop(guard); }
#[qa_attr::critical_concurrency]
fn atomic(a: &std::sync::atomic::AtomicU8) { a.load(std::sync::atomic::Ordering::Relaxed); }
struct X;
impl Drop for X { fn drop(&mut self) { panic!("boom"); } }
async fn work() {}
unsafe impl Send for X {}
"#,
    )]);
    let mut findings = Vec::new();
    analyze(&source, &QaConfig::default(), &mut findings);
    let found = ids(&findings);
    for expected in [
        "QA-ASYNC-001",
        "QA-ASYNC-003",
        "QA-ASYNC-004",
        "QA-ASYNC-005",
        "QA-ASYNC-002",
        "QA-CONC-006",
        "QA-CONC-003",
        "QA-CONC-004",
    ] {
        assert!(found.contains(&expected), "missing {expected}: {found:?}");
    }
    cleanup(&root);
}

#[test]
fn declared_contract_supervised_spawn_dropped_guard_and_allow_policies_suppress_findings() {
    let (root, source) = discover(&[(
        "src/lib.rs",
        r#"
static mut COUNTER: u32 = 0;
#[qa_attr::critical_async]
#[qa_attr::cancel_safe]
async fn safe(lock: std::sync::Mutex<u8>) { let guard = lock.lock().unwrap(); drop(guard); let handle = tokio::spawn(async {}); work().await; let _=handle; }
#[qa_attr::critical_concurrency]
fn atomic(a: &std::sync::atomic::AtomicU8) { a.load(std::sync::atomic::Ordering::Relaxed); }
async fn work() {}
"#,
    )]);
    let mut config = QaConfig::default();
    config.async_rules.relaxed_atomics = "allow".into();
    config.async_rules.static_mut = "allow".into();
    let mut findings = Vec::new();
    analyze(&source, &config, &mut findings);
    let found = ids(&findings);
    assert!(!found.contains(&"QA-ASYNC-001"));
    assert!(!found.contains(&"QA-ASYNC-004"));
    assert!(!found.contains(&"QA-ASYNC-005"));
    assert!(!found.contains(&"QA-CONC-006"));
    assert!(!found.contains(&"QA-CONC-004"));
    cleanup(&root);
}

#[test]
fn helper_detectors_cover_positive_and_negative_forms() {
    assert!(blocking_call("std::fs::read(path)"));
    assert!(!blocking_call("tokio::fs::read(path).await"));
    assert!(detached_spawn("tokio::spawn(async {});"));
    assert!(detached_spawn("let guard = lock.lock(); tokio::spawn(async {});"));
    assert!(!detached_spawn("let handle = tokio::spawn(async {});"));
    assert!(!detached_spawn("let mut set = JoinSet::new(); set.spawn(async {});"));
    assert!(guard_may_cross_await("let g = m.lock(); work().await"));
    assert!(!guard_may_cross_await("let g = m.lock(); drop(g); work().await"));
    assert!(!guard_may_cross_await("work().await"));
}

#[test]
fn drop_send_sync_and_static_mut_conjunctions_are_exact() {
    let (root, source) = discover(&[(
        "src/lib.rs",
        r#"
struct X;
impl Drop for X { fn drop(&mut self) { panic!("drop panic"); } }
fn drop() { panic!("ordinary function"); }
impl X { fn helper(&self) { panic!("not drop"); } }
unsafe impl Send for X {}
struct Y;
unsafe impl SomeTrait for Y {}
unsafe impl Sync for Y { /* SAFETY: fixture rationale */ }
static mut FIRST: u8 = 0;
static mut SECOND: u8 = 0;
trait SomeTrait {}
"#,
    )]);
    let real_drop = source
        .functions
        .iter()
        .find(|function| function.qualified_name.contains("Drop for X") && function.name == "drop")
        .unwrap();
    let ordinary_drop =
        source.functions.iter().find(|function| function.qualified_name == "drop").unwrap();
    let helper = source.functions.iter().find(|function| function.name == "helper").unwrap();
    assert!(is_drop(real_drop));
    assert!(!is_drop(ordinary_drop));
    assert!(!is_drop(helper));

    let mut findings = Vec::new();
    check_drop_panic(real_drop, &sanitize(&real_drop.source), &mut findings);
    assert_eq!(ids(&findings), vec!["QA-ASYNC-002"]);
    findings.clear();
    check_drop_panic(ordinary_drop, &sanitize(&ordinary_drop.source), &mut findings);
    check_drop_panic(helper, &sanitize(&helper.source), &mut findings);
    assert!(findings.is_empty());

    analyze_send_sync(&source, &mut findings);
    assert_eq!(findings.iter().filter(|finding| finding.rule_id == "QA-CONC-003").count(), 1);
    assert!(findings.iter().any(|finding| {
        finding.rule_id == "QA-CONC-003" && finding.message.contains("Send/Sync")
    }));

    findings.clear();
    analyze_static_mut(&source, &QaConfig::default(), &mut findings);
    let lines = findings
        .iter()
        .filter(|finding| finding.rule_id == "QA-CONC-004")
        .map(|finding| finding.line.unwrap())
        .collect::<Vec<_>>();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[1], lines[0] + 1);
    cleanup(&root);
}

#[test]
fn static_mut_findings_preserve_the_exact_source_line() {
    let (root, source) =
        discover(&[("src/lib.rs", "fn before() {}\n\nstatic mut SHARED: u8 = 0;\n")]);
    let mut findings = Vec::new();
    analyze_static_mut(&source, &QaConfig::default(), &mut findings);
    let finding = findings.iter().find(|finding| finding.rule_id == "QA-CONC-004").unwrap();
    assert_eq!(finding.line, Some(3));
    cleanup(&root);
}
