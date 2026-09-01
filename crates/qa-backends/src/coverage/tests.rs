use super::*;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(1);

fn temp_dir(name: &str) -> PathBuf {
    let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
    let path =
        std::env::temp_dir().join(format!("urqa-coverage-{name}-{}-{id}", std::process::id()));
    if path.exists() {
        fs::remove_dir_all(&path).unwrap();
    }
    fs::create_dir_all(&path).unwrap();
    path
}

fn write_fixture(path: &Path) {
    let value = serde_json::json!({
        "data": [{
            "totals": {"lines": {"percent": 75.0}},
            "files": [
                {
                    "filename": "C:\\work\\src\\lib.rs",
                    "segments": [
                        [10, 1, 1],
                        [11, 1, 0],
                        [11, 2, 3],
                        [12, 1, 0],
                        [null, 1, 7]
                    ]
                },
                {"segments": [[1, 1, 1]]}
            ]
        }]
    });
    fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
}

#[test]
fn parses_coverage_and_computes_function_percentages() {
    let root = temp_dir("parse");
    let path = root.join("llvm-cov.json");
    write_fixture(&path);
    let evidence = parse(&path);
    assert_eq!(evidence.status, EvidenceStatus::Available);
    assert_eq!(evidence.percent, Some(75.0));
    let expected_source = path.to_string_lossy().into_owned();
    assert_eq!(evidence.source.as_deref(), Some(expected_source.as_str()));
    assert!(evidence.error.is_none());
    assert!(evidence.files.contains_key("C:/work/src/lib.rs"));
    assert_eq!(function_percent(&evidence, "C:\\work\\src\\lib.rs", 10, 12), Some(200.0 / 3.0));
    assert_eq!(function_percent(&evidence, "src/lib.rs", 10, 11), Some(100.0));
    assert_eq!(function_percent(&evidence, "src/lib.rs", 20, 21), None);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn collect_honors_disabled_and_explicit_existing_evidence_paths() {
    let root = temp_dir("collect");
    let out = root.join("qa-out");
    fs::create_dir_all(&out).unwrap();

    let mut config = QaConfig::default();
    config.coverage.mode = "off".into();
    assert_eq!(collect(&root, &config, &out, false).status, EvidenceStatus::Disabled);

    config.coverage.mode = "auto".into();
    let missing = collect(&root, &config, &out, false);
    assert_eq!(missing.status, EvidenceStatus::Unavailable);
    assert!(missing.error.as_deref().is_some_and(|error| error.contains("not found")));

    write_fixture(&out.join("llvm-cov.json"));
    let available = collect(&root, &config, &out, false);
    assert_eq!(available.status, EvidenceStatus::Available);
    assert_eq!(available.percent, Some(75.0));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn existing_mode_reuses_json_even_when_the_run_requests_fresh_coverage() {
    let root = temp_dir("existing-mode");
    let out = root.join("qa-out");
    fs::create_dir_all(&out).unwrap();
    write_fixture(&out.join("llvm-cov.json"));
    let mut config = QaConfig::default();
    config.coverage.mode = "existing".into();

    let evidence = collect_with(&config, &out, true, || {
        panic!("existing coverage mode must not launch cargo llvm-cov")
    });
    assert_eq!(evidence.status, EvidenceStatus::Available);
    assert_eq!(evidence.percent, Some(75.0));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn malformed_coverage_is_failed_and_unavailable_status_has_no_function_percent() {
    let root = temp_dir("bad");
    let path = root.join("llvm-cov.json");
    fs::write(&path, "not-json").unwrap();
    let bad = parse(&path);
    assert_eq!(bad.status, EvidenceStatus::Failed);
    assert!(bad.error.as_deref().is_some_and(|error| error.contains("expected")));
    assert!(bad.source.is_none());

    let missing_path = root.join("absent.json");
    let missing = parse(&missing_path);
    assert_eq!(missing.status, EvidenceStatus::Failed);
    assert!(missing.error.as_deref().is_some_and(|error| !error.is_empty()));
    assert!(missing.source.is_none());
    assert!(function_percent(&missing, "anything.rs", 1, 2).is_none());
    assert_eq!(normalize("a\\b\\c.rs"), "a/b/c.rs");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn coverage_diagnostics_preserve_test_stdout_and_failure_tail() {
    let stdout = format!("{}test-panic-at-tail", "test progress\n".repeat(500));
    let stderr = format!("{}failure-at-tail", "compiler-progress\n".repeat(500));
    let detail = crate::process::diagnostics(stdout.as_bytes(), stderr.as_bytes());
    assert!(detail.contains("command stream truncated"));
    assert!(detail.contains("test-panic-at-tail"));
    assert!(detail.contains("failure-at-tail"));
}

#[test]
fn forced_collection_maps_success_failure_and_unavailable_commands_without_running_coverage() {
    let root = temp_dir("forced-status");
    let out = root.join("qa-out");
    fs::create_dir_all(&out).unwrap();
    write_fixture(&out.join("llvm-cov.json"));
    let config = QaConfig::default();

    let success = collect_with(&config, &out, true, || CoverageCommand::Success);
    assert_eq!(success.status, EvidenceStatus::Available);
    assert_eq!(success.percent, Some(75.0));

    let failed = collect_with(&config, &out, true, || CoverageCommand::Failed("boom".into()));
    assert_eq!(failed.status, EvidenceStatus::Failed);
    assert_eq!(failed.error.as_deref(), Some("boom"));

    let unavailable =
        collect_with(&config, &out, true, || CoverageCommand::Unavailable("missing cargo".into()));
    assert_eq!(unavailable.status, EvidenceStatus::Unavailable);
    assert_eq!(unavailable.error.as_deref(), Some("missing cargo"));

    let mut disabled = config.clone();
    disabled.coverage.mode = "off".into();
    let disabled = collect_with(&disabled, &out, true, || panic!("disabled mode must not run"));
    assert_eq!(disabled.status, EvidenceStatus::Disabled);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn coverage_command_classifies_process_success_failure_and_spawn_errors() {
    let root = temp_dir("command-status");
    let ok = super::super::process::run(&root, "rustc", &["--version".into()], &[]);
    assert!(matches!(coverage_command(ok), CoverageCommand::Success));

    let bad = super::super::process::run(
        &root,
        "rustc",
        &["--definitely-not-a-real-rustc-option".into()],
        &[],
    );
    match coverage_command(bad) {
        CoverageCommand::Failed(detail) => assert!(!detail.is_empty()),
        other => panic!("expected failed command, got {other:?}"),
    }

    let missing = super::super::process::run(
        &root,
        "definitely-not-a-real-universal-rust-qa-command",
        &[],
        &[],
    );
    match coverage_command(missing) {
        CoverageCommand::Unavailable(detail) => assert!(!detail.is_empty()),
        other => panic!("expected unavailable command, got {other:?}"),
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn coverage_target_is_isolated_and_reset_before_collection() {
    let root = temp_dir("isolated-target");
    let output = root.join("qa-out");
    let target = output.join("llvm-cov-target");
    fs::create_dir_all(&target).unwrap();
    fs::write(target.join("stale.profraw"), b"stale").unwrap();
    fs::write(output.join("llvm-cov.json"), b"stale").unwrap();

    assert_eq!(prepare_coverage_target(&output).unwrap(), target);
    assert!(output.is_dir());
    assert!(!target.exists());
    assert!(!output.join("llvm-cov.json").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn coverage_arguments_distinguish_all_features_and_exact_output_path() {
    let path = Path::new("reports/llvm-cov.json");
    let mut config = QaConfig::default();
    config.coverage.all_features = false;
    assert_eq!(
        coverage_args(&config, path),
        vec!["llvm-cov", "--json", "--output-path", "reports/llvm-cov.json"]
    );
    config.coverage.all_features = true;
    assert_eq!(
        coverage_args(&config, path),
        vec!["llvm-cov", "--json", "--output-path", "reports/llvm-cov.json", "--all-features",]
    );
}

#[test]
fn coverage_environment_auto_provisions_llvm_tools_without_prompting() {
    assert_eq!(
        coverage_env("isolated-target"),
        [
            ("CARGO_LLVM_COV_TARGET_DIR", "isolated-target".into()),
            ("CARGO_LLVM_COV_BUILD_DIR", "isolated-target".into()),
            ("CARGO_LLVM_COV_SETUP", "yes".into()),
        ]
    );
}
