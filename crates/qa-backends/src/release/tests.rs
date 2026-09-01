use super::*;
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(1);

fn temp_dir(name: &str) -> PathBuf {
    let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
    let path =
        std::env::temp_dir().join(format!("urqa-release-{name}-{}-{id}", std::process::id()));
    if path.exists() {
        fs::remove_dir_all(&path).unwrap();
    }
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn pending_release_subsystems_and_disabled_options_are_reported() {
    let root = temp_dir("states");
    let mut config = QaConfig::default();
    let docs_records = docs(&root, &config, false);
    assert_eq!(docs_records[0].status, EvidenceStatus::Unknown);
    assert_eq!(dependencies(&root, &config, false)[0].status, EvidenceStatus::Unknown);
    assert_eq!(api(&root, &config, true)[0].status, EvidenceStatus::Disabled);
    assert_eq!(generated(&root, &config, true)[0].status, EvidenceStatus::NotApplicable);
    assert_eq!(snapshots(&root, &config, true)[0].status, EvidenceStatus::NotApplicable);

    config.generated.verify = false;
    assert_eq!(generated(&root, &config, true)[0].status, EvidenceStatus::Disabled);
    config.reproducibility.enabled = false;
    assert_eq!(
        repro(&root, &config, &root.join("qa-out"), true)[0].status,
        EvidenceStatus::Disabled
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn snapshots_honor_pending_and_allow_policy_without_running_tools() {
    let root = temp_dir("snap");
    fs::write(root.join("case.snap"), "snapshot").unwrap();
    let mut config = QaConfig::default();
    assert_eq!(snapshots(&root, &config, false)[0].status, EvidenceStatus::Unknown);
    config.snapshots.unreferenced = "allow".into();
    assert_eq!(snapshots(&root, &config, true)[0].status, EvidenceStatus::Disabled);

    fs::create_dir_all(root.join("target")).unwrap();
    fs::write(root.join("target/ignored.snap"), "ignored").unwrap();
    fs::remove_file(root.join("case.snap")).unwrap();
    assert_eq!(snapshots(&root, &config, true)[0].status, EvidenceStatus::NotApplicable);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn generator_verification_detects_drift_determinism_and_failure_and_restores_outputs() {
    let root = temp_dir("generator");
    let target = GeneratorTarget {
        name: "gen".into(),
        command: "echo generated > generated.txt".into(),
        outputs: vec!["generated.txt".into()],
    };
    assert!(!root.join("generated.txt").exists());
    let records = verify_generator(&root, &target);
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].status, EvidenceStatus::Failed);
    assert_eq!(records[1].status, EvidenceStatus::Available);
    assert!(!root.join("generated.txt").exists());

    super::super::process::run_shell(&root, &target.command, &[]).unwrap();
    let original = fs::read(root.join("generated.txt")).unwrap();
    let records = verify_generator(&root, &target);
    assert!(records.iter().all(|record| record.status == EvidenceStatus::Available));
    assert_eq!(fs::read(root.join("generated.txt")).unwrap(), original);

    let failing = GeneratorTarget {
        name: "bad".into(),
        command: "exit 7".into(),
        outputs: vec!["bad.txt".into()],
    };
    let records = verify_generator(&root, &failing);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].status, EvidenceStatus::Failed);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn restore_reinstates_existing_files_and_removes_new_files() {
    let root = temp_dir("restore");
    let a = root.join("a.txt");
    let b = root.join("b.txt");
    fs::write(&a, "changed").unwrap();
    fs::write(&b, "created").unwrap();
    restore(&[a.clone(), b.clone()], &[Some(b"old".to_vec()), None]).unwrap();
    assert_eq!(fs::read(&a).unwrap(), b"old");
    assert!(!b.exists());
    restore(&[root.join("absent")], &[None]).unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn reproducibility_argument_and_comparison_helpers_are_strict() {
    let mut config = QaConfig::default();
    assert_eq!(
        repro_build_args(&config),
        vec!["build", "--workspace", "--jobs=1", "--release", "--locked"]
    );
    config.reproducibility.release = false;
    config.reproducibility.locked = false;
    assert_eq!(repro_build_args(&config), vec!["build", "--workspace", "--jobs=1"]);

    let root = Path::new("repro");
    assert_eq!(repro_comparison(root, &[]).status, EvidenceStatus::Unknown);
    let mut one = BTreeMap::new();
    one.insert("app".to_string(), vec![1, 2, 3]);
    assert_eq!(
        repro_comparison(root, &[one.clone(), one.clone()]).status,
        EvidenceStatus::Available
    );
    let mut two = one.clone();
    two.insert("app".to_string(), vec![9]);
    let mismatch = repro_comparison(root, &[one, two]);
    assert_eq!(mismatch.status, EvidenceStatus::Failed);
    let detail = mismatch.detail.unwrap();
    assert!(detail.contains("`app` differs in run 2"));
    assert!(detail.contains("first byte offset 0"));
    assert_eq!(first_diff(b"abc", b"axc"), Some(1));
    assert_eq!(first_diff(b"abc", b"abc"), None);
    assert_ne!(fnv64(b"abc"), fnv64(b"abd"));

    let mut baseline = BTreeMap::new();
    baseline.insert("a.exe".to_string(), vec![1]);
    baseline.insert("b.exe".to_string(), vec![2]);
    let mut missing = baseline.clone();
    missing.remove("b.exe");
    assert_eq!(
        snapshot_mismatch_detail(&baseline, &missing, 3).as_deref(),
        Some("release binary `b.exe` is missing in run 3")
    );
    let mut added = baseline.clone();
    added.insert("c.exe".to_string(), vec![3]);
    assert_eq!(
        snapshot_mismatch_detail(&baseline, &added, 4).as_deref(),
        Some("release binary `c.exe` appears only in run 4")
    );
    assert_eq!(snapshot_mismatch_detail(&baseline, &baseline, 2), None);
}

#[test]
fn job_and_record_helpers_report_command_outcomes() {
    let root = temp_dir("job");
    let available = job(&root, "TEST", "rustc", "rustc", &["--version"]);
    assert_eq!(available.status, EvidenceStatus::Available);
    assert!(available.detail.as_deref().unwrap_or_default().contains("stdout:"));
    let failed = job(&root, "TEST", "bad", "rustc", &["--definitely-invalid-option"]);
    assert_eq!(failed.status, EvidenceStatus::Failed);
    assert!(failed.detail.as_deref().unwrap_or_default().contains("stderr:"));
    assert_eq!(
        job(&root, "TEST", "missing", "urqa-program-that-does-not-exist", &[]).status,
        EvidenceStatus::Unavailable
    );
    let item = record("X", "y", EvidenceStatus::Available, Some(Path::new("z")), "ok");
    assert_eq!((item.family.as_str(), item.check.as_str()), ("X", "y"));
    assert!(excluded(Path::new("root/target/a")));
    assert!(!excluded(Path::new("root/src/a")));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn repro_preconditions_distinguish_disabled_pending_and_no_artifacts() {
    let root = temp_dir("precondition");
    let mut config = QaConfig::default();
    config.reproducibility.enabled = false;
    assert_eq!(repro_precondition(&root, &config, true).unwrap().status, EvidenceStatus::Disabled);
    config.reproducibility.enabled = true;
    assert_eq!(repro_precondition(&root, &config, false).unwrap().status, EvidenceStatus::Unknown);
    assert_eq!(
        repro_precondition(&root, &config, true).unwrap().status,
        EvidenceStatus::NotApplicable
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn api_arguments_include_baseline_only_when_configured() {
    let mut config = QaConfig::default();
    assert_eq!(api_args(&config), vec!["semver-checks"]);
    config.api.baseline = Some("v1.2.3".into());
    assert_eq!(api_args(&config), vec!["semver-checks", "--baseline-rev", "v1.2.3"]);
}

#[test]
fn release_branch_helpers_cover_empty_enabled_pending_and_process_outcomes() {
    let root = temp_dir("branch-helpers");
    let mut config = QaConfig::default();

    config.documentation.run_doctests = false;
    config.documentation.check_examples = false;
    assert!(docs(&root, &config, true).is_empty());

    config.dependencies.run_cargo_deny = false;
    config.dependencies.run_unused = false;
    assert!(dependencies(&root, &config, true).is_empty());

    config.api.run_semver_checks = true;
    assert_eq!(api(&root, &config, false)[0].status, EvidenceStatus::Unknown);

    config.generated.verify = true;
    config.generated.target = vec![GeneratorTarget {
        name: "pending".into(),
        command: "echo ignored".into(),
        outputs: vec!["ignored.txt".into()],
    }];
    assert_eq!(generated(&root, &config, false)[0].status, EvidenceStatus::Unknown);

    let ok = super::super::process::run(&root, "rustc", &["--version".into()], &[]).unwrap();
    assert_eq!(api_output_record(ok).status, EvidenceStatus::Available);
    let failed = super::super::process::run(
        &root,
        "rustc",
        &["--definitely-not-a-real-rustc-option".into()],
        &[],
    )
    .unwrap();
    assert_eq!(api_output_record(failed).status, EvidenceStatus::Failed);

    let target = root.join("repro-clean");
    fs::create_dir_all(&target).unwrap();
    fs::write(target.join("stale"), b"x").unwrap();
    clean_repro_target(&target).unwrap();
    assert!(!target.exists());

    let unavailable = repro_build_result(
        &root,
        &config,
        &root.join("target"),
        Err(std::io::Error::new(std::io::ErrorKind::NotFound, "cargo unavailable")),
    )
    .unwrap_err();
    assert_eq!(unavailable.status, EvidenceStatus::Unavailable);
    assert!(
        unavailable.detail.as_deref().is_some_and(|detail| detail.contains("cargo unavailable"))
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn fnv64_matches_standard_fnv1a_vectors() {
    assert_eq!(fnv64(b""), 0xcbf2_9ce4_8422_2325);
    assert_eq!(fnv64(b"a"), 0xaf63_dc4c_8601_ec8c);
    assert_eq!(fnv64(b"abc"), 0xe71f_a219_0541_574b);
    assert_eq!(fnv64(b"hello"), 0xa430_d846_80aa_bd0b);
}

#[test]
fn binary_snapshot_and_successful_build_output_preserve_exact_artifact_bytes() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = temp_dir("snapshot-bytes");
    let target_dir = root.join("target");
    let mut config = QaConfig::default();
    config.reproducibility.release = false;

    let paths = super::super::artifact::binary_paths(workspace, &target_dir, false);
    assert!(!paths.is_empty());
    let path = &paths[0];
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, b"exact-artifact-bytes").unwrap();
    let name = path.file_name().unwrap().to_string_lossy().into_owned();

    let snapshot = snapshot_binaries(workspace, &config, &target_dir);
    assert_eq!(snapshot.len(), 1);
    assert_eq!(snapshot.get(&name).map(Vec::as_slice), Some(b"exact-artifact-bytes".as_slice()));

    let success =
        super::super::process::run(workspace, "rustc", &["--version".into()], &[]).unwrap();
    let output = repro_build_output(workspace, &config, &target_dir, success).unwrap();
    assert_eq!(output, snapshot);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn release_run_and_repro_execute_never_silently_return_no_evidence() {
    let root = temp_dir("run-evidence");
    let mut config = QaConfig::default();
    config.documentation.run_doctests = false;
    config.documentation.check_examples = false;
    config.dependencies.run_cargo_deny = false;
    config.dependencies.run_unused = false;
    config.api.run_semver_checks = false;
    config.generated.verify = false;
    config.reproducibility.enabled = false;
    let evidence = run(&root, &config, &root.join("qa-out"), true);
    assert!(!evidence.is_empty());
    assert!(evidence.iter().any(|record| record.family == "REPRO"));

    config.reproducibility.enabled = true;
    let repro = repro_execute(&root, &config, &root.join("qa-out"));
    assert!(!repro.is_empty());
    assert_eq!(repro[0].family, "REPRO");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn job_status_classification_distinguishes_success_missing_tools_and_real_failures() {
    assert_eq!(classify_job_status(true, "anything"), EvidenceStatus::Available);
    for detail in [
        "error: no such command: cargo-foo",
        "'cargo-foo' is not recognized as a command",
        "could not execute process cargo-foo",
    ] {
        assert_eq!(classify_job_status(false, detail), EvidenceStatus::Unavailable);
    }
    assert_eq!(classify_job_status(false, "tests failed"), EvidenceStatus::Failed);
}

#[test]
fn api_enabled_without_execute_returns_pending_evidence() {
    let root = temp_dir("api-pending");
    let mut config = QaConfig::default();
    config.api.run_semver_checks = true;
    let records = api(&root, &config, false);
    assert!(!records.is_empty());
    assert!(records.iter().all(|record| record.status == EvidenceStatus::Unknown));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn repro_execute_cannot_turn_a_failed_build_into_an_available_empty_snapshot() {
    let root = temp_dir("repro-invalid-workspace");
    let mut config = QaConfig::default();
    config.reproducibility.enabled = true;
    config.reproducibility.runs = 2;
    let records = repro_execute(&root, &config, &root.join("qa-out"));
    assert_eq!(records.len(), 1);
    assert!(matches!(
        records[0].status,
        EvidenceStatus::Failed | EvidenceStatus::Unavailable | EvidenceStatus::NotApplicable
    ));
    fs::remove_dir_all(root).unwrap();
}
