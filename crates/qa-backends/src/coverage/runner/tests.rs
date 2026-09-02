use super::super::model::{CoverageAttempt, CoverageManifest};
use super::*;
use qa_model::EvidenceStatus;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(1);

fn temp_dir(name: &str) -> PathBuf {
    let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir()
        .join(format!("urqa-progressive-coverage-{name}-{}-{id}", std::process::id()));
    if path.exists() {
        fs::remove_dir_all(&path).unwrap();
    }
    fs::create_dir_all(&path).unwrap();
    path
}

fn attempt(
    package: Option<&str>,
    configuration: &str,
    outcome: &str,
    category: Option<&str>,
    profiles_before: usize,
    profiles_after: usize,
) -> CoverageAttempt {
    CoverageAttempt {
        package: package.map(str::to_string),
        target: None,
        configuration: configuration.into(),
        features: vec![],
        no_default_features: false,
        all_features: configuration == "all-features",
        command: vec!["cargo".into(), "llvm-cov".into()],
        exit_code: (outcome != "unavailable").then_some(if outcome == "success" { 0 } else { 1 }),
        stage: if category == Some("test-failure") {
            "test-execution".into()
        } else {
            "instrument-build".into()
        },
        outcome: outcome.into(),
        category: category.map(str::to_string),
        profiles_before,
        profiles_after,
        diagnostic: (outcome != "success").then(|| format!("{configuration} failed")),
    }
}

fn parsed(percent: f64) -> CoverageEvidence {
    CoverageEvidence {
        status: EvidenceStatus::Available,
        percent: Some(percent),
        source: Some("llvm-cov.json".into()),
        ..CoverageEvidence::default()
    }
}

#[test]
fn one_failed_member_preserves_successful_member_profiles_as_partial_coverage() {
    let root = temp_dir("member-failure");
    let manifest = CoverageManifest {
        schema: 1,
        eligible_packages: 2,
        covered_packages: 1,
        failed_packages: 1,
        eligible_source_loc: 200,
        covered_source_loc: 120,
        profile_count: 75,
        attempts: vec![
            attempt(Some("wallet"), "default-package-retry", "success", None, 0, 75),
            attempt(
                Some("rocksdb"),
                "default-package-retry",
                "failed",
                Some("environment-native-build"),
                75,
                75,
            ),
        ],
        ..CoverageManifest::default()
    };
    let evidence = finish_collection(&root, Some(parsed(71.4)), manifest, true);
    assert_eq!(evidence.status, EvidenceStatus::Partial);
    assert_eq!(evidence.percent, Some(71.4));
    assert_eq!(evidence.profile_count, 75);
    assert_eq!(evidence.scope_percent, Some(60.0));
    assert_eq!(evidence.covered_packages, 1);
    assert_eq!(evidence.failed_packages, 1);
    assert!(root.join("coverage-failures.json").is_file());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn optional_all_features_failure_does_not_erase_successful_default_coverage() {
    let root = temp_dir("all-features-failure");
    let manifest = CoverageManifest {
        schema: 1,
        eligible_packages: 1,
        covered_packages: 1,
        eligible_source_loc: 80,
        covered_source_loc: 80,
        profile_count: 6,
        attempts: vec![
            attempt(Some("node"), "default", "success", None, 0, 4),
            attempt(Some("node"), "all-features", "failed", Some("build-or-instrumentation"), 4, 6),
        ],
        ..CoverageManifest::default()
    };
    let evidence = finish_collection(&root, Some(parsed(92.0)), manifest, true);
    assert_eq!(evidence.status, EvidenceStatus::Partial);
    assert_eq!(evidence.percent, Some(92.0));
    assert_eq!(evidence.scope_percent, Some(100.0));
    assert_eq!(evidence.failed_packages, 0);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn report_failure_retains_profile_count_and_fails_instead_of_inventing_coverage() {
    let root = temp_dir("report-failure");
    let manifest = CoverageManifest {
        schema: 1,
        eligible_packages: 2,
        covered_packages: 1,
        failed_packages: 1,
        eligible_source_loc: 200,
        covered_source_loc: 120,
        profile_count: 75,
        attempts: vec![CoverageAttempt {
            package: None,
            target: None,
            configuration: "merged-report".into(),
            features: vec![],
            no_default_features: false,
            all_features: false,
            command: vec!["cargo".into(), "llvm-cov".into(), "report".into()],
            exit_code: Some(1),
            stage: "report".into(),
            outcome: "failed".into(),
            category: Some("build-or-instrumentation".into()),
            profiles_before: 75,
            profiles_after: 75,
            diagnostic: Some("llvm-profdata could not merge one profile".into()),
        }],
        ..CoverageManifest::default()
    };
    let evidence = finish_collection(&root, None, manifest, true);
    assert_eq!(evidence.status, EvidenceStatus::Failed);
    assert_eq!(evidence.profile_count, 75);
    assert!(evidence.percent.is_none());
    assert!(evidence.error.as_deref().is_some_and(|error| error.contains("75 raw profile")));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn unsupported_member_is_not_applicable_only_for_implicit_host_coverage() {
    let attempt = attempt(
        Some("web-only"),
        "default-package-retry",
        "failed",
        Some("unsupported-target"),
        0,
        0,
    );
    assert!(host_incompatible(&attempt, None));
    assert!(!host_incompatible(&attempt, Some("wasm32-unknown-unknown")));
}

#[test]
fn project_default_is_used_only_for_unfiltered_implicit_host_scope() {
    let mut config = QaConfig::default();
    assert!(project_default_matches_scope(&config));
    config.coverage.include_packages = vec!["wallet".into()];
    assert!(!project_default_matches_scope(&config));
    config.coverage.include_packages.clear();
    config.coverage.exclude_packages = vec!["ffi".into()];
    assert!(!project_default_matches_scope(&config));
    config.coverage.exclude_packages.clear();
    config.coverage.targets = vec!["wasm32-unknown-unknown".into()];
    assert!(!project_default_matches_scope(&config));
}

#[test]
fn successful_project_default_marks_only_metadata_default_members() {
    let packages = vec![
        super::super::model::CoveragePackage {
            name: "root".into(),
            root: "/workspace/root".into(),
            source_loc: 10,
            default_member: true,
        },
        super::super::model::CoveragePackage {
            name: "extra".into(),
            root: "/workspace/extra".into(),
            source_loc: 20,
            default_member: false,
        },
    ];
    let mut states = packages
        .iter()
        .map(|package| (package.name.clone(), PackageState::default()))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(mark_default_success(&packages, &mut states), vec!["root"]);
    assert_eq!(states["root"].baseline_successes, 1);
    assert_eq!(states["extra"].baseline_successes, 0);
}

#[test]
fn project_default_failure_states_distinguish_retryable_from_tooling() {
    let test_failure = attempt(None, "project-default", "failed", Some("test-failure"), 0, 2);
    let tooling_failure = attempt(None, "project-default", "unavailable", Some("tooling"), 0, 0);
    assert_eq!(project_default_state(&test_failure), ProjectDefaultState::RetryableFailure);
    assert_eq!(project_default_state(&tooling_failure), ProjectDefaultState::ToolingFailure);
}
