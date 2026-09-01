use super::*;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(1);

fn temp_dir(name: &str) -> PathBuf {
    let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("urqa-perf-{name}-{}-{id}", std::process::id()));
    if path.exists() {
        fs::remove_dir_all(&path).unwrap();
    }
    fs::create_dir_all(&path).unwrap();
    path
}

fn metric(name: &str, attrs: &[&str]) -> FunctionMetric {
    FunctionMetric {
        path: "src/lib.rs".into(),
        name: name.into(),
        qualified_name: name.into(),
        line: 1,
        end_line: 2,
        logical_loc: 2,
        statements: 1,
        parameters: 0,
        generic_parameters: 0,
        cyclomatic: 1,
        cognitive: 0,
        coverage_percent: Some(100.0),
        crap: Some(1.0),
        is_test: false,
        is_public: false,
        is_async: false,
        attributes: attrs.iter().map(|value| (*value).to_string()).collect(),
    }
}

#[test]
fn disabled_pending_and_hot_function_selection_are_precise() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut config = QaConfig::default();
    assert_eq!(run(root, &config, true, false, &[])[0].status, EvidenceStatus::Disabled);
    config.performance.enabled = true;
    assert_eq!(run(root, &config, false, false, &[])[0].status, EvidenceStatus::Unknown);

    let cold = metric("cold", &[]);
    let hot = metric("hot", &["qa_attr :: hot_path"]);
    let vector = metric("vector", &["qa_attr :: vectorize_expected"]);
    let functions = [cold, hot, vector];
    let selected = hot_functions(&functions);
    assert_eq!(selected.len(), 2);
    assert!(hot_path_header(&selected).is_empty());
    assert_eq!(hot_path_header(&[])[0].status, EvidenceStatus::NotApplicable);
}

#[test]
fn vectorization_and_instruction_parsers_distinguish_evidence() {
    assert_eq!(instruction_count("label:\n  mov eax, ebx\n.comment\n; note\n  add eax, 1\n"), 2);
    assert!(simd_evidence("vaddps ymm0, ymm1, ymm2"));
    assert!(simd_evidence("NEON q0"));
    assert!(!simd_evidence("mov eax, ebx"));

    let vector = metric("vector", &["vectorize_expected"]);
    let mut output = Vec::new();
    add_vectorization_evidence(&vector, "vaddps ymm0, ymm1, ymm2", &mut output);
    assert_eq!(output[0].status, EvidenceStatus::Available);
    output.clear();
    add_vectorization_evidence(&vector, "mov eax, ebx", &mut output);
    assert_eq!(output[0].status, EvidenceStatus::Failed);

    output.clear();
    add_vectorization_evidence(&metric("cold", &[]), "vaddps ymm0", &mut output);
    assert!(output.is_empty());
}

#[test]
fn percent_and_drift_rules_enforce_both_instruction_and_binary_limits() {
    let config = QaConfig::default();
    assert_eq!(percent_delta(0, 100), 0.0);
    assert_eq!(percent_delta(100, 125), 25.0);
    assert_eq!(percent_delta(100, 50), -50.0);

    let path = Path::new("baseline.json");
    assert_eq!(
        instruction_drift_record(path, &config, "f", 100, 110).status,
        EvidenceStatus::Available
    );
    assert_eq!(
        instruction_drift_record(path, &config, "f", 100, 200).status,
        EvidenceStatus::Failed
    );

    assert_eq!(
        binary_drift_record(path, &config, "app", 1_000_000, 1_010_000).status,
        EvidenceStatus::Available
    );
    assert_eq!(
        binary_drift_record(path, &config, "app", 1_000_000, 2_000_000).status,
        EvidenceStatus::Failed
    );
    assert_eq!(
        binary_drift_record(path, &config, "app", 2_000_000, 1_000_000).status,
        EvidenceStatus::Available
    );
}

#[test]
fn instruction_baseline_write_compare_and_missing_paths_are_covered() {
    let root = temp_dir("instruction");
    let path = root.join("qa/performance.json");
    let mut counts = BTreeMap::new();
    counts.insert("hot".to_string(), 100usize);
    let mut output = Vec::new();
    write_instruction_baseline(&path, &counts, &mut output);
    assert_eq!(output[0].status, EvidenceStatus::Available);
    assert_eq!(read_json_map::<usize>(&path).unwrap()["hot"], 100);

    let config = QaConfig::default();
    output.clear();
    compare_instruction_baseline(&path, &config, &counts, &mut output);
    assert_eq!(output.len(), 1);
    assert_eq!(output[0].status, EvidenceStatus::Available);

    output.clear();
    compare_instruction_baseline(&root.join("missing.json"), &config, &counts, &mut output);
    assert_eq!(output[0].status, EvidenceStatus::Unknown);

    output.clear();
    instruction_baseline(&root, &config, false, &BTreeMap::new(), &mut output);
    assert!(output.is_empty());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn binary_baseline_write_compare_and_json_failure_are_covered() {
    let root = temp_dir("binary");
    let path = root.join("qa/bloat.json");
    let mut sizes = BTreeMap::new();
    sizes.insert("app".to_string(), 1000u64);
    let mut output = Vec::new();
    write_binary_baseline(&path, &sizes, &mut output);
    assert_eq!(output[0].status, EvidenceStatus::Available);
    assert_eq!(read_json_map::<u64>(&path).unwrap()["app"], 1000);

    let config = QaConfig::default();
    output.clear();
    compare_binary_baseline(&path, &config, &sizes, &mut output);
    assert_eq!(output[0].status, EvidenceStatus::Available);

    output.clear();
    compare_binary_baseline(&root.join("missing.json"), &config, &sizes, &mut output);
    assert_eq!(output[0].status, EvidenceStatus::Unknown);

    fs::write(&path, "not-json").unwrap();
    assert!(read_json_map::<u64>(&path).is_none());
    assert!(read_json_map::<u64>(&root.join("absent")).is_none());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn tool_and_record_helpers_capture_process_status() {
    let root = temp_dir("tool");
    assert_eq!(
        tool(&root, "P", "rustc", "rustc", &["--version"]).status,
        EvidenceStatus::Available
    );
    assert_eq!(
        tool(&root, "P", "bad", "rustc", &["--definitely-invalid-option"]).status,
        EvidenceStatus::Failed
    );
    assert_eq!(
        tool(&root, "P", "missing", "urqa-program-that-does-not-exist", &[]).status,
        EvidenceStatus::Unavailable
    );
    assert_eq!(stderr(b"abcdef", 3), "abc");
    let item = record("P", "x", EvidenceStatus::Available, Some(Path::new("p")), "ok");
    assert_eq!((item.family.as_str(), item.check.as_str()), ("P", "x"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn decomposed_performance_records_preserve_status_and_identity() {
    let function = metric("hot", &["qa_attr :: hot_path"]);
    let failed = asm_record(&function, EvidenceStatus::Failed, "asm failed");
    assert_eq!(failed.family, "PERF");
    assert_eq!(failed.check, "asm:hot");
    assert_eq!(failed.status, EvidenceStatus::Failed);
    assert_eq!(failed.source.as_deref(), Some("src/lib.rs"));
    assert_eq!(failed.detail.as_deref(), Some("asm failed"));

    let missing = no_binary_size_record();
    assert_eq!(missing.family, "BLOAT");
    assert_eq!(missing.check, "binary-size");
    assert_eq!(missing.status, EvidenceStatus::NotApplicable);
}

#[test]
fn asm_result_helpers_distinguish_spawn_failure_command_failure_and_success() {
    let function = metric("hot", &["qa_attr :: hot_path"]);
    let mut counts = BTreeMap::new();
    let mut output = Vec::new();

    inspect_asm_result(
        &function,
        Err(std::io::Error::new(std::io::ErrorKind::NotFound, "cargo-asm missing")),
        &mut counts,
        &mut output,
    );
    assert!(counts.is_empty());
    assert_eq!(output.len(), 1);
    assert_eq!(output[0].status, EvidenceStatus::Unavailable);
    assert!(output[0].detail.as_deref().is_some_and(|detail| detail.contains("cargo-asm")));

    output.clear();
    let root = temp_dir("asm-output");
    let success = super::super::process::run(&root, "rustc", &["--version".into()], &[]).unwrap();
    inspect_asm_output(&function, success, &mut counts, &mut output);
    assert!(counts.contains_key("hot"));
    assert!(output.is_empty());

    let failed = super::super::process::run(
        &root,
        "rustc",
        &["--definitely-not-a-real-rustc-option".into()],
        &[],
    )
    .unwrap();
    inspect_asm_output(&function, failed, &mut counts, &mut output);
    assert_eq!(output.len(), 1);
    assert_eq!(output[0].status, EvidenceStatus::Failed);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn binary_bloat_wrappers_update_compare_and_report_missing_binaries() {
    let root = temp_dir("bloat-wrapper");
    let mut config = QaConfig::default();
    config.bloat.baseline_path = "qa/bloat-wrapper.json".into();
    let mut current = BTreeMap::new();
    current.insert("app".to_string(), 1_000u64);
    let mut output = Vec::new();

    binary_bloat_current(&root, &config, true, &current, &mut output);
    assert_eq!(output.len(), 1);
    assert_eq!(output[0].status, EvidenceStatus::Available);
    assert!(root.join("qa/bloat-wrapper.json").is_file());

    output.clear();
    binary_bloat_current(&root, &config, false, &current, &mut output);
    assert_eq!(output.len(), 1);
    assert_eq!(output[0].status, EvidenceStatus::Available);

    output.clear();
    binary_bloat(&root, &config, false, &mut output);
    assert_eq!(output.len(), 1);
    assert_eq!(output[0].status, EvidenceStatus::NotApplicable);
    assert!(current_binary_sizes(&root).is_empty());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn performance_request_and_threshold_edges_are_exact() {
    assert!(!performance_requested(false, false));
    assert!(performance_requested(true, false));
    assert!(performance_requested(false, true));
    assert!(performance_requested(true, true));

    let mut config = QaConfig::default();
    config.performance.instruction_deny_percent = 25.0;
    let baseline = Path::new("baseline.json");
    assert_eq!(
        instruction_drift_record(baseline, &config, "hot", 100, 125).status,
        EvidenceStatus::Available
    );
    assert_eq!(
        instruction_drift_record(baseline, &config, "hot", 100, 126).status,
        EvidenceStatus::Failed
    );

    config.bloat.max_percent_growth = 5.0;
    config.bloat.max_absolute_growth_bytes = 40;
    assert_eq!(
        binary_drift_record(baseline, &config, "app", 1_000, 1_050).status,
        EvidenceStatus::Available
    );
    assert_eq!(
        binary_drift_record(baseline, &config, "app", 1_000, 1_051).status,
        EvidenceStatus::Failed
    );
    config.bloat.max_absolute_growth_bytes = 100;
    assert_eq!(
        binary_drift_record(baseline, &config, "app", 1_000, 1_100).status,
        EvidenceStatus::Available
    );
    assert_eq!(
        binary_drift_record(baseline, &config, "app", 1_000, 1_101).status,
        EvidenceStatus::Failed
    );
}

#[test]
fn performance_wrappers_have_observable_side_effects() {
    let root = temp_dir("wrapper-observability");
    let mut config = QaConfig::default();
    config.performance.enabled = true;
    config.performance.baseline_path = "qa/performance.json".into();

    let mut counts = BTreeMap::new();
    counts.insert("hot".to_string(), 10usize);
    let mut output = Vec::new();
    instruction_baseline(&root, &config, true, &counts, &mut output);
    assert!(root.join("qa/performance.json").is_file());
    assert_eq!(output.len(), 1);

    output.clear();
    add_tool_evidence(&root, &config, false, &mut output);
    assert_eq!(output.len(), 3);
    assert_eq!(output[0].check, "cargo-bloat");
    assert_eq!(output[1].check, "binary-size");
    assert_eq!(output[2].check, "cargo-llvm-lines");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn current_binary_sizes_reads_exact_release_artifact_size() {
    let root = temp_dir("binary-sizes");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname='size-fixture'\nversion='0.1.0'\nedition='2021'\n",
    )
    .unwrap();
    fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
    let target = root.join("target/release");
    fs::create_dir_all(&target).unwrap();
    let name = if cfg!(windows) { "size-fixture.exe" } else { "size-fixture" };
    fs::write(target.join(name), b"1234567").unwrap();
    let sizes = current_binary_sizes(&root);
    assert_eq!(sizes.get(name), Some(&7));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn hot_path_inspection_cannot_be_a_silent_noop() {
    let root = temp_dir("hot-path-wrapper");
    let function = metric("definitely_missing_symbol", &["qa_attr :: hot_path"]);
    let mut counts = BTreeMap::new();
    let mut output = Vec::new();
    inspect_hot_path(&root, &function, &mut counts, &mut output);
    assert!(counts.contains_key("definitely_missing_symbol") || !output.is_empty());
    fs::remove_dir_all(root).unwrap();
}
