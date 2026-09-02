use super::{
    CoverageEvidence,
    execute::{
        AttemptSpec, TestMode, count_profiles, coverage_env, optional_modes,
        prepare_coverage_target, run_attempt, target_variants, test_args,
    },
    manifest::not_applicable_evidence,
    plan::workspace_packages,
};
use qa_policy::QaConfig;
use std::{collections::BTreeMap, path::Path};

mod finalize;
mod recovery;
mod scope;

use scope::{CoverageScope, coverage_scope};

#[derive(Debug, Default)]
struct PackageState {
    baseline_successes: usize,
    optional_failed: bool,
    host_not_applicable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProjectDefaultState {
    Skipped,
    Success,
    RetryableFailure,
    ToolingFailure,
}

pub(super) fn collect_progressive(
    workspace: &Path,
    config: &QaConfig,
    output: &Path,
) -> CoverageEvidence {
    let mut target = match prepare_coverage_target(output) {
        Ok(target) => target,
        Err(error) => return finalize::failed(error),
    };
    if !workspace.join("Cargo.toml").is_file() {
        return not_applicable_evidence(
            output,
            0,
            vec![],
            0,
            vec![],
            "coverage not applicable: inspected root has no Cargo.toml",
        );
    }
    let (workspace_count, packages, static_not_applicable) =
        match workspace_packages(workspace, &config.coverage) {
            Ok(value) => value,
            Err(error) => return finalize::metadata_error(output, error),
        };
    if packages.is_empty() {
        return not_applicable_evidence(
            output,
            workspace_count,
            static_not_applicable,
            0,
            vec![],
            "no selected workspace members have Cargo-testable targets",
        );
    }
    if let Err(error) = super::tooling::ensure_llvm_cov(workspace) {
        return finalize::failed(error);
    }

    let mut attempts = Vec::new();
    if direct_primary_enabled(&config.coverage) {
        if let Some(recovered) =
            recovery::collect_primary_direct_report(workspace, output, &packages, &mut attempts)
        {
            if all_packages_measured(&packages, &recovered.package_names) {
                return finalize::finalize_direct(
                    output,
                    workspace_count,
                    static_not_applicable,
                    &packages,
                    recovered,
                    attempts,
                );
            }
        }
        target = match prepare_coverage_target(output) {
            Ok(target) => target,
            Err(error) => return finalize::failed(error),
        };
    }

    let env = coverage_env(&target);
    let target_variants = target_variants(&config.coverage);
    let mut states = initial_states(&packages);
    let mut degraded = execute_coverage_plan(
        workspace,
        config,
        &packages,
        &target_variants,
        &target,
        &env,
        &mut attempts,
        &mut states,
    );
    let scope = coverage_scope(&packages, &states, target_variants.len());
    if scope.eligible.is_empty() {
        let mut not_applicable_names = static_not_applicable;
        not_applicable_names
            .extend(scope.runtime_not_applicable.iter().map(|package| package.name.clone()));
        return not_applicable_evidence(
            output,
            workspace_count,
            not_applicable_names,
            count_profiles(&target),
            attempts,
            "all selected workspace members are incompatible with implicit host coverage",
        );
    }
    degraded |= states.values().any(|state| state.optional_failed)
        || (target_variants.len() > 1 && scope.incomplete_baseline);
    finalize::finalize_progressive(
        workspace,
        output,
        workspace_count,
        static_not_applicable,
        scope,
        target,
        env,
        attempts,
        degraded,
    )
}

fn direct_primary_enabled(config: &qa_policy::CoverageConfig) -> bool {
    config.targets.is_empty()
        && config.features.is_empty()
        && !config.no_default_features
        && !config.all_features
}

fn all_packages_measured(
    packages: &[super::model::CoveragePackage],
    measured_names: &[String],
) -> bool {
    packages.len() == measured_names.len()
        && packages.iter().all(|package| measured_names.contains(&package.name))
}

fn initial_states(packages: &[super::model::CoveragePackage]) -> BTreeMap<String, PackageState> {
    packages.iter().map(|package| (package.name.clone(), PackageState::default())).collect()
}

#[allow(clippy::too_many_arguments)]
fn execute_coverage_plan(
    workspace: &Path,
    config: &QaConfig,
    packages: &[super::model::CoveragePackage],
    target_variants: &[Option<String>],
    target: &Path,
    env: &[(&str, String)],
    attempts: &mut Vec<super::model::CoverageAttempt>,
    states: &mut BTreeMap<String, PackageState>,
) -> bool {
    let project_default = run_project_default(workspace, config, target, env, attempts);
    let mut degraded = false;
    for target_triple in target_variants {
        let baseline_ok = execute_baseline(
            workspace,
            config,
            packages,
            project_default,
            target_triple.as_deref(),
            target,
            env,
            attempts,
            states,
        );
        run_optional_configurations(
            workspace,
            config,
            packages,
            &baseline_ok,
            target_triple.as_deref(),
            target,
            env,
            attempts,
            states,
            &mut degraded,
        );
    }
    degraded
}

#[allow(clippy::too_many_arguments)]
fn execute_baseline(
    workspace: &Path,
    config: &QaConfig,
    packages: &[super::model::CoveragePackage],
    project_default: ProjectDefaultState,
    target_triple: Option<&str>,
    target: &Path,
    env: &[(&str, String)],
    attempts: &mut Vec<super::model::CoverageAttempt>,
    states: &mut BTreeMap<String, PackageState>,
) -> Vec<String> {
    if target_triple.is_none() {
        match project_default {
            ProjectDefaultState::Success => {
                let mut successful = mark_default_success(packages, states);
                let remaining = packages
                    .iter()
                    .filter(|package| !package.default_member)
                    .cloned()
                    .collect::<Vec<_>>();
                if !remaining.is_empty() {
                    successful.extend(run_baseline_scope(
                        workspace, config, &remaining, None, target, env, attempts, states,
                    ));
                }
                return successful;
            }
            ProjectDefaultState::RetryableFailure => {
                if config.coverage.adaptive {
                    return retry_packages(
                        workspace, config, packages, None, target, env, attempts, states,
                    );
                }
                return vec![];
            }
            ProjectDefaultState::ToolingFailure => return vec![],
            ProjectDefaultState::Skipped => {}
        }
    }
    run_baseline_scope(workspace, config, packages, target_triple, target, env, attempts, states)
}

fn run_project_default(
    workspace: &Path,
    config: &QaConfig,
    target: &Path,
    env: &[(&str, String)],
    attempts: &mut Vec<super::model::CoverageAttempt>,
) -> ProjectDefaultState {
    if !project_default_matches_scope(config) {
        return ProjectDefaultState::Skipped;
    }
    let args = test_args(&config.coverage, &[], None, None, TestMode::Default);
    let attempt = run_attempt(
        workspace,
        target,
        env,
        AttemptSpec {
            package: None,
            target: None,
            configuration: "project-default",
            mode: TestMode::Default,
            args,
        },
    );
    let state = project_default_state(&attempt);
    attempts.push(attempt);
    state
}

fn project_default_state(attempt: &super::model::CoverageAttempt) -> ProjectDefaultState {
    if attempt.outcome == "success" {
        ProjectDefaultState::Success
    } else if attempt.category.as_deref() == Some("tooling") {
        ProjectDefaultState::ToolingFailure
    } else {
        ProjectDefaultState::RetryableFailure
    }
}

fn project_default_matches_scope(config: &QaConfig) -> bool {
    config.coverage.targets.is_empty()
        && config.coverage.include_packages.is_empty()
        && config.coverage.exclude_packages.is_empty()
}

fn mark_default_success(
    packages: &[super::model::CoveragePackage],
    states: &mut BTreeMap<String, PackageState>,
) -> Vec<String> {
    let defaults =
        packages.iter().filter(|package| package.default_member).cloned().collect::<Vec<_>>();
    mark_group_success(&defaults, states)
}

#[allow(clippy::too_many_arguments)]
fn run_baseline_scope(
    workspace: &Path,
    config: &QaConfig,
    packages: &[super::model::CoveragePackage],
    target_triple: Option<&str>,
    target: &Path,
    env: &[(&str, String)],
    attempts: &mut Vec<super::model::CoverageAttempt>,
    states: &mut BTreeMap<String, PackageState>,
) -> Vec<String> {
    let group = run_attempt(
        workspace,
        target,
        env,
        AttemptSpec {
            package: None,
            target: target_triple,
            configuration: "eligible-package-group",
            mode: TestMode::Default,
            args: test_args(&config.coverage, packages, None, target_triple, TestMode::Default),
        },
    );
    let group_ok = group.outcome == "success";
    let retryable = group.category.as_deref() != Some("tooling");
    attempts.push(group);
    if group_ok {
        return mark_group_success(packages, states);
    }
    if !config.coverage.adaptive || !retryable {
        return vec![];
    }
    retry_packages(workspace, config, packages, target_triple, target, env, attempts, states)
}

fn mark_group_success(
    packages: &[super::model::CoveragePackage],
    states: &mut BTreeMap<String, PackageState>,
) -> Vec<String> {
    let mut successful = Vec::new();
    for package in packages {
        let Some(state) = states.get_mut(&package.name) else {
            continue;
        };
        state.baseline_successes += 1;
        successful.push(package.name.clone());
    }
    successful
}

#[allow(clippy::too_many_arguments)]
fn retry_packages(
    workspace: &Path,
    config: &QaConfig,
    packages: &[super::model::CoveragePackage],
    target_triple: Option<&str>,
    target: &Path,
    env: &[(&str, String)],
    attempts: &mut Vec<super::model::CoverageAttempt>,
    states: &mut BTreeMap<String, PackageState>,
) -> Vec<String> {
    let mut successful = Vec::new();
    for package in packages {
        let attempt = run_attempt(
            workspace,
            target,
            env,
            AttemptSpec {
                package: Some(&package.name),
                target: target_triple,
                configuration: "default-package-retry",
                mode: TestMode::Default,
                args: test_args(
                    &config.coverage,
                    std::slice::from_ref(package),
                    Some(package),
                    target_triple,
                    TestMode::Default,
                ),
            },
        );
        if attempt.outcome == "success" {
            if let Some(state) = states.get_mut(&package.name) {
                state.baseline_successes += 1;
                successful.push(package.name.clone());
            }
        } else if host_incompatible(&attempt, target_triple) {
            if let Some(state) = states.get_mut(&package.name) {
                state.host_not_applicable = true;
            }
        }
        attempts.push(attempt);
    }
    successful
}

fn host_incompatible(attempt: &super::model::CoverageAttempt, target_triple: Option<&str>) -> bool {
    target_triple.is_none()
        && attempt.outcome != "success"
        && attempt.category.as_deref() == Some("unsupported-target")
}

#[allow(clippy::too_many_arguments)]
fn run_optional_configurations(
    workspace: &Path,
    config: &QaConfig,
    packages: &[super::model::CoveragePackage],
    successful: &[String],
    target_triple: Option<&str>,
    target: &Path,
    env: &[(&str, String)],
    attempts: &mut Vec<super::model::CoverageAttempt>,
    states: &mut BTreeMap<String, PackageState>,
    degraded: &mut bool,
) {
    for package_name in successful {
        let Some(package) = packages.iter().find(|package| package.name == *package_name) else {
            continue;
        };
        for mode in optional_modes(&config.coverage) {
            let attempt = run_attempt(
                workspace,
                target,
                env,
                AttemptSpec {
                    package: Some(&package.name),
                    target: target_triple,
                    configuration: mode.label(),
                    mode,
                    args: test_args(
                        &config.coverage,
                        std::slice::from_ref(package),
                        Some(package),
                        target_triple,
                        mode,
                    ),
                },
            );
            if attempt.outcome != "success" {
                if let Some(state) = states.get_mut(&package.name) {
                    state.optional_failed = true;
                }
                *degraded = true;
            }
            attempts.push(attempt);
        }
    }
}

pub(super) fn restore_manifest(output: &Path, evidence: &mut CoverageEvidence) -> bool {
    finalize::restore_manifest(output, evidence)
}

#[cfg(test)]
use finalize::finish_collection;

#[cfg(test)]
mod tests;
