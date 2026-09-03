use super::*;

fn package(name: &str) -> CoveragePackage {
    CoveragePackage {
        name: name.into(),
        root: format!("/workspace/{name}"),
        source_loc: 10,
        default_member: true,
    }
}

#[test]
fn default_is_first_and_all_features_is_an_additional_opt_in_configuration() {
    let packages = vec![package("a"), package("b")];
    let config = CoverageConfig::default();
    let args = test_args(&config, &packages, None, None, TestMode::Default);
    assert!(args.windows(2).any(|pair| pair[0] == "-p" && pair[1] == "a"));
    assert!(args.windows(2).any(|pair| pair[0] == "-p" && pair[1] == "b"));
    assert!(!args.iter().any(|arg| arg == "--all-features"));
    assert!(optional_modes(&config).is_empty());

    let config = CoverageConfig { all_features: true, ..CoverageConfig::default() };
    assert_eq!(optional_modes(&config), vec![TestMode::AllFeatures]);
    let args = test_args(
        &config,
        std::slice::from_ref(&packages[0]),
        Some(&packages[0]),
        None,
        TestMode::AllFeatures,
    );
    assert!(args.iter().any(|arg| arg == "--all-features"));
}

#[test]
fn configured_features_no_defaults_and_target_are_explicit_and_package_scoped() {
    let package = package("wallet");
    let config = CoverageConfig {
        features: vec!["serde".into(), "rpc".into()],
        no_default_features: true,
        ..CoverageConfig::default()
    };
    let args = test_args(
        &config,
        std::slice::from_ref(&package),
        Some(&package),
        Some("wasm32-unknown-unknown"),
        TestMode::Configured,
    );
    assert!(args.windows(2).any(|pair| pair[0] == "-p" && pair[1] == "wallet"));
    assert!(args.windows(2).any(|pair| pair[0] == "--features" && pair[1] == "serde,rpc"));
    assert!(args.iter().any(|arg| arg == "--no-default-features"));
    assert!(
        args.windows(2)
            .any(|pair| { pair[0] == "--target" && pair[1] == "wasm32-unknown-unknown" })
    );
    assert!(args.iter().any(|arg| arg == "--coverage-target-only"));
}

#[test]
fn failure_stages_separate_test_execution_from_instrumentation_and_reporting() {
    assert_eq!(
        attempt_stage(TestMode::Default, AttemptOutcome::Failed, Some("test-failure")),
        "test-execution"
    );
    assert_eq!(
        attempt_stage(TestMode::Default, AttemptOutcome::Failed, Some("environment-native-build")),
        "instrument-build"
    );
    assert_eq!(
        attempt_stage(TestMode::Default, AttemptOutcome::Failed, Some("unsupported-target")),
        "target-compatibility"
    );
    assert_eq!(
        attempt_stage(TestMode::DirectReport, AttemptOutcome::Failed, Some("profile-merge")),
        "direct-report"
    );
    assert_eq!(attempt_stage(TestMode::Report, AttemptOutcome::Failed, None), "report");
}

#[test]
fn diagnostics_distinguish_native_bindgen_target_test_and_build_failures() {
    assert_eq!(
        classify_failure("librocksdb-sys bindgen could not find libclang"),
        "environment-bindgen-clang"
    );
    assert_eq!(
        classify_failure(
            "failed to build archive: There is not enough space on the disk. (os error 112)"
        ),
        "resource-exhaustion"
    );
    assert_eq!(
        classify_failure("rustc-LLVM ERROR: IO failure on output stream: no space on device"),
        "resource-exhaustion"
    );
    assert_eq!(
        classify_failure("can't find crate for `std`; target may not be installed wasm32"),
        "unsupported-target"
    );
    assert_eq!(
        classify_failure("this crate only supports wasm32 on this build"),
        "unsupported-target"
    );
    assert_eq!(
        classify_failure("unresolved import wasm_bindgen::prelude"),
        "build-or-instrumentation"
    );
    assert_eq!(classify_failure("test result: FAILED. 1 failed"), "test-failure");
    assert_eq!(
        classify_failure("llvm-profdata failed to merge malformed profile"),
        "profile-merge"
    );
    assert_eq!(
        classify_failure("build script panicked at configuration"),
        "build-or-instrumentation"
    );
    assert_eq!(classify_failure("rustc exited with code 1"), "build-or-instrumentation");
    assert_eq!(classify_failure("cargo-llvm-cov not found"), "tooling");
}

#[test]
fn primary_direct_collection_matches_plain_manual_json_contract() {
    let path = Path::new("qa-out/llvm-cov.json");
    let args = primary_direct_report_args(path);
    assert_eq!(args, vec!["llvm-cov", "--json", "--output-path", "qa-out/llvm-cov.json"]);
    assert!(!args.iter().any(|arg| arg == "-p" || arg == "--workspace"));
    assert!(!args.iter().any(|arg| arg == "--ignore-run-fail"));
}

#[test]
fn tolerant_direct_collection_only_adds_ignore_run_fail_to_manual_contract() {
    let path = Path::new("qa-out/llvm-cov.json");
    assert_eq!(
        tolerant_direct_report_args(path),
        vec!["llvm-cov", "--ignore-run-fail", "--json", "--output-path", "qa-out/llvm-cov.json"]
    );
}

#[test]
fn resilient_direct_collection_reuses_instrumented_builds_and_tolerates_profiles() {
    let path = Path::new("qa-out/llvm-cov.json");
    let args = resilient_direct_report_args(path);
    assert!(args.iter().any(|arg| arg == "--ignore-run-fail"));
    assert!(args.iter().any(|arg| arg == "--no-clean"));
    assert!(args.windows(2).any(|pair| pair[0] == "--failure-mode" && pair[1] == "all"));
    assert!(
        args.windows(2)
            .any(|pair| { pair[0] == "--output-path" && pair[1] == "qa-out/llvm-cov.json" })
    );
}

#[test]
fn workspace_direct_recovery_matches_manual_json_collection_contract() {
    let path = Path::new("qa-out/workspace.json");
    let packages = [package("consensus"), package("wallet")];
    let args = workspace_direct_report_args(&packages, None, path);
    assert_eq!(args[0], "llvm-cov");
    assert!(args.iter().any(|arg| arg == "--ignore-run-fail"));
    assert!(!args.iter().any(|arg| arg == "--no-fail-fast"));
    assert!(args.iter().any(|arg| arg == "--json"));
    assert!(
        args.windows(2)
            .any(|pair| pair[0] == "--output-path" && pair[1] == "qa-out/workspace.json")
    );
    assert!(args.windows(2).any(|pair| pair[0] == "-p" && pair[1] == "consensus"));
    assert!(args.windows(2).any(|pair| pair[0] == "-p" && pair[1] == "wallet"));
    assert!(!args.iter().any(|arg| arg == "--no-report"));
    assert!(!args.iter().any(|arg| arg == "--no-clean"));
}

#[test]
fn direct_report_recovery_is_package_scoped_and_generates_json() {
    let package = package("wallet");
    let path = Path::new("qa-out/wallet.json");
    let args = direct_report_args(&package, Some("x86_64-pc-windows-msvc"), path);
    assert!(args.windows(2).any(|pair| pair[0] == "-p" && pair[1] == "wallet"));
    assert!(args.iter().any(|arg| arg == "--json"));
    assert!(
        args.windows(2)
            .any(|pair| { pair[0] == "--output-path" && pair[1] == "qa-out/wallet.json" })
    );
    assert!(
        args.windows(2)
            .any(|pair| { pair[0] == "--target" && pair[1] == "x86_64-pc-windows-msvc" })
    );
    assert!(args.iter().any(|arg| arg == "--coverage-target-only"));
    assert!(args.iter().any(|arg| arg == "--ignore-run-fail"));
    assert!(!args.iter().any(|arg| arg == "--no-fail-fast"));
    assert!(!args.iter().any(|arg| arg == "--no-report"));
    assert!(args.iter().any(|arg| arg == "--no-clean"));
    assert!(args.windows(2).any(|pair| pair[0] == "--failure-mode" && pair[1] == "all"));
}

#[test]
fn bindgen_host_pointer_mismatch_is_recognized_for_host_bindgen_retry() {
    let diagnostic = r#"error: failed to run custom build command for `librocksdb-sys`
thread 'main' panicked at bindgen-0.72.1\lib.rs:917:13:
assertion `left == right` failed: "x86_64-pc-windows-msvc"
  left: 4
 right: 8"#;
    assert!(bindgen_clang_environment_failure(diagnostic));
    assert!(bindgen_clang_environment_failure("bindgen could not find libclang"));
    assert!(!bindgen_clang_environment_failure("ordinary Rust compile failure"));
}

#[test]
fn report_generation_is_strict_first_and_tolerant_only_for_partial_recovery() {
    let strict = report_args(Path::new("qa-out/llvm-cov.json"), false);
    assert_eq!(strict[0], "llvm-cov");
    assert_eq!(strict[1], "report");
    assert!(!strict.iter().any(|arg| arg == "--failure-mode"));

    let tolerant = report_args(Path::new("qa-out/llvm-cov.json"), true);
    assert!(tolerant.windows(2).any(|pair| pair[0] == "--failure-mode" && pair[1] == "all"));
}

#[test]
fn profile_count_uses_cargo_llvm_covs_top_level_raw_profile_contract() {
    let root =
        std::env::temp_dir().join(format!("urqa-coverage-profile-count-{}", std::process::id()));
    match fs::remove_dir_all(&root) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => panic!("failed to reset coverage profile fixture: {error}"),
    }
    fs::create_dir_all(root.join("debug")).unwrap();
    fs::write(root.join("one.profraw"), b"one").unwrap();
    fs::write(root.join("two.profraw"), b"two").unwrap();
    fs::write(root.join("debug/irrelevant.profraw"), b"nested").unwrap();
    assert_eq!(count_profiles(&root), 2);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn environment_auto_provisions_llvm_tools_without_prompting_and_bounds_scratch_growth() {
    let env = coverage_env(Path::new("isolated-target"));
    assert!(env.iter().any(|(key, value)| *key == "CARGO_LLVM_COV_SETUP" && value == "yes"));
    assert!(env.iter().any(|(key, value)| *key == "CARGO_INCREMENTAL" && value == "0"));
}

#[test]
fn primary_output_setup_does_not_touch_existing_evidence_or_fallback_targets() {
    let root = std::env::temp_dir().join(format!(
        "urqa-primary-output-reset-{}-{}",
        std::process::id(),
        module_path!().replace("::", "-")
    ));
    match fs::remove_dir_all(&root) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => panic!("failed to reset primary output fixture: {error}"),
    }
    fs::create_dir_all(root.join("llvm-cov-target")).unwrap();
    fs::create_dir_all(root.join("llvm-cov-rescue")).unwrap();
    fs::write(root.join("llvm-cov-target/stale.profraw"), b"stale").unwrap();
    fs::write(root.join("llvm-cov-rescue/stale.profraw"), b"stale").unwrap();
    fs::write(root.join("llvm-cov.json"), b"stale").unwrap();
    fs::write(root.join(MANIFEST_NAME), b"stale").unwrap();

    prepare_primary_coverage_output(&root).unwrap();

    assert!(root.join("llvm-cov.json").is_file());
    assert!(root.join(MANIFEST_NAME).is_file());
    assert!(root.join("llvm-cov-target/stale.profraw").is_file());
    assert!(root.join("llvm-cov-rescue/stale.profraw").is_file());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn bindgen_retry_removes_ambient_clang_state_before_injecting_a_known_host_libclang() {
    let keys = bindgen_conflicting_env_keys_from(
        [
            "LIBCLANG_PATH".to_string(),
            "CLANG_PATH".to_string(),
            "BINDGEN_EXTRA_CLANG_ARGS".to_string(),
            "BINDGEN_EXTRA_CLANG_ARGS_x86_64-pc-windows-msvc".to_string(),
            "BINDGEN_EXTRA_CLANG_ARGS_x86_64_pc_windows_msvc".to_string(),
            "LLVM_CONFIG_PATH".to_string(),
            "UNRELATED".to_string(),
        ],
        true,
    );
    for expected in [
        "LIBCLANG_PATH",
        "CLANG_PATH",
        "LLVM_CONFIG_PATH",
        "BINDGEN_EXTRA_CLANG_ARGS",
        "BINDGEN_EXTRA_CLANG_ARGS_x86_64-pc-windows-msvc",
        "BINDGEN_EXTRA_CLANG_ARGS_x86_64_pc_windows_msvc",
    ] {
        assert!(keys.iter().any(|key| key == expected), "missing {expected}");
    }
    assert!(!keys.iter().any(|key| key == "UNRELATED"));
}

#[test]
fn non_windows_bindgen_retry_preserves_existing_clang_locators() {
    let keys = bindgen_conflicting_env_keys_from(
        [
            "LIBCLANG_PATH".to_string(),
            "CLANG_PATH".to_string(),
            "LLVM_CONFIG_PATH".to_string(),
            "BINDGEN_EXTRA_CLANG_ARGS".to_string(),
        ],
        false,
    );
    assert_eq!(keys, vec!["BINDGEN_EXTRA_CLANG_ARGS"]);
}
