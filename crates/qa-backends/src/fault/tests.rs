use super::*;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(1);

fn temp_dir(name: &str) -> PathBuf {
    let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("urqa-fault-{name}-{}-{id}", std::process::id()));
    if path.exists() {
        fs::remove_dir_all(&path).unwrap();
    }
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn disabled_pending_and_zero_schedule_runs_are_reported() {
    let root = temp_dir("run");
    let config = QaConfig::default();
    assert_eq!(run(&root, &config, &root.join("qa-out"), true)[0].status, EvidenceStatus::Disabled);

    let mut config = QaConfig::default();
    config.fault.enabled = true;
    assert_eq!(run(&root, &config, &root.join("qa-out"), false)[0].status, EvidenceStatus::Unknown);

    config.fault.max_fail_points = 0;
    config.fault.kinds = vec!["io".into(), "clock".into()];
    let records = run(&root, &config, &root.join("qa-out"), true);
    assert_eq!(records.len(), 3);
    assert!(records.iter().all(|record| record.status == EvidenceStatus::Available));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn failure_cases_and_replay_files_preserve_deterministic_coordinates() {
    let root = temp_dir("persist");
    let config = QaConfig::default();
    let value = failure_case(&config, "io", 3, Some("boom"));
    assert_eq!(value["seed"], config.fault.seed);
    assert_eq!(value["kind"], "io");
    assert_eq!(value["fail_at"], 3);
    assert_eq!(value["detail"], "boom");

    let empty = persist_failures(&root, Vec::new());
    assert_eq!(empty.status, EvidenceStatus::Available);
    assert!(empty.source.is_none());

    let persisted = persist_failures(&root, vec![value]);
    assert_eq!(persisted.status, EvidenceStatus::Available);
    let path = PathBuf::from(persisted.source.unwrap());
    let text = fs::read_to_string(path).unwrap();
    assert!(text.contains("\"fail_at\":3"));
    assert!(text.ends_with('\n'));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn record_carries_family_source_and_detail() {
    let source = Path::new("fixture.rs");
    let item = record("io", EvidenceStatus::Failed, Some(source), "failure");
    assert_eq!(item.family, "FAULT");
    assert_eq!(item.check, "io");
    assert_eq!(item.source.as_deref(), Some("fixture.rs"));
    assert_eq!(item.detail.as_deref(), Some("failure"));
}

#[test]
fn run_kind_with_counts_each_schedule_and_persists_failed_coordinates() {
    let mut config = QaConfig::default();
    config.fault.max_fail_points = 3;
    config.fault.seed = 77;
    let mut failures = Vec::new();
    let outcomes = [true, false, true];
    let record = run_kind_with(&config, "io", &mut failures, |fail_at| Ok(outcomes[fail_at]));

    assert_eq!(record.status, EvidenceStatus::Failed);
    assert_eq!(
        record.detail.as_deref(),
        Some("3 deterministic fail points, 1 failing schedules, seed=77")
    );
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0]["seed"], 77);
    assert_eq!(failures[0]["kind"], "io");
    assert_eq!(failures[0]["fail_at"], 1);
}

#[test]
fn run_kind_with_reports_schedule_errors_at_the_exact_fail_point() {
    let mut config = QaConfig::default();
    config.fault.max_fail_points = 4;
    config.fault.seed = 19;
    let mut failures = Vec::new();
    let record = run_kind_with(&config, "clock", &mut failures, |fail_at| {
        if fail_at == 2 { Err("runner unavailable".into()) } else { Ok(true) }
    });

    assert_eq!(record.status, EvidenceStatus::Unavailable);
    assert_eq!(record.check, "clock");
    assert_eq!(record.detail.as_deref(), Some("seed=19 fail_at=2: runner unavailable"));
    assert!(failures.is_empty());
}
