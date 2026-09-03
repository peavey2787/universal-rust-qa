use super::super::{
    CoverageEvidence,
    execute::{
        AttemptSpec, TestMode, bindgen_clang_environment_failure, count_profiles,
        direct_report_args, primary_direct_report_args, report_args, resilient_direct_report_args,
        run_attempt, run_attempt_with_env_removals, workspace_direct_report_args,
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
    let target = fresh_primary_target_path(output);
    let env = super::super::execute::coverage_env(&target);

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
    let clean_clang_retry = needs_clean_clang_retry(&primary);
    attempts.push(primary);
    if let Some(evidence) = parse_direct_report(&primary_path) {
        return Some(finish_primary_recovery(
            output,
            &target,
            &primary_path,
            evidence,
            primary_failed,
        ));
    }
    clear_temporary_report(&primary_path);

    if clean_clang_retry {
        let clean_path = fresh_primary_report_path(output, "clean-clang");
        let clean = run_attempt_with_env_removals(
            workspace,
            &target,
            &env,
            CLANG_OVERRIDE_ENV,
            AttemptSpec {
                package: None,
                target: None,
                configuration: "direct-workspace-clean-clang",
                mode: TestMode::DirectReport,
                args: resilient_direct_report_args(&clean_path),
            },
        );
        let clean_failed = clean.outcome != "success";
        attempts.push(clean);
        if let Some(evidence) = parse_direct_report(&clean_path) {
            return Some(finish_primary_recovery(
                output,
                &target,
                &clean_path,
                evidence,
                clean_failed,
            ));
        }
        clear_temporary_report(&clean_path);
    }

    if let Some(recovered) = salvage_profiles(workspace, output, &target, &env, attempts) {
        return Some(recovered);
    }

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
            args: resilient_direct_report_args(&tolerant_path),
        },
    );
    let tolerant_failed = tolerant.outcome != "success";
    attempts.push(tolerant);
    if let Some(evidence) = parse_direct_report(&tolerant_path) {
        return Some(finish_primary_recovery(
            output,
            &target,
            &tolerant_path,
            evidence,
            tolerant_failed,
        ));
    }
    clear_temporary_report(&tolerant_path);

    salvage_profiles(workspace, output, &target, &env, attempts)
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

const MAX_PACKAGES_PER_RECOVERY_TARGET: usize = 8;

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
    let mut rescue_generation = 0;
    let mut rescue_target = fresh_recovery_target_path(output, rescue_generation);
    cleanup_primary_target(&rescue_target);
    let mut rescue_env = super::super::execute::coverage_env(&rescue_target);
    let clean_clang_first = attempts.iter().any(needs_clean_clang_retry);
    let mut degraded = false;
    let mut completed_profile_count = 0;
    let mut packages_in_target = 0;

    for (index, package) in candidates.iter().enumerate() {
        let path = output.join(format!("llvm-cov-rescue-{index}.json"));
        if !clear_temporary_report(&path) {
            degraded = true;
            continue;
        }
        let target_triple = recovery_target(attempts, &package.name);
        let mut run = run_package_direct_recovery(
            workspace,
            &rescue_target,
            &rescue_env,
            package,
            target_triple.as_deref(),
            &path,
            clean_clang_first,
            attempts,
        );
        degraded |= run.degraded;

        if run.storage_exhausted && parse_direct_report(&path).is_none() {
            recycle_recovery_target(
                output,
                &mut rescue_generation,
                &mut rescue_target,
                &mut rescue_env,
                &mut completed_profile_count,
            );
            packages_in_target = 0;
            clear_temporary_report(&path);
            run = run_package_direct_recovery(
                workspace,
                &rescue_target,
                &rescue_env,
                package,
                target_triple.as_deref(),
                &path,
                clean_clang_first,
                attempts,
            );
            degraded |= run.degraded;
        }

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
        packages_in_target += 1;

        if should_recycle_recovery_target(packages_in_target, run.storage_exhausted) {
            recycle_recovery_target(
                output,
                &mut rescue_generation,
                &mut rescue_target,
                &mut rescue_env,
                &mut completed_profile_count,
            );
            packages_in_target = 0;
        }
    }

    let profile_count = completed_profile_count + count_profiles(&rescue_target);
    cleanup_primary_target(&rescue_target);
    if package_names.is_empty() || merged.files.is_empty() {
        return None;
    }
    persist_merged_evidence(output, &mut merged);
    Some(DirectRecovery { evidence: merged, package_names, profile_count, degraded })
}

#[derive(Debug, Clone, Copy)]
struct PackageRecoveryRun {
    degraded: bool,
    storage_exhausted: bool,
}

fn run_package_direct_recovery(
    workspace: &Path,
    rescue_target: &Path,
    rescue_env: &[(&str, String)],
    package: &CoveragePackage,
    target_triple: Option<&str>,
    path: &Path,
    clean_clang_first: bool,
    attempts: &mut Vec<CoverageAttempt>,
) -> PackageRecoveryRun {
    let spec = AttemptSpec {
        package: Some(&package.name),
        target: target_triple,
        configuration: if clean_clang_first {
            "direct-report-recovery-clean-clang"
        } else {
            "direct-report-recovery"
        },
        mode: TestMode::DirectReport,
        args: direct_report_args(package, target_triple, path),
    };
    let attempt = if clean_clang_first {
        run_attempt_with_env_removals(
            workspace,
            rescue_target,
            rescue_env,
            CLANG_OVERRIDE_ENV,
            spec,
        )
    } else {
        run_attempt(workspace, rescue_target, rescue_env, spec)
    };
    let clean_clang_retry = !clean_clang_first && needs_clean_clang_retry(&attempt);
    let mut run = PackageRecoveryRun {
        degraded: attempt.outcome != "success",
        storage_exhausted: resource_exhausted(&attempt),
    };
    attempts.push(attempt);

    if clean_clang_retry && parse_direct_report(path).is_none() {
        clear_temporary_report(path);
        let retry = run_attempt_with_env_removals(
            workspace,
            rescue_target,
            rescue_env,
            CLANG_OVERRIDE_ENV,
            AttemptSpec {
                package: Some(&package.name),
                target: target_triple,
                configuration: "direct-report-recovery-clean-clang",
                mode: TestMode::DirectReport,
                args: direct_report_args(package, target_triple, path),
            },
        );
        run.degraded |= retry.outcome != "success";
        run.storage_exhausted = resource_exhausted(&retry);
        attempts.push(retry);
    }
    run
}

fn resource_exhausted(attempt: &CoverageAttempt) -> bool {
    attempt.category.as_deref() == Some("resource-exhaustion")
}

fn should_recycle_recovery_target(packages_in_target: usize, storage_exhausted: bool) -> bool {
    storage_exhausted || packages_in_target >= MAX_PACKAGES_PER_RECOVERY_TARGET
}

fn recycle_recovery_target(
    output: &Path,
    generation: &mut usize,
    target: &mut PathBuf,
    env: &mut Vec<(&'static str, String)>,
    profile_count: &mut usize,
) {
    *profile_count += count_profiles(target);
    cleanup_primary_target(target);
    *generation += 1;
    *target = fresh_recovery_target_path(output, *generation);
    cleanup_primary_target(target);
    *env = super::super::execute::coverage_env(target);
}

fn fresh_recovery_target_path(output: &Path, generation: usize) -> PathBuf {
    output.join(format!(".cov-target-{}-rescue-{generation}", std::process::id()))
}

pub(super) fn persist_merged_evidence(output: &Path, evidence: &mut CoverageEvidence) {
    let path = output.join("llvm-cov.json");
    match parse::write_merged_json(&path, evidence) {
        Ok(()) => evidence.source = Some(path.display().to_string()),
        Err(error) => {
            evidence.status = EvidenceStatus::Partial;
            evidence.error = Some(match evidence.error.take() {
                Some(existing) => format!("{existing}; {error}"),
                None => error,
            });
        }
    }
}

const CLANG_OVERRIDE_ENV: &[&str] =
    &["LIBCLANG_PATH", "CLANG_PATH", "LLVM_CONFIG_PATH", "BINDGEN_EXTRA_CLANG_ARGS"];

fn needs_clean_clang_retry(attempt: &CoverageAttempt) -> bool {
    attempt.diagnostic.as_deref().is_some_and(bindgen_clang_environment_failure)
}

fn salvage_profiles(
    workspace: &Path,
    output: &Path,
    target: &Path,
    env: &[(&str, String)],
    attempts: &mut Vec<CoverageAttempt>,
) -> Option<DirectRecovery> {
    if count_profiles(target) == 0 {
        return None;
    }
    let path = fresh_primary_report_path(output, "profiles");
    let attempt = run_attempt(
        workspace,
        target,
        env,
        AttemptSpec {
            package: None,
            target: None,
            configuration: "direct-profile-salvage",
            mode: TestMode::Report,
            args: report_args(&path, true),
        },
    );
    attempts.push(attempt);
    let evidence = parse_direct_report(&path)?;
    Some(finish_primary_recovery(output, target, &path, evidence, true))
}

fn finish_primary_recovery(
    output: &Path,
    target: &Path,
    path: &Path,
    evidence: CoverageEvidence,
    degraded: bool,
) -> DirectRecovery {
    let profile_count = count_profiles(target);
    let evidence = persist_primary_report(output, path, evidence);
    cleanup_primary_target(target);
    DirectRecovery { evidence, package_names: vec![], profile_count, degraded }
}

fn fresh_primary_target_path(output: &Path) -> PathBuf {
    output.join(format!(".cov-target-{}", std::process::id()))
}

fn cleanup_primary_target(path: &Path) {
    match fs::remove_dir_all(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => eprintln!(
            "warning: failed to remove temporary coverage target {}: {error}",
            path.display()
        ),
    }
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
    fn merged_package_recovery_persists_canonical_json() {
        let root = std::env::temp_dir().join(format!(
            "urqa-merged-recovery-{}-{}",
            std::process::id(),
            module_path!().replace("::", "-")
        ));
        match fs::remove_dir_all(&root) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => panic!("failed to reset merged recovery fixture: {error}"),
        }
        fs::create_dir_all(&root).unwrap();
        let mut evidence = CoverageEvidence {
            status: EvidenceStatus::Available,
            percent: Some(50.0),
            files: BTreeMap::from([(
                "C:/ws/wallet/src/lib.rs".into(),
                BTreeMap::from([(1, 1), (2, 0)]),
            )]),
            ..CoverageEvidence::default()
        };
        persist_merged_evidence(&root, &mut evidence);
        assert_eq!(evidence.source, Some(root.join("llvm-cov.json").display().to_string()));
        assert!(root.join("llvm-cov.json").is_file());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn workspace_direct_recovery_is_skipped_for_multiple_explicit_targets() {
        let attempts = vec![attempt_with_target("a"), attempt_with_target("b")];
        assert_eq!(common_recovery_target(&attempts), None);
    }

    #[test]
    fn recovery_target_batch_is_bounded_before_disk_exhaustion() {
        assert!(!should_recycle_recovery_target(MAX_PACKAGES_PER_RECOVERY_TARGET - 1, false));
        assert!(should_recycle_recovery_target(MAX_PACKAGES_PER_RECOVERY_TARGET, false));
        assert!(should_recycle_recovery_target(1, true));
    }

    #[test]
    fn recovery_target_recycle_preserves_profile_count_and_moves_to_fresh_scratch() {
        let root = std::env::temp_dir().join(format!(
            "urqa-recovery-recycle-{}-{}",
            std::process::id(),
            module_path!().replace("::", "-")
        ));
        match fs::remove_dir_all(&root) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => panic!("failed to reset recovery recycle fixture: {error}"),
        }
        fs::create_dir_all(&root).unwrap();

        let mut generation = 0;
        let mut target = fresh_recovery_target_path(&root, generation);
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("one.profraw"), b"profile").unwrap();
        let mut env = super::super::super::execute::coverage_env(&target);
        let mut profile_count = 0;
        let old_target = target.clone();

        recycle_recovery_target(&root, &mut generation, &mut target, &mut env, &mut profile_count);

        assert_eq!(profile_count, 1);
        assert_eq!(generation, 1);
        assert!(!old_target.exists());
        assert_ne!(target, old_target);
        assert!(env.iter().any(|(key, value)| {
            *key == "CARGO_LLVM_COV_TARGET_DIR" && value == &target.display().to_string()
        }));
        fs::remove_dir_all(root).unwrap();
    }
}
