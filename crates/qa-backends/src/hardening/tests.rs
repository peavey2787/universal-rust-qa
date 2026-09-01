use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(1);

fn temp_dir(name: &str) -> PathBuf {
    let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
    let path =
        std::env::temp_dir().join(format!("urqa-hardening-{name}-{}-{id}", std::process::id()));
    if path.exists() {
        fs::remove_dir_all(&path).unwrap();
    }
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn disabled_and_pending_hardening_states_are_reported() {
    let root = temp_dir("states");
    let mut config = QaConfig::default();
    config.hardening.enabled = false;
    assert_eq!(run(&root, &config, true)[0].status, EvidenceStatus::Disabled);
    config.hardening.enabled = true;
    assert_eq!(run(&root, &config, false)[0].status, EvidenceStatus::Unknown);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn path_disclosure_detects_workspace_paths_and_honors_policy() {
    let root = temp_dir("path");
    let binary = root.join("artifact.bin");
    let config = QaConfig::default();
    fs::write(&binary, format!("prefix {} suffix", root.display())).unwrap();
    let bad = path_disclosure(&binary, &root, &config);
    assert_eq!(bad.status, EvidenceStatus::Failed);

    fs::write(&binary, "clean artifact").unwrap();
    assert_eq!(path_disclosure(&binary, &root, &config).status, EvidenceStatus::Available);

    let mut relaxed = config.clone();
    relaxed.hardening.deny_host_paths = false;
    assert_eq!(path_disclosure(&binary, &root, &relaxed).status, EvidenceStatus::Disabled);
    assert_eq!(
        path_disclosure(&root.join("missing"), &root, &config).status,
        EvidenceStatus::Unavailable
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn host_path_markers_drop_empty_values_and_deduplicate_exact_matches() {
    assert_eq!(
        unique_nonempty_markers(["".into(), "same".into(), "same".into(), "other".into()]),
        vec!["same", "other"]
    );
    assert!(!host_path_markers(Path::new("")).iter().any(String::is_empty));
}

#[test]
fn platform_records_always_report_the_current_host_inspection_result() {
    let config = QaConfig::default();
    let records = platform_records(Path::new("definitely-missing-qa-artifact"), &config);
    assert!(!records.is_empty());
    assert!(records.iter().all(|record| record.family == "HARDEN"));
    #[cfg(target_os = "windows")]
    assert_eq!(records[0].check, "PE");
    #[cfg(target_os = "linux")]
    assert_eq!(records[0].check, "ELF");
    #[cfg(target_os = "macos")]
    assert_eq!(records[0].check, "Mach-O");
}

#[test]
fn path_marker_matching_accepts_native_and_alternate_separators() {
    assert!(!contains_path_marker("anything", ""));
    assert!(contains_path_marker("C:/Users/alice/project", "C:\\Users\\alice"));
    assert!(contains_path_marker("/home/alice/project", "/home/alice"));
    let markers = host_path_markers(Path::new("workspace-marker"));
    assert_eq!(markers.first().map(String::as_str), Some("workspace-marker"));
}

#[cfg(target_os = "linux")]
#[test]
fn elf_mitigation_parsers_distinguish_secure_and_insecure_headers() {
    let secure = "Type: DYN\nGNU_RELRO\nFLAGS NOW\nGNU_STACK RW\nLOAD R E\n";
    assert!(elf_pie(secure));
    assert!(elf_full_relro(secure));
    assert!(!elf_has_executable_stack(secure));
    assert!(!elf_has_rwx_segment(secure));

    let insecure = "Type: EXEC\nGNU_STACK RWE\nLOAD RWE\n";
    assert!(!elf_pie(insecure));
    assert!(!elf_full_relro(insecure));
    assert!(elf_has_executable_stack(insecure));
    assert!(elf_has_rwx_segment(insecure));

    let config = QaConfig::default();
    let records = elf_records(Path::new("app"), &config, secure);
    assert_eq!(records.len(), 4);
    assert!(records.iter().all(|record| record.status == EvidenceStatus::Available));

    let records = elf_records(Path::new("app"), &config, insecure);
    assert!(records.iter().all(|record| record.status == EvidenceStatus::Failed));
}

#[cfg(target_os = "windows")]
#[test]
fn pe_mitigation_records_distinguish_present_and_absent_flags() {
    let path = Path::new("app.exe");
    let available = pe_mitigation(path, "ASLR", true, "PE DYNAMIC_BASE");
    let failed = pe_mitigation(path, "DEP", false, "PE NX_COMPAT");
    assert_eq!(available.status, EvidenceStatus::Available);
    assert_eq!(available.check, "ASLR");
    assert_eq!(failed.status, EvidenceStatus::Failed);
    assert_eq!(failed.check, "DEP");
    assert_eq!(pe_unknown(path).status, EvidenceStatus::Unknown);
}

#[test]
fn stderr_and_record_helpers_preserve_diagnostics() {
    assert_eq!(stderr(b"hello"), "hello");
    let item = record("check", EvidenceStatus::Available, Some(Path::new("app")), "ok");
    assert_eq!(item.family, "HARDEN");
    assert_eq!(item.check, "check");
    assert_eq!(item.source.as_deref(), Some("app"));
}

#[test]
fn run_with_classifies_build_failures_and_empty_success_artifacts() {
    let root = temp_dir("run-with");
    let config = QaConfig::default();

    let failed =
        run_with(&root, &config, true, || BuildOutcome::Failed("compile failed".into()), Vec::new);
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].status, EvidenceStatus::Failed);
    assert_eq!(failed[0].detail.as_deref(), Some("compile failed"));

    let unavailable = run_with(
        &root,
        &config,
        true,
        || BuildOutcome::Unavailable("cargo missing".into()),
        Vec::new,
    );
    assert_eq!(unavailable.len(), 1);
    assert_eq!(unavailable[0].status, EvidenceStatus::Unavailable);

    let success = run_with(&root, &config, true, || BuildOutcome::Success, Vec::new);
    assert_eq!(success.len(), 2);
    assert_eq!(success[0].status, EvidenceStatus::Available);
    assert_eq!(success[1].status, EvidenceStatus::Unknown);

    let missing = root.join("does-not-exist");
    let filtered = inspect_artifacts(&root, &config, vec![missing]);
    assert_eq!(filtered.len(), 2);
    assert_eq!(filtered[1].check, "artifacts");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn build_outcome_distinguishes_success_failure_and_spawn_error() {
    let root = temp_dir("build-outcome");
    let ok = super::super::process::run(&root, "rustc", &["--version".into()], &[]);
    assert!(matches!(build_outcome(ok), BuildOutcome::Success));

    let bad = super::super::process::run(
        &root,
        "rustc",
        &["--definitely-not-a-real-rustc-option".into()],
        &[],
    );
    match build_outcome(bad) {
        BuildOutcome::Failed(detail) => assert!(!detail.is_empty()),
        other => panic!("expected failed build, got {other:?}"),
    }

    let missing = super::super::process::run(
        &root,
        "definitely-not-a-real-universal-rust-qa-command",
        &[],
        &[],
    );
    match build_outcome(missing) {
        BuildOutcome::Unavailable(detail) => assert!(!detail.is_empty()),
        other => panic!("expected unavailable build, got {other:?}"),
    }
    fs::remove_dir_all(root).unwrap();
}

#[cfg(target_os = "windows")]
#[test]
fn pe_output_requires_success_and_emits_both_mitigation_records() {
    let root = temp_dir("pe-output");
    let path = root.join("app.exe");
    let mut success =
        super::super::process::run(&root, "rustc", &["--version".into()], &[]).unwrap();
    success.stdout = b"Dynamic Base\nNX Compatible\n".to_vec();
    let records = pe_output(&path, success);
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].check, "ASLR");
    assert_eq!(records[0].status, EvidenceStatus::Available);
    assert_eq!(records[1].check, "DEP");
    assert_eq!(records[1].status, EvidenceStatus::Available);

    let failed = super::super::process::run(
        &root,
        "rustc",
        &["--definitely-not-a-real-rustc-option".into()],
        &[],
    )
    .unwrap();
    let records = pe_output(&path, failed);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].check, "PE");
    assert_eq!(records[0].status, EvidenceStatus::Unknown);
    fs::remove_dir_all(root).unwrap();
}
