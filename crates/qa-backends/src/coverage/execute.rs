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
    DirectReport,
    Report,
}

impl TestMode {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Configured => "configured-features",
            Self::AllFeatures => "all-features",
            Self::DirectReport => "direct-report",
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
        TestMode::Default | TestMode::DirectReport | TestMode::Report => {}
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

pub(super) fn primary_direct_report_args(path: &Path) -> Vec<String> {
    vec![
        "llvm-cov".into(),
        "--json".into(),
        "--output-path".into(),
        path.display().to_string(),
    ]
}

pub(super) fn tolerant_direct_report_args(path: &Path) -> Vec<String> {
    let mut args = primary_direct_report_args(path);
    args.insert(1, "--ignore-run-fail".into());
    args
}

pub(super) fn workspace_direct_report_args(
    packages: &[CoveragePackage],
    target: Option<&str>,
    path: &Path,
) -> Vec<String> {
    let mut args = tolerant_direct_report_args(path);
    for package in packages {
        args.extend(["-p".into(), package.name.clone()]);
    }
    if let Some(target) = target {
        args.extend(["--target".into(), target.into(), "--coverage-target-only".into()]);
    }
    args
}

pub(super) fn direct_report_args(
    package: &CoveragePackage,
    target: Option<&str>,
    path: &Path,
) -> Vec<String> {
    let mut args = vec![
        "llvm-cov".into(),
        "--ignore-run-fail".into(),
        "-p".into(),
        package.name.clone(),
        "--json".into(),
        "--output-path".into(),
        path.display().to_string(),
    ];
    if let Some(target) = target {
        args.extend(["--target".into(), target.into(), "--coverage-target-only".into()]);
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
        crate::process::run_system_cargo(workspace, &args, env)
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
    if mode == TestMode::DirectReport {
        return "direct-report";
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
    if tooling_failure(&text) {
        "tooling".into()
    } else if unsupported_target_failure(&text) {
        "unsupported-target".into()
    } else if profile_merge_failure(&text) {
        "profile-merge".into()
    } else if native_build_failure(&text) {
        "environment-native-build".into()
    } else if test_failure(&text) {
        "test-failure".into()
    } else {
        "build-or-instrumentation".into()
    }
}

fn tooling_failure(text: &str) -> bool {
    ["no such command", "cargo-llvm-cov", "llvm-tools", "llvm-profdata", "llvm-cov"]
        .iter()
        .any(|needle| text.contains(needle))
        && ["not found", "missing", "unavailable", "no such command"]
            .iter()
            .any(|needle| text.contains(needle))
}

fn unsupported_target_failure(text: &str) -> bool {
    [
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
            .any(|needle| text.contains(needle)))
}

fn profile_merge_failure(text: &str) -> bool {
    [
        "failed to merge",
        "could not merge",
        "malformed profile",
        "profile version mismatch",
        "profile data may be out of date",
    ]
    .iter()
    .any(|needle| text.contains(needle))
}

fn native_build_failure(text: &str) -> bool {
    ["libclang", "clang-sys", "linker", "link.exe", "cmake", "librocksdb-sys", "rocksdb-sys"]
        .iter()
        .any(|needle| text.contains(needle))
        || (text.contains("bindgen") && text.contains("clang"))
}

fn test_failure(text: &str) -> bool {
    ["test result: failed", "test failed"].iter().any(|needle| text.contains(needle))
}

pub(super) fn primary_coverage_env() -> Vec<(&'static str, String)> {
    vec![("CARGO_LLVM_COV_SETUP", "yes".into())]
}

pub(super) fn coverage_env(target: &Path) -> Vec<(&'static str, String)> {
    let target = target.display().to_string();
    vec![
        ("CARGO_LLVM_COV_TARGET_DIR", target.clone()),
        ("CARGO_LLVM_COV_BUILD_DIR", target),
        ("CARGO_LLVM_COV_SETUP", "yes".into()),
    ]
}

pub(super) fn prepare_primary_coverage_output(output: &Path) -> Result<(), String> {
    fs::create_dir_all(output).map_err(|error| {
        format!("failed to create coverage output {}: {error}", output.display())
    })
}

pub(super) fn prepare_coverage_target(output: &Path) -> Result<PathBuf, String> {
    prepare_primary_coverage_output(output)?;
    for name in ["llvm-cov.json", MANIFEST_NAME] {
        let path = output.join(name);
        if path.exists() {
            fs::remove_file(&path).map_err(|error| {
                format!("failed to reset coverage evidence {}: {error}", path.display())
            })?;
        }
    }
    let target = output.join("llvm-cov-target");
    reset_directory(&target, "coverage target")?;
    reset_directory(&output.join("llvm-cov-rescue"), "coverage rescue target")?;
    Ok(target)
}

fn reset_directory(path: &Path, label: &str) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    fs::remove_dir_all(path)
        .map_err(|error| format!("failed to reset {label} {}: {error}", path.display()))
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
#[path = "execute/tests.rs"]
mod tests;
