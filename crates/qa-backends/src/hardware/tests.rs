use super::*;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(1);

fn temp_dir(name: &str) -> PathBuf {
    let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("urqa-hw-{name}-{}-{id}", std::process::id()));
    if path.exists() {
        fs::remove_dir_all(&path).unwrap();
    }
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn hardware_disabled_pending_and_no_target_states_are_covered() {
    let root = temp_dir("states");
    let mut config = QaConfig::default();
    assert_eq!(run(&root, &config, true)[0].status, EvidenceStatus::Disabled);
    config.hardware.enabled = true;
    assert_eq!(run(&root, &config, false)[0].status, EvidenceStatus::Unknown);
    let records = run(&root, &config, true);
    assert_eq!(records.len(), 2);
    assert!(records.iter().all(|r| r.status == EvidenceStatus::NotApplicable));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn linker_map_recognition_distinguishes_known_unknown_and_missing_files() {
    let root = temp_dir("map");
    let mut config = QaConfig::default();
    config.hardware.enabled = true;
    config.hardware.linker_map = Some("firmware.map".into());

    fs::write(root.join("firmware.map"), "0000 _stack_top\n0001 _sbss\n").unwrap();
    let records = run(&root, &config, true);
    assert_eq!(records[1].status, EvidenceStatus::Available);
    assert!(records[1].detail.as_deref().unwrap().contains("2 conventional"));

    fs::write(root.join("firmware.map"), "nothing useful\n").unwrap();
    assert_eq!(run(&root, &config, true)[1].status, EvidenceStatus::Unknown);

    fs::remove_file(root.join("firmware.map")).unwrap();
    assert_eq!(run(&root, &config, true)[1].status, EvidenceStatus::Unavailable);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn job_reports_success_failure_and_missing_program() {
    let root = temp_dir("job");
    let ok = job(&root, "version", "rustc", &["--version".into()], Some("rustc"));
    assert_eq!(ok.status, EvidenceStatus::Available);
    let bad = job(&root, "bad", "rustc", &["--definitely-invalid-option".into()], None);
    assert_eq!(bad.status, EvidenceStatus::Failed);
    let missing = job(&root, "missing", "urqa-program-that-does-not-exist", &[], None);
    assert_eq!(missing.status, EvidenceStatus::Unavailable);
    fs::remove_dir_all(root).unwrap();
}
