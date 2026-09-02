use super::{
    manifest::MANIFEST_NAME,
    model::{AttemptOutcome, AttemptResult, CoverageAttempt, CoveragePackage},
};
use qa_policy::CoverageConfig;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TestMode {
    Default,
    Configured,
    AllFeatures,
    Report,
}

impl TestMode {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Configured => "configured-features",
            Self::AllFeatures => "all-features",
            Self::Report => "merged-report",
        }
    }
}

pub(super) fn optional_modes(config: &CoverageConfig) -> Vec<TestMode> {
    let mut modes = Vec::new();
    if !config.features.is_empty() || config.no_default_features {
        modes.push(TestMode::Configured);
    }
    if config.all_features {
        modes.push(TestMode::AllFeatures);
    }
    modes
}

pub(super) fn target_variants(config: &CoverageConfig) -> Vec<Option<String>> {
    if config.targets.is_empty() {
        vec![None]
    } else {
        config.targets.iter().cloned().map(Some).collect()
    }
}

pub(super) fn test_args(
    config: &CoverageConfig,
    packages: &[CoveragePackage],
    package: Option<&CoveragePackage>,
    target: Option<&str>,
    mode: TestMode,
) -> Vec<String> {
    let mut args =
        vec!["llvm-cov".into(), "--no-report".into(), "--no-clean".into(), "--no-fail-fast".into()];
    if let Some(package) = package {
        args.extend(["-p".into(), package.name.clone()]);
    } else {
        for package in packages {
            args.extend(["-p".into(), package.name.clone()]);
        }
    }
    match mode {
        TestMode::Configured => {
            if config.no_default_features {
                args.push("--no-default-features".into());
            }
            if !config.features.is_empty() {
                args.extend(["--features".into(), config.features.join(",")]);
            }
        }
        TestMode::AllFeatures => args.push("--all-features".into()),
        TestMode::Default | TestMode::Report => {}
    }
    if let Some(target) = target {
        args.extend(["--target".into(), target.into(), "--coverage-target-only".into()]);
    }
    args
}

pub(super) fn report_args(path: &Path, tolerant: bool) -> Vec<String> {
    let mut args = vec![
        "llvm-cov".into(),
        "report".into(),
        "--json".into(),
        "--output-path".into(),
        path.display().to_string(),
    ];
    if tolerant {
        args.extend(["--failure-mode".into(), "all".into()]);
    }
    args
}

pub(super) struct AttemptSpec<'a> {
    pub package: Option<&'a str>,
    pub target: Option<&'a str>,
    pub configuration: &'a str,
    pub mode: TestMode,
    pub args: Vec<String>,
}

pub(super) fn run_attempt(
    workspace: &Path,
    target_dir: &Path,
    env: &[(&str, String)],
    spec: AttemptSpec<'_>,
) -> CoverageAttempt {
    let AttemptSpec { package, target, configuration, mode, args } = spec;
    let before = count_profiles(target_dir);
    let result = crate::process::with_cargo_target_dir(None, || {
        crate::process::run(workspace, "cargo", &args, env)
    });
    let after = count_profiles(target_dir);
    let result = classify_result(result);
    let category = if result.outcome == AttemptOutcome::Unavailable {
        Some("tooling".into())
    } else {
        result.diagnostic.as_deref().map(classify_failure)
    };
    let stage = attempt_stage(mode, result.outcome, category.as_deref());
    CoverageAttempt {
        package: package.map(str::to_string),
        target: target.map(str::to_string),
        configuration: configuration.into(),
        features: configured_features(&args, mode),
        no_default_features: args.iter().any(|arg| arg == "--no-default-features"),
        all_features: args.iter().any(|arg| arg == "--all-features"),
        command: std::iter::once("cargo".to_string()).chain(args).collect(),
        exit_code: result.exit_code,
        stage: stage.into(),
        outcome: result.outcome.label().into(),
        category,
        profiles_before: before,
        profiles_after: after,
        diagnostic: result.diagnostic,
    }
}

fn attempt_stage(mode: TestMode, outcome: AttemptOutcome, category: Option<&str>) -> &'static str {
    if mode == TestMode::Report {
        return "report";
    }
    if outcome == AttemptOutcome::Success || category == Some("test-failure") {
        return "test-execution";
    }
    match category {
        Some("tooling") => "tooling",
        Some("unsupported-target") => "target-compatibility",
        _ => "instrument-build",
    }
}

fn configured_features(args: &[String], mode: TestMode) -> Vec<String> {
    if mode != TestMode::Configured {
        return vec![];
    }
    args.windows(2)
        .find(|pair| pair[0] == "--features")
        .map(|pair| pair[1].split(',').map(str::to_string).collect())
        .unwrap_or_default()
}

fn classify_result(result: std::io::Result<std::process::Output>) -> AttemptResult {
    match result {
        Ok(output) if output.status.success() => AttemptResult {
            outcome: AttemptOutcome::Success,
            exit_code: output.status.code(),
            diagnostic: None,
        },
        Ok(output) => {
            let diagnostic = crate::process::diagnostics(&output.stdout, &output.stderr);
            let unavailable = classify_failure(&diagnostic) == "tooling";
            AttemptResult {
                outcome: if unavailable {
                    AttemptOutcome::Unavailable
                } else {
                    AttemptOutcome::Failed
                },
                exit_code: output.status.code(),
                diagnostic: Some(diagnostic),
            }
        }
        Err(error) => AttemptResult {
            outcome: AttemptOutcome::Unavailable,
            exit_code: None,
            diagnostic: Some(error.to_string()),
        },
    }
}

pub(super) fn classify_failure(diagnostic: &str) -> String {
    let text = diagnostic.to_ascii_lowercase();
    if ["no such command", "cargo-llvm-cov", "llvm-tools", "llvm-profdata", "llvm-cov"]
        .iter()
        .any(|needle| text.contains(needle))
        && ["not found", "missing", "unavailable", "no such command"]
            .iter()
            .any(|needle| text.contains(needle))
    {
        return "tooling".into();
    }
    let unsupported_target = [
        "target may not be installed",
        "target is not installed",
        "can't find crate for `std`",
        "unsupported target",
        "does not support target",
    ]
    .iter()
    .any(|needle| text.contains(needle))
        || (text.contains("wasm32")
            && [
                "only supports wasm32",
                "only supported on wasm32",
                "requires wasm32",
                "not supported on non-wasm",
                "compile for wasm32",
            ]
            .iter()
            .any(|needle| text.contains(needle)));
    if unsupported_target {
        return "unsupported-target".into();
    }
    if [
        "failed to merge",
        "could not merge",
        "malformed profile",
        "profile version mismatch",
        "profile data may be out of date",
    ]
    .iter()
    .any(|needle| text.contains(needle))
    {
        return "profile-merge".into();
    }
    let native_build =
        ["libclang", "clang-sys", "linker", "link.exe", "cmake", "librocksdb-sys", "rocksdb-sys"]
            .iter()
            .any(|needle| text.contains(needle))
            || (text.contains("bindgen") && text.contains("clang"));
    if native_build {
        return "environment-native-build".into();
    }
    if ["test result: failed", "test failed"].iter().any(|needle| text.contains(needle)) {
        return "test-failure".into();
    }
    "build-or-instrumentation".into()
}

pub(super) fn coverage_env(target: &Path) -> Vec<(&'static str, String)> {
    let target = target.display().to_string();
    vec![
        ("CARGO_LLVM_COV_TARGET_DIR", target.clone()),
        ("CARGO_LLVM_COV_BUILD_DIR", target),
        ("CARGO_LLVM_COV_SETUP", "yes".into()),
    ]
}

pub(super) fn prepare_coverage_target(output: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(output).map_err(|error| {
        format!("failed to create coverage output {}: {error}", output.display())
    })?;
    for name in ["llvm-cov.json", MANIFEST_NAME] {
        let path = output.join(name);
        if path.exists() {
            fs::remove_file(&path).map_err(|error| {
                format!("failed to reset coverage evidence {}: {error}", path.display())
            })?;
        }
    }
    let target = output.join("llvm-cov-target");
    if target.exists() {
        fs::remove_dir_all(&target).map_err(|error| {
            format!("failed to reset coverage target {}: {error}", target.display())
        })?;
    }
    Ok(target)
}

pub(super) fn count_profiles(target: &Path) -> usize {
    fs::read_dir(target)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.file_type().is_ok_and(|kind| kind.is_file())
                && entry.path().extension().is_some_and(|extension| extension == "profraw")
        })
        .count()
}

#[cfg(test)]
mod tests {
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
            attempt_stage(
                TestMode::Default,
                AttemptOutcome::Failed,
                Some("environment-native-build")
            ),
            "instrument-build"
        );
        assert_eq!(
            attempt_stage(TestMode::Default, AttemptOutcome::Failed, Some("unsupported-target")),
            "target-compatibility"
        );
        assert_eq!(attempt_stage(TestMode::Report, AttemptOutcome::Failed, None), "report");
    }

    #[test]
    fn diagnostics_distinguish_native_bindgen_target_test_and_build_failures() {
        assert_eq!(
            classify_failure("librocksdb-sys bindgen could not find libclang"),
            "environment-native-build"
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
        let root = std::env::temp_dir()
            .join(format!("urqa-coverage-profile-count-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("debug")).unwrap();
        fs::write(root.join("one.profraw"), b"one").unwrap();
        fs::write(root.join("two.profraw"), b"two").unwrap();
        fs::write(root.join("debug/irrelevant.profraw"), b"nested").unwrap();
        assert_eq!(count_profiles(&root), 2);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn environment_auto_provisions_llvm_tools_without_prompting() {
        let env = coverage_env(Path::new("isolated-target"));
        assert!(env.iter().any(|(key, value)| *key == "CARGO_LLVM_COV_SETUP" && value == "yes"));
    }
}
