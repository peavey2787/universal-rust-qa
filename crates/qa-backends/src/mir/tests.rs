use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(1);

fn temp_dir(name: &str) -> PathBuf {
    let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("urqa-mir-{name}-{}-{id}", std::process::id()));
    if path.exists() {
        fs::remove_dir_all(&path).unwrap();
    }
    fs::create_dir_all(&path).unwrap();
    path
}

fn source_function(root: &Path, source: &str, name: &str) -> qa_syntax::SourceFunction {
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/lib.rs"), source).unwrap();
    qa_syntax::discover(root).functions.into_iter().find(|function| function.name == name).unwrap()
}

#[test]
fn disabled_pending_and_empty_workspace_states_are_reported() {
    let root = temp_dir("states");
    let mut config = QaConfig::default();
    config.mir.mode = "off".into();
    assert_eq!(
        run(&root, &config, &root.join("qa-out"), true).records[0].status,
        EvidenceStatus::Disabled
    );
    config.mir.mode = "explicit".into();
    assert_eq!(
        run(&root, &config, &root.join("qa-out"), false).records[0].status,
        EvidenceStatus::Unknown
    );
    assert_eq!(
        run(&root, &config, &root.join("qa-out"), true).records[0].status,
        EvidenceStatus::NotApplicable
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn package_discovery_and_rustc_arguments_cover_library_binary_and_exclusions() {
    let root = temp_dir("packages");
    fs::create_dir_all(root.join("lib/src")).unwrap();
    fs::create_dir_all(root.join("bin/src")).unwrap();
    fs::create_dir_all(root.join("target/ignored/src")).unwrap();
    fs::write(root.join("lib/Cargo.toml"), "[package]\nname='libx'\nversion='0.1.0'\n").unwrap();
    fs::write(root.join("lib/src/lib.rs"), "pub fn x(){}\n").unwrap();
    fs::write(root.join("bin/Cargo.toml"), "[package]\nname='binx'\nversion='0.1.0'\n").unwrap();
    fs::write(root.join("bin/src/main.rs"), "fn main(){}\n").unwrap();
    fs::write(root.join("bin/Other.toml"), "[package]\nname='not-a-manifest'\nversion='0.1.0'\n")
        .unwrap();
    fs::write(
        root.join("target/ignored/Cargo.toml"),
        "[package]\nname='ignored'\nversion='0.1.0'\n",
    )
    .unwrap();
    fs::write(root.join("target/ignored/src/lib.rs"), "fn ignored(){}\n").unwrap();

    let packages = package_manifests(&root);
    assert_eq!(packages.len(), 2);
    let config = QaConfig::default();
    let lib = packages.iter().find(|package| package.name == "libx").unwrap();
    let bin = packages.iter().find(|package| package.name == "binx").unwrap();
    let lib_args = rustc_args(&config, lib);
    assert!(lib_args.iter().any(|arg| arg == "--lib"));
    let bin_args = rustc_args(&config, bin);
    assert!(bin_args.windows(2).any(|pair| pair[0] == "--bin" && pair[1] == "binx"));
    assert!(lib_args.ends_with(&["--".to_string(), "-Zunpretty=mir".to_string()]));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn mir_section_extracts_named_functions_without_bleeding_into_next_function() {
    let mir = "header\nfn alpha() -> () {\n  bb0: {}\n}\nfn beta() -> () {\n  bb0: {}\n}\n";
    let alpha = mir_section(mir, "alpha").unwrap();
    assert!(alpha.contains("fn alpha("));
    assert!(!alpha.contains("fn beta("));
    assert!(mir_section(mir, "missing").is_none());
}

#[test]
fn mir_rule_checks_emit_all_targeted_findings_and_honor_config_switches() {
    let root = temp_dir("rules");
    let function = source_function(
        &root,
        r#"
#[qa_attr::critical]
#[qa_attr::no_alloc]
#[qa_attr::hot_path]
#[qa_attr::secret]
#[qa_attr::critical_async]
async fn risky() { let secret: Vec<u8> = Vec::new(); zeroize(&secret); work().await; }
"#,
        "risky",
    );
    let section =
        "fn risky() { assert(_1); exchange_malloc; drop(_1); drop(_2); drop(_3); drop(_4); }";
    let config = QaConfig::default();
    let mut findings = Vec::new();
    analyze_function(&config, section, &function, &mut findings);
    let ids = findings.iter().map(|finding| finding.rule_id.as_str()).collect::<Vec<_>>();
    assert!(ids.contains(&"QA-MIR-003"));
    assert!(ids.contains(&"QA-MIR-004"));
    assert!(ids.contains(&"QA-MIR-001"));
    assert!(ids.contains(&"QA-MIR-002"));
    assert!(ids.contains(&"QA-MIR-005"));

    let mut disabled = config.clone();
    disabled.mir.check_panic_edges = false;
    disabled.mir.check_no_alloc = false;
    disabled.mir.check_drop_cleanup = false;
    disabled.mir.check_zeroization = false;
    disabled.mir.check_async_retention = false;
    findings.clear();
    analyze_function(&disabled, section, &function, &mut findings);
    assert!(findings.is_empty());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn analyze_text_correlates_source_functions_to_synthetic_mir() {
    let root = temp_dir("analyze");
    let source = r#"
#[qa_attr::no_alloc]
fn no_alloc() { let _ = Vec::<u8>::new(); }
fn ordinary() {}
"#;
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/lib.rs"), source).unwrap();
    let mir = "fn no_alloc() { _1 = exchange_malloc; }\nfn unrelated() {}\n";
    let mut findings = Vec::new();
    analyze_text(&root, &QaConfig::default(), mir, &mut findings);
    assert!(findings.iter().any(|finding| finding.rule_id == "QA-MIR-004"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn finish_mir_writes_aggregate_and_suite_status() {
    let root = temp_dir("finish");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("src/lib.rs"),
        "#[qa_attr::no_alloc]\nfn no_alloc() { let _ = Vec::<u8>::new(); }\n",
    )
    .unwrap();
    let out = root.join("qa-out/mir");
    fs::create_dir_all(&out).unwrap();
    let evidence = finish_mir(
        &root,
        &QaConfig::default(),
        &out,
        "fn no_alloc() { _1 = exchange_malloc; }\n".into(),
        Vec::new(),
        false,
    );
    assert_eq!(evidence.records.last().unwrap().status, EvidenceStatus::Available);
    assert!(evidence.findings.iter().any(|finding| finding.rule_id == "QA-MIR-004"));
    assert!(out.join("workspace.mir").exists());

    let failed = finish_mir(&root, &QaConfig::default(), &out, String::new(), Vec::new(), true);
    assert_eq!(failed.records.last().unwrap().status, EvidenceStatus::Failed);
    assert_eq!(stderr(b"diagnostic"), "diagnostic");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn emit_package_record_preserves_status_source_and_failure_signal() {
    let package = Package {
        manifest: PathBuf::from("pkg/Cargo.toml"),
        name: "pkg".into(),
        lib: true,
        bin: false,
    };
    let mut records = Vec::new();
    emit_package_record(&package, EvidenceStatus::Unavailable, "missing rustc", &mut records);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].status, EvidenceStatus::Unavailable);
    assert_eq!(records[0].check, "pkg");
    assert_eq!(records[0].source.as_deref(), Some("pkg/Cargo.toml"));
    assert_eq!(records[0].detail.as_deref(), Some("missing rustc"));
}

#[test]
fn mir_boundaries_distinguish_three_drops_from_four_and_partial_async_signals() {
    let root = temp_dir("boundaries");
    let hot = source_function(&root, "#[qa_attr::hot_path]\nfn hot() {}\n", "hot");
    let mut findings = Vec::new();
    check_drop_cleanup(
        &QaConfig::default(),
        "drop(_1); drop(_2); drop(_3);",
        &hot,
        "hot_path",
        &mut findings,
    );
    assert!(findings.is_empty());
    check_drop_cleanup(
        &QaConfig::default(),
        "drop(_1); drop(_2); drop(_3); drop(_4);",
        &hot,
        "hot_path",
        &mut findings,
    );
    assert_eq!(findings.iter().filter(|finding| finding.rule_id == "QA-MIR-001").count(), 1);

    let async_only = source_function(
        &root,
        "#[qa_attr::critical_async]\nasync fn async_only() { work().await; }\nasync fn work() {}\n",
        "async_only",
    );
    findings.clear();
    check_async_retention(&QaConfig::default(), &async_only, "critical_async", &mut findings);
    assert!(findings.is_empty());

    let retained_only = source_function(
        &root,
        "#[qa_attr::critical_async]\nasync fn retained_only() { let secret = String::new(); }\n",
        "retained_only",
    );
    findings.clear();
    check_async_retention(&QaConfig::default(), &retained_only, "critical_async", &mut findings);
    assert!(findings.is_empty());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn emit_package_output_success_and_failure_have_opposite_side_effects() {
    let package = Package {
        manifest: PathBuf::from("pkg/Cargo.toml"),
        name: "pkg".into(),
        lib: true,
        bin: false,
    };
    let success = std::process::Command::new("rustc").arg("--version").output().unwrap();
    let failure =
        std::process::Command::new("rustc").arg("--definitely-invalid-option").output().unwrap();
    let mut aggregate = String::new();
    let mut records = Vec::new();
    emit_package_output(&package, success, &mut aggregate, &mut records);
    assert!(aggregate.contains("pkg/Cargo.toml"));
    assert_eq!(records.last().unwrap().status, EvidenceStatus::Available);

    aggregate.clear();
    records.clear();
    emit_package_output(&package, failure, &mut aggregate, &mut records);
    assert!(aggregate.is_empty());
    assert_eq!(records.last().unwrap().status, EvidenceStatus::Failed);
}

#[test]
fn empty_mir_does_not_create_an_artifact_and_failure_state_is_preserved() {
    let root = temp_dir("empty-finish");
    let out = root.join("mir");
    fs::create_dir_all(&out).unwrap();
    let evidence = finish_mir(&root, &QaConfig::default(), &out, String::new(), Vec::new(), false);
    assert!(!out.join("workspace.mir").exists());
    assert_eq!(evidence.records.last().unwrap().status, EvidenceStatus::Available);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn package_failure_detection_requires_a_new_available_record() {
    let available = record("pkg", EvidenceStatus::Available, None, "ok");
    let failed = record("pkg", EvidenceStatus::Failed, None, "bad");
    assert!(package_failed(0, &[]));
    assert!(!package_failed(0, &[available.clone()]));
    assert!(package_failed(1, &[available]));
    assert!(package_failed(0, &[failed]));
}

#[test]
fn emit_package_always_records_an_invalid_manifest_attempt() {
    let root = temp_dir("emit-invalid");
    let package = Package {
        manifest: root.join("missing/Cargo.toml"),
        name: "missing".into(),
        lib: true,
        bin: false,
    };
    let mut aggregate = String::new();
    let mut records = Vec::new();
    emit_package(&root, &QaConfig::default(), &package, &mut aggregate, &mut records);
    assert_eq!(records.len(), 1);
    assert_ne!(records[0].status, EvidenceStatus::Available);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn execute_packages_reports_ready_and_output_directory_failure_paths() {
    let root = temp_dir("execute-packages");
    let artifact_root = root.join("qa-out");
    let ready = execute_packages(&root, &QaConfig::default(), &artifact_root, Vec::new());
    assert_eq!(ready.records.last().unwrap().status, EvidenceStatus::Available);
    assert!(artifact_root.join("mir").is_dir());

    let blocked = root.join("blocked");
    fs::write(&blocked, "not a directory").unwrap();
    let failed = execute_packages(&root, &QaConfig::default(), &blocked, Vec::new());
    assert_eq!(failed.records.len(), 1);
    assert_eq!(failed.records[0].status, EvidenceStatus::Failed);
    assert_eq!(failed.records[0].check, "output-directory");
    fs::remove_dir_all(root).unwrap();
}
