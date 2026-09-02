use qa_sdk::{
    QaRunLayout, RunControl, RunOptions, run_workspace, run_workspace_with_options,
    run_workspace_with_options_and_layout, run_workspace_with_progress,
    run_workspace_with_progress_and_layout,
};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_WORKSPACE_ID: AtomicU64 = AtomicU64::new(1);

fn temp_workspace(name: &str) -> PathBuf {
    let id = NEXT_WORKSPACE_ID.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("urqa-sdk-{name}-{}-{id}", std::process::id()));
    if root.exists() {
        fs::remove_dir_all(&root).unwrap();
    }
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("src/lib.rs"),
        "pub fn add(a: u32, b: u32) -> u32 { a.saturating_add(b) }\n#[test] fn test_add(){ assert_eq!(add(1,2),3); }\n",
    )
    .unwrap();
    root
}

#[test]
fn sdk_default_run_honors_explicitly_disabled_coverage_and_writes_reports() {
    let root = temp_workspace("default");
    fs::write(
        root.join("qa.toml"),
        "schema = 1\nprofile = \"strict\"\noutput_dir = \"qa-out\"\n\n[coverage]\nmode = \"off\"\n",
    )
    .unwrap();
    let run = run_workspace(&root).unwrap();
    assert_eq!(run.config.profile, "strict");
    assert_eq!(run.report.schema, 21);
    assert!(run.report.functions.iter().any(|function| function.name == "add"));
    assert!(run.output_dir.join("report.json").is_file());
    assert!(run.output_dir.join("summary.txt").is_file());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn sdk_explicit_options_roundtrip_into_engine_without_forcing_external_backends() {
    let root = temp_workspace("options");
    let options = RunOptions {
        run_concurrency: false,
        run_constant_time: false,
        run_differential: false,
        run_fault: false,
        run_mir: false,
        run_platform: false,
        run_hardware: false,
        run_performance: false,
        run_hardening: false,
        run_release: false,
        run_self_hardening: false,
        ..RunOptions::existing_coverage()
    };
    let run = run_workspace_with_options(&root, &options).unwrap();
    assert!(run.report.evidence.iter().any(|record| record.family == "COV"));
    assert!(run.report.evidence.iter().any(|record| record.family == "MUT"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn sdk_errors_preserve_display_and_source_chains() {
    use std::error::Error;

    let io: qa_sdk::QaSdkError = std::io::Error::other("io failure").into();
    assert!(matches!(io, qa_sdk::QaSdkError::Io(_)));
    assert!(io.to_string().contains("io failure"));
    assert!(io.source().is_some());

    let config_error = qa_policy::ConfigError::Read(
        PathBuf::from("qa.toml"),
        std::io::Error::new(std::io::ErrorKind::NotFound, "missing"),
    );
    let config: qa_sdk::QaSdkError = config_error.into();
    assert!(matches!(config, qa_sdk::QaSdkError::Config(_)));
    assert!(config.to_string().contains("qa.toml"));
    assert!(config.source().is_some());
}

#[test]
fn sdk_external_layout_keeps_reports_and_transient_state_outside_workspace() {
    let root = temp_workspace("external");
    let outside = temp_workspace("external-state");
    let state = outside.join("state");
    let layout = QaRunLayout {
        state_dir: state.clone(),
        artifact_root: state.clone(),
        reports_dir: outside.join("reports"),
        coverage_dir: state.join("coverage"),
        mutation_dir: state.join("mutations"),
        cargo_target_dir: Some(state.join("build/target")),
    };
    let options = RunOptions::existing_coverage();
    let run = run_workspace_with_options_and_layout(&root, &options, &layout).unwrap();

    assert_eq!(run.layout, layout);
    assert_eq!(run.output_dir, outside.join("reports"));
    assert!(run.output_dir.join("report.json").is_file());
    assert!(run.output_dir.join("summary.txt").is_file());
    assert!(state.join("coverage").is_dir());
    assert!(state.join("build/target").is_dir());
    assert!(!root.join("qa-out").exists());
    assert!(!root.join("mutants.out").exists());
    assert!(!root.join("target").exists());
    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(outside).unwrap();
}

#[test]
fn sdk_progress_wrappers_complete_local_and_external_runs() {
    let root = temp_workspace("progress");
    let options = RunOptions::existing_coverage();
    let control = RunControl::new(qa_sdk::RUN_CATEGORY_COUNT);
    let local = run_workspace_with_progress(&root, &options, &control).unwrap();
    assert!(local.output_dir.join("report.json").is_file());
    let snapshot = control.snapshot();
    assert!(!snapshot.running);
    assert_eq!(snapshot.category, "complete");

    let outside = temp_workspace("progress-external");
    let state = outside.join("state");
    let layout = QaRunLayout {
        state_dir: state.clone(),
        artifact_root: state.clone(),
        reports_dir: outside.join("reports"),
        coverage_dir: state.join("coverage"),
        mutation_dir: state.join("mutations"),
        cargo_target_dir: Some(state.join("build/target")),
    };
    let external_control = RunControl::new(qa_sdk::RUN_CATEGORY_COUNT);
    let external =
        run_workspace_with_progress_and_layout(&root, &options, &layout, &external_control)
            .unwrap();
    assert_eq!(external.layout, layout);
    assert!(external.output_dir.join("summary.txt").is_file());
    assert!(!external_control.snapshot().running);

    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(outside).unwrap();
}
