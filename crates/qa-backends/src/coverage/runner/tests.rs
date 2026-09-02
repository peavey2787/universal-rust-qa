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
fn usable_partial_report_keeps_numeric_coverage_and_remains_blocking() {
    let root = temp_dir("usable-partial");
    let manifest = CoverageManifest {
        schema: 1,
        eligible_packages: 1,
        covered_packages: 1,
        eligible_source_loc: 40,
        covered_source_loc: 40,
        profile_count: 2,
        ..CoverageManifest::default()
    };
    let mut partial = parsed(87.5);
    partial.status = EvidenceStatus::Partial;
    partial.error = Some("canonical merged report could not be persisted".into());
    let evidence = finish_collection(&root, Some(partial), manifest, false);
    assert_eq!(evidence.status, EvidenceStatus::Partial);
    assert_eq!(evidence.percent, Some(87.5));
    assert!(evidence.error.as_deref().is_some_and(|error| {
        error.contains("canonical merged report") && error.contains("coverage partial")
    }));
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
fn direct_workspace_json_is_the_primary_path_for_normal_host_coverage() {
    let mut config = QaConfig::default();
    assert!(direct_primary_enabled(&config.coverage));

    config.coverage.features = vec!["special".into()];
    assert!(!direct_primary_enabled(&config.coverage));
    config.coverage.features.clear();

    config.coverage.no_default_features = true;
    assert!(!direct_primary_enabled(&config.coverage));
    config.coverage.no_default_features = false;

    config.coverage.all_features = true;
    assert!(!direct_primary_enabled(&config.coverage));
    config.coverage.all_features = false;

    config.coverage.targets = vec!["wasm32-unknown-unknown".into()];
    assert!(!direct_primary_enabled(&config.coverage));
    config.coverage.targets.clear();

    config.coverage.include_packages = vec!["wallet".into()];
    assert!(!direct_primary_enabled(&config.coverage));
    config.coverage.include_packages.clear();

    config.coverage.exclude_packages = vec!["ffi".into()];
    assert!(!direct_primary_enabled(&config.coverage));
}

#[test]
fn successful_direct_primary_trusts_cargo_llvm_cov_even_when_a_package_has_no_report_file() {
    let root = temp_dir("direct-partial");
    let packages = vec![
        super::super::model::CoveragePackage {
            name: "consensus".into(),
            root: "/workspace/consensus".into(),
            source_loc: 10,
            default_member: true,
        },
        super::super::model::CoveragePackage {
            name: "macro-helper".into(),
            root: "/workspace/macro-helper".into(),
            source_loc: 20,
            default_member: true,
        },
    ];
    let recovered = recovery::DirectRecovery {
        evidence: CoverageEvidence {
            status: EvidenceStatus::Available,
            percent: Some(50.0),
            source: Some(root.join("llvm-cov.json").display().to_string()),
            files: std::collections::BTreeMap::from([(
                "/workspace/consensus/src/lib.rs".into(),
                std::collections::BTreeMap::from([(1, 1), (2, 0)]),
            )]),
            ..CoverageEvidence::default()
        },
        package_names: vec!["consensus".into()],
        profile_count: 2,
        degraded: false,
    };
    let evidence = finalize::finalize_direct(&root, 2, vec![], &packages, recovered, vec![]);
    assert_eq!(evidence.status, EvidenceStatus::Available);
    assert_eq!(evidence.percent, Some(50.0));
    assert_eq!(evidence.eligible_packages, 2);
    assert_eq!(evidence.covered_packages, 2);
    assert_eq!(evidence.failed_packages, 0);
    assert_eq!(evidence.eligible_source_loc, 30);
    assert_eq!(evidence.covered_source_loc, 30);
    assert!(evidence.failure_manifest.is_some());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn successful_direct_primary_does_not_downgrade_unknown_package_path_attribution() {
    let root = temp_dir("direct-unattributed");
    let packages = vec![super::super::model::CoveragePackage {
        name: "kaspa-consensus".into(),
        root: "C:/work/rusty-kaspa/consensus".into(),
        source_loc: 10,
        default_member: true,
    }];
    let recovered = recovery::DirectRecovery {
        evidence: CoverageEvidence {
            status: EvidenceStatus::Available,
            percent: Some(73.5),
            source: Some(root.join("llvm-cov.json").display().to_string()),
            files: std::collections::BTreeMap::from([(
                "unexpected/path/src/lib.rs".into(),
                std::collections::BTreeMap::from([(1, 1), (2, 0)]),
            )]),
            ..CoverageEvidence::default()
        },
        package_names: vec![],
        profile_count: 0,
        degraded: false,
    };

    let evidence = finalize::finalize_direct(&root, 1, vec![], &packages, recovered, vec![]);
    assert_eq!(evidence.status, EvidenceStatus::Available);
    assert_eq!(evidence.percent, Some(73.5));
    assert!(evidence.files.contains_key("unexpected/path/src/lib.rs"));
    assert_eq!(evidence.eligible_packages, 1);
    assert_eq!(evidence.covered_packages, 1);
    assert_eq!(evidence.failed_packages, 0);
    fs::remove_dir_all(root).unwrap();
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

#[test]
fn non_cargo_repository_reports_coverage_not_applicable_without_invoking_cargo() {
    let root = temp_dir("non-cargo");
    fs::write(root.join("README.md"), "not a Cargo workspace\n").unwrap();
    let output = root.join("qa-out");
    let evidence = collect_progressive(&root, &QaConfig::default(), &output);
    assert_eq!(evidence.status, EvidenceStatus::NotApplicable);
    assert!(evidence.error.as_deref().is_some_and(|error| error.contains("no Cargo.toml")));
    assert!(output.join("coverage-failures.json").is_file());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cargo_workspace_resolution_keeps_a_real_workspace_root() {
    let root = temp_dir("cargo-root");
    fs::write(root.join("Cargo.toml"), "[workspace]\n").unwrap();
    assert_eq!(resolve_cargo_workspace(&root), Some(root.clone()));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cargo_workspace_resolution_unwraps_multiple_archive_directories() {
    let root = temp_dir("cargo-double-wrapper");
    let first = root.join("download");
    let project = first.join("rusty-kaspa-master");
    fs::create_dir_all(&project).unwrap();
    fs::write(project.join("Cargo.toml"), "[workspace]\n").unwrap();
    assert_eq!(resolve_cargo_workspace(&root), Some(project));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cargo_workspace_resolution_refuses_ambiguous_projects() {
    let root = temp_dir("cargo-ambiguous-wrapper");
    for name in ["one", "two"] {
        let project = root.join(name);
        fs::create_dir_all(&project).unwrap();
        fs::write(project.join("Cargo.toml"), "[workspace]\n").unwrap();
    }
    assert_eq!(resolve_cargo_workspace(&root), None);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn successful_direct_primary_never_scope_filters_valid_llvm_json() {
    let root = temp_dir("direct-scope-empty");
    let packages = vec![super::super::model::CoveragePackage {
        name: "kaspa-consensus".into(),
        root: "C:/work/rusty-kaspa/consensus".into(),
        source_loc: 10,
        default_member: true,
    }];
    let recovered = recovery::DirectRecovery {
        evidence: CoverageEvidence {
            status: EvidenceStatus::Available,
            percent: Some(61.25),
            source: Some(root.join("llvm-cov.json").display().to_string()),
            files: std::collections::BTreeMap::from([
                ("C:/work/rusty-kaspa/consensus/src/lib.rs".into(), Default::default()),
                (
                    "unattributed/generated/source.rs".into(),
                    std::collections::BTreeMap::from([(1, 1), (2, 0)]),
                ),
            ]),
            ..CoverageEvidence::default()
        },
        package_names: vec!["kaspa-consensus".into()],
        profile_count: 0,
        degraded: false,
    };

    let evidence = finalize::finalize_direct(&root, 1, vec![], &packages, recovered, vec![]);
    assert_eq!(evidence.status, EvidenceStatus::Available);
    assert_eq!(evidence.percent, Some(61.25));
    assert!(evidence.files.contains_key("unattributed/generated/source.rs"));
    assert_eq!(evidence.covered_packages, 1);
    assert_eq!(evidence.failed_packages, 0);
    fs::remove_dir_all(root).unwrap();
}
