use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(1);

fn temp_dir(name: &str) -> PathBuf {
    let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
    let path =
        std::env::temp_dir().join(format!("urqa-platform-{name}-{}-{id}", std::process::id()));
    if path.exists() {
        fs::remove_dir_all(&path).unwrap();
    }
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn pending_platform_matrix_contains_configured_jobs_and_targets() {
    let root = temp_dir("matrix");
    let mut config = QaConfig::default();
    config.platform.check_each_feature = true;
    config.platform.targets = vec!["wasm32-unknown-unknown".into()];
    config.platform.check_msrv = false;
    let records = run(&root, &config, false);
    let checks = records.iter().map(|record| record.check.as_str()).collect::<Vec<_>>();
    assert!(checks.contains(&"default"));
    assert!(checks.contains(&"no-default"));
    assert!(checks.contains(&"all-features"));
    assert!(checks.contains(&"each-feature"));
    assert!(checks.contains(&"target:wasm32-unknown-unknown"));
    assert!(records.iter().all(|record| record.status == EvidenceStatus::Unknown));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn manifest_discovery_and_msrv_jobs_cover_declared_missing_and_invalid_manifests() {
    let root = temp_dir("msrv");
    fs::create_dir_all(root.join("a/src")).unwrap();
    fs::create_dir_all(root.join("b/src")).unwrap();
    fs::create_dir_all(root.join("target/ignored")).unwrap();
    fs::write(
        root.join("a/Cargo.toml"),
        "[package]\nname='a'\nversion='0.1.0'\nrust-version='1.85'\n",
    )
    .unwrap();
    fs::write(root.join("b/Cargo.toml"), "[package]\nname='b'\nversion='0.1.0'\n").unwrap();
    fs::write(root.join("Cargo.toml"), "not toml = [").unwrap();
    fs::write(root.join("target/ignored/Cargo.toml"), "[package]\nname='ignored'\n").unwrap();

    let found = manifests(&root);
    assert_eq!(found.len(), 3);
    assert!(!found.iter().any(|path| path.to_string_lossy().contains("target")));

    let records = msrv_jobs(&root, false);
    assert_eq!(records.len(), 2);
    assert!(records.iter().any(|record| record.check == "msrv:a"));
    assert!(records.iter().any(|record| record.check == "msrv:b"));
    assert!(records.iter().all(|record| record.status == EvidenceStatus::Unknown));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn job_runner_reports_pending_success_failure_and_missing_program() {
    let root = temp_dir("job");
    let args = ["--version".into()];
    assert_eq!(run_job(&root, false, "pending", "rustc", &args).status, EvidenceStatus::Unknown);
    assert_eq!(run_job(&root, true, "ok", "rustc", &args).status, EvidenceStatus::Available);
    assert_eq!(
        run_job(&root, true, "bad", "rustc", &["--definitely-invalid-option".into()]).status,
        EvidenceStatus::Failed
    );
    assert_eq!(
        run_job(&root, true, "missing", "urqa-program-that-does-not-exist", &[]).status,
        EvidenceStatus::Unavailable
    );
    fs::remove_dir_all(root).unwrap();
}
