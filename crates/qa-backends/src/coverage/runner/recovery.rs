use super::super::{
    CoverageEvidence,
    execute::{
        AttemptSpec, TestMode, count_profiles, direct_report_args, primary_direct_report_args,
        run_attempt, tolerant_direct_report_args, workspace_direct_report_args,
    },
    model::{CoverageAttempt, CoveragePackage},
    parse,
};
use super::CoverageScope;
use qa_model::EvidenceStatus;
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub(super) struct DirectRecovery {
    pub(super) evidence: CoverageEvidence,
    pub(super) package_names: Vec<String>,
    pub(super) profile_count: usize,
    pub(super) degraded: bool,
}

pub(super) fn collect_primary_direct_report(
    workspace: &Path,
    output: &Path,
    attempts: &mut Vec<CoverageAttempt>,
) -> Option<DirectRecovery> {
    let target = output.join("llvm-cov-primary");
    let env = super::super::execute::primary_coverage_env();

    let primary_path = fresh_primary_report_path(output, "plain");
    let primary = run_attempt(
        workspace,
        &target,
        &env,
        AttemptSpec {
            package: None,
            target: None,
            configuration: "direct-workspace-primary",
            mode: TestMode::DirectReport,
            args: primary_direct_report_args(&primary_path),
        },
    );
    let primary_failed = primary.outcome != "success";
    attempts.push(primary);
    if let Some(evidence) = parse_direct_report(&primary_path) {
        let evidence = persist_primary_report(output, &primary_path, evidence);
        return Some(DirectRecovery {
            evidence,
            package_names: vec![],
            profile_count: count_profiles(&target),
            degraded: primary_failed,
        });
    }
    clear_temporary_report(&primary_path);

    let tolerant_path = fresh_primary_report_path(output, "tolerant");
    let tolerant = run_attempt(
        workspace,
        &target,
        &env,
        AttemptSpec {
            package: None,
            target: None,
            configuration: "direct-workspace-ignore-run-fail",
            mode: TestMode::DirectReport,
            args: tolerant_direct_report_args(&tolerant_path),
        },
    );
    attempts.push(tolerant);
    let evidence = parse_direct_report(&tolerant_path)?;
    let evidence = persist_primary_report(output, &tolerant_path, evidence);
    Some(DirectRecovery {
        evidence,
        package_names: vec![],
        profile_count: count_profiles(&target),
        degraded: true,
    })
}

pub(super) fn recover_workspace_direct_report(
    workspace: &Path,
    output: &Path,
    scope: &CoverageScope,
    attempts: &mut Vec<CoverageAttempt>,
) -> Option<DirectRecovery> {
    let target_triple = common_recovery_target(attempts)?;
    let path = output.join("llvm-cov-workspace-rescue.json");
    if !clear_temporary_report(&path) {
        return None;
    }
    let rescue_target = output.join("llvm-cov-rescue").join("workspace");
    let rescue_env = super::super::execute::coverage_env(&rescue_target);
    let attempt = run_attempt(
        workspace,
        &rescue_target,
        &rescue_env,
        AttemptSpec {
            package: None,
            target: target_triple.as_deref(),
            configuration: "direct-workspace-recovery",
            mode: TestMode::DirectReport,
            args: workspace_direct_report_args(&scope.eligible, target_triple.as_deref(), &path),
        },
    );
    let degraded = attempt.outcome != "success";
    let profile_count = count_profiles(&rescue_target);
    attempts.push(attempt);
    let roots = scope.eligible.iter().map(|package| package.root.clone()).collect::<Vec<_>>();
    let excluded =
        scope.runtime_not_applicable.iter().map(|package| package.root.clone()).collect::<Vec<_>>();
    let evidence = parse_scoped_direct_report(&path, &roots, &excluded);
    clear_temporary_report(&path);
    let evidence = evidence?;
    let package_names = measured_package_names(&scope.eligible, &evidence);
    if package_names.is_empty() {
        return None;
    }
    Some(DirectRecovery { evidence, package_names, profile_count, degraded })
}

pub(super) fn recover_direct_reports(
    workspace: &Path,
    output: &Path,
    scope: &CoverageScope,
    candidates: &[CoveragePackage],
    attempts: &mut Vec<CoverageAttempt>,
) -> Option<DirectRecovery> {
    let mut merged =
        CoverageEvidence { status: EvidenceStatus::Available, ..CoverageEvidence::default() };
    let mut package_names = Vec::new();
    let rescue_root = output.join("llvm-cov-rescue");
    let mut profile_count = 0usize;
    let mut degraded = false;
    for (index, package) in candidates.iter().enumerate() {
        let path = output.join(format!("llvm-cov-rescue-{index}.json"));
        if !clear_temporary_report(&path) {
            degraded = true;
            continue;
        }
        let rescue_target = rescue_root.join(format!("package-{index}"));
        let rescue_env = super::super::execute::coverage_env(&rescue_target);
        let target_triple = recovery_target(attempts, &package.name);
        let attempt = run_attempt(
            workspace,
            &rescue_target,
            &rescue_env,
            AttemptSpec {
                package: Some(&package.name),
                target: target_triple.as_deref(),
                configuration: "direct-report-recovery",
                mode: TestMode::DirectReport,
                args: direct_report_args(package, target_triple.as_deref(), &path),
            },
        );
        degraded |= attempt.outcome != "success";
        profile_count += count_profiles(&rescue_target);
        attempts.push(attempt);
        let excluded = scope
            .eligible
            .iter()
            .chain(scope.runtime_not_applicable.iter())
            .filter(|candidate| candidate.name != package.name)
            .map(|candidate| candidate.root.clone())
            .collect::<Vec<_>>();
        if let Some(evidence) =
            parse_scoped_direct_report(&path, std::slice::from_ref(&package.root), &excluded)
        {
            parse::merge_evidence(&mut merged, evidence);
            package_names.push(package.name.clone());
        }
        clear_temporary_report(&path);
    }
    if package_names.is_empty() || merged.files.is_empty() {
        return None;
    }
    Some(DirectRecovery { evidence: merged, package_names, profile_count, degraded })
}

fn fresh_primary_report_path(output: &Path, label: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    output.join(format!(".llvm-cov-primary-{label}-{}-{stamp}.json", std::process::id()))
}

fn persist_primary_report(
    output: &Path,
    temporary: &Path,
    mut evidence: CoverageEvidence,
) -> CoverageEvidence {
    let canonical = output.join("llvm-cov.json");
    match fs::copy(temporary, &canonical) {
        Ok(_) => {
            evidence.source = Some(canonical.display().to_string());
            let _ = fs::remove_file(temporary);
        }
        Err(error) => {
            evidence.status = EvidenceStatus::Partial;
            evidence.error = Some(format!(
                "coverage was collected, but the canonical report {} could not be updated: {error}; fresh evidence remains at {}",
                canonical.display(),
                temporary.display()
            ));
        }
    }
    evidence
}

fn parse_direct_report(path: &Path) -> Option<CoverageEvidence> {
    if !path.exists() {
        return None;
    }
    let evidence = parse::parse(path);
    usable_coverage(&evidence).then_some(evidence)
}

fn parse_scoped_direct_report(
    path: &Path,
    covered_roots: &[String],
    excluded_roots: &[String],
) -> Option<CoverageEvidence> {
    if !path.exists() {
        return None;
    }
    let mut evidence = parse::parse(path);
    if evidence.status != EvidenceStatus::Available {
        return None;
    }
    parse::retain_package_scope(&mut evidence, covered_roots, excluded_roots);
    usable_coverage(&evidence).then_some(evidence)
}

fn clear_temporary_report(path: &Path) -> bool {
    match fs::remove_file(path) {
        Ok(()) => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
        Err(error) => {
            eprintln!(
                "warning: failed to remove temporary coverage report {}: {error}",
                path.display()
            );
            false
        }
    }
}

fn common_recovery_target(attempts: &[CoverageAttempt]) -> Option<Option<String>> {
    let targets =
        attempts.iter().filter_map(|attempt| attempt.target.clone()).collect::<BTreeSet<_>>();
    if targets.len() > 1 {
        return None;
    }
    Some(targets.into_iter().next())
}

pub(super) fn measured_package_names(
    packages: &[CoveragePackage],
    evidence: &CoverageEvidence,
) -> Vec<String> {
    let mut names = BTreeSet::new();
    for path in evidence.files.keys() {
        let owner = packages
            .iter()
            .filter(|package| parse::path_within_root(path, &package.root))
            .max_by_key(|package| parse::normalize(&package.root).trim_end_matches('/').len());
        if let Some(package) = owner {
            names.insert(package.name.clone());
        }
    }
    names.into_iter().collect()
}

fn recovery_target(attempts: &[CoverageAttempt], package: &str) -> Option<String> {
    attempts
        .iter()
        .rev()
        .find(|attempt| attempt.package.as_deref() == Some(package) && attempt.target.is_some())
        .and_then(|attempt| attempt.target.clone())
}

fn usable_coverage(evidence: &CoverageEvidence) -> bool {
    matches!(evidence.status, EvidenceStatus::Available | EvidenceStatus::Partial)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn package(name: &str, root: &str) -> CoveragePackage {
        CoveragePackage {
            name: name.into(),
            root: root.into(),
            source_loc: 10,
            default_member: true,
        }
    }

    #[test]
    fn workspace_report_ownership_uses_the_most_specific_nested_package_root() {
        let packages =
            vec![package("parent", "C:/ws/consensus"), package("child", "C:/ws/consensus/core")];
        let evidence = CoverageEvidence {
            status: EvidenceStatus::Available,
            files: BTreeMap::from([
                ("C:/ws/consensus/src/lib.rs".into(), BTreeMap::from([(1, 1)])),
                ("C:/ws/consensus/core/src/lib.rs".into(), BTreeMap::from([(1, 1)])),
            ]),
            ..CoverageEvidence::default()
        };
        assert_eq!(measured_package_names(&packages, &evidence), vec!["child", "parent"]);
    }

    fn attempt_with_target(target: &str) -> CoverageAttempt {
        CoverageAttempt {
            package: None,
            target: Some(target.into()),
            configuration: "test".into(),
            features: vec![],
            no_default_features: false,
            all_features: false,
            command: vec![],
            exit_code: Some(1),
            stage: "instrument-build".into(),
            outcome: "failed".into(),
            category: None,
            profiles_before: 0,
            profiles_after: 0,
            diagnostic: None,
        }
    }

    #[test]
    fn stale_direct_report_is_removed_before_recovery() {
        let path = std::env::temp_dir().join(format!(
            "urqa-direct-report-stale-{}-{}.json",
            std::process::id(),
            module_path!().replace("::", "-")
        ));
        fs::write(&path, b"stale").unwrap();
        assert!(clear_temporary_report(&path));
        assert!(!path.exists());
        assert!(clear_temporary_report(&path));
    }

    #[test]
    fn workspace_direct_recovery_is_skipped_for_multiple_explicit_targets() {
        let attempts = vec![attempt_with_target("a"), attempt_with_target("b")];
        assert_eq!(common_recovery_target(&attempts), None);
    }
}
