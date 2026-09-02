use super::super::{
    CoverageEvidence,
    execute::{AttemptSpec, TestMode, count_profiles, report_args, run_attempt},
    manifest::{
        failed_report_detail, metadata_failure, partial_detail, scope_percent, write_manifest,
    },
    model::{CoverageAttempt, CoverageManifest},
    parse,
};
use super::{
    CoverageScope,
    recovery::{recover_direct_reports, recover_workspace_direct_report},
};
use qa_model::EvidenceStatus;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[allow(clippy::too_many_arguments)]
pub(super) fn finalize_progressive(
    workspace: &Path,
    output: &Path,
    workspace_count: usize,
    static_not_applicable: Vec<String>,
    scope: CoverageScope,
    target: PathBuf,
    env: Vec<(&'static str, String)>,
    mut attempts: Vec<CoverageAttempt>,
    mut degraded: bool,
) -> CoverageEvidence {
    let report_path = output.join("llvm-cov.json");
    let mut profile_count = count_profiles(&target);
    let mut report_ok = false;
    if profile_count > 0 {
        let strict_report = run_attempt(
            workspace,
            &target,
            &env,
            AttemptSpec {
                package: None,
                target: None,
                configuration: "merged-report-strict",
                mode: TestMode::Report,
                args: report_args(&report_path, false),
            },
        );
        report_ok = strict_report.outcome == "success";
        attempts.push(strict_report);
        if !report_ok {
            degraded = true;
            remove_stale_report(&report_path, &mut degraded);
            let tolerant_report = run_attempt(
                workspace,
                &target,
                &env,
                AttemptSpec {
                    package: None,
                    target: None,
                    configuration: "merged-report-tolerant",
                    mode: TestMode::Report,
                    args: report_args(&report_path, true),
                },
            );
            report_ok = tolerant_report.outcome == "success";
            attempts.push(tolerant_report);
        }
    }

    let planned_covered_roots =
        scope.covered.iter().map(|package| package.root.clone()).collect::<Vec<_>>();
    let planned_excluded_roots = scope
        .eligible
        .iter()
        .filter(|package| !scope.covered.iter().any(|covered| covered.name == package.name))
        .chain(scope.runtime_not_applicable.iter())
        .map(|package| package.root.clone())
        .collect::<Vec<_>>();
    let mut parsed = report_ok
        .then(|| parse_report(&report_path, &planned_covered_roots, &planned_excluded_roots))
        .flatten();
    let mut measured_names = if parsed.as_ref().is_some_and(usable_coverage) {
        scope.covered.iter().map(|package| package.name.clone()).collect::<Vec<_>>()
    } else {
        vec![]
    };
    let mut recovery_used = false;

    if parsed.as_ref().is_none_or(|evidence| !usable_coverage(evidence)) {
        if let Some(recovered) =
            recover_workspace_direct_report(workspace, output, &scope, &mut attempts)
        {
            degraded |= recovered.degraded;
            profile_count += recovered.profile_count;
            measured_names = recovered.package_names;
            parsed = Some(recovered.evidence);
            recovery_used = true;
        }
    }

    let missing = scope
        .eligible
        .iter()
        .filter(|package| !measured_names.iter().any(|name| name == &package.name))
        .cloned()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        if let Some(recovered) =
            recover_direct_reports(workspace, output, &scope, &missing, &mut attempts)
        {
            degraded |= recovered.degraded;
            profile_count += recovered.profile_count;
            if let Some(existing) = parsed.as_mut().filter(|evidence| usable_coverage(evidence)) {
                parse::merge_evidence(existing, recovered.evidence);
            } else {
                parsed = Some(recovered.evidence);
            }
            for name in recovered.package_names {
                if !measured_names.contains(&name) {
                    measured_names.push(name);
                }
            }
            recovery_used = true;
        }
    }
    let measured = scope
        .eligible
        .iter()
        .filter(|package| measured_names.iter().any(|name| name == &package.name))
        .collect::<Vec<_>>();
    let covered_roots = measured.iter().map(|package| package.root.clone()).collect::<Vec<_>>();
    let excluded_roots = scope
        .eligible
        .iter()
        .filter(|package| !measured.iter().any(|covered| covered.name == package.name))
        .chain(scope.runtime_not_applicable.iter())
        .map(|package| package.root.clone())
        .collect::<Vec<_>>();
    if let Some(evidence) = parsed.as_mut() {
        if usable_coverage(evidence) {
            parse::retain_package_scope(evidence, &covered_roots, &excluded_roots);
            if recovery_used && usable_coverage(evidence) {
                if let Err(error) = parse::write_merged_json(&report_path, evidence) {
                    degraded = true;
                    evidence.status = EvidenceStatus::Partial;
                    evidence.error = Some(append_error(evidence.error.take(), error));
                } else {
                    evidence.source = Some(report_path.display().to_string());
                }
            }
        }
    }
    let failed_package_names = scope
        .eligible
        .iter()
        .filter(|package| !measured.iter().any(|covered| covered.name == package.name))
        .map(|package| package.name.clone())
        .collect::<Vec<_>>();
    let eligible_source_loc = scope.eligible.iter().map(|package| package.source_loc).sum();
    let covered_source_loc = measured.iter().map(|package| package.source_loc).sum();
    let eligible_package_names =
        scope.eligible.iter().map(|package| package.name.clone()).collect::<Vec<_>>();
    let covered_package_names =
        measured.iter().map(|package| package.name.clone()).collect::<Vec<_>>();
    let not_applicable_package_names = static_not_applicable
        .iter()
        .cloned()
        .chain(scope.runtime_not_applicable.iter().map(|package| package.name.clone()))
        .collect::<Vec<_>>();
    degraded |= measured.len() != scope.eligible.len() || has_test_execution_failure(&attempts);
    finish_collection(
        output,
        parsed,
        CoverageManifest {
            schema: 1,
            status: String::new(),
            workspace_packages: workspace_count,
            eligible_packages: scope.eligible.len(),
            covered_packages: measured.len(),
            failed_packages: failed_package_names.len(),
            not_applicable_packages: not_applicable_package_names.len(),
            eligible_source_loc,
            covered_source_loc,
            profile_count,
            eligible_package_names,
            covered_package_names,
            failed_package_names,
            not_applicable_package_names,
            covered_package_roots: covered_roots,
            excluded_package_roots: excluded_roots,
            attempts,
        },
        degraded,
    )
}

fn has_test_execution_failure(attempts: &[CoverageAttempt]) -> bool {
    attempts.iter().any(|attempt| attempt.stage == "test-execution" && attempt.outcome != "success")
}

fn remove_stale_report(path: &Path, degraded: &mut bool) {
    if !path.exists() {
        return;
    }
    if let Err(error) = fs::remove_file(path) {
        eprintln!("warning: failed to remove stale coverage report {}: {error}", path.display());
        *degraded = true;
    }
}

fn usable_coverage(evidence: &CoverageEvidence) -> bool {
    matches!(evidence.status, EvidenceStatus::Available | EvidenceStatus::Partial)
}

fn manifest_result(output: &Path, manifest: &CoverageManifest) -> (Option<String>, Option<String>) {
    match write_manifest(output, manifest) {
        Ok(path) => (Some(path), None),
        Err(error) => (None, Some(error)),
    }
}

fn append_error(existing: Option<String>, extra: String) -> String {
    if extra.is_empty() {
        return existing.unwrap_or_default();
    }
    match existing {
        Some(existing) if !existing.is_empty() => format!("{existing}; {extra}"),
        _ => extra,
    }
}

fn parse_report(
    path: &Path,
    covered_roots: &[String],
    excluded_roots: &[String],
) -> Option<CoverageEvidence> {
    path.exists().then(|| {
        let mut evidence = parse::parse(path);
        if evidence.status == EvidenceStatus::Available {
            parse::retain_package_scope(&mut evidence, covered_roots, excluded_roots);
        }
        evidence
    })
}

pub(super) fn finish_collection(
    output: &Path,
    parsed: Option<CoverageEvidence>,
    mut manifest: CoverageManifest,
    degraded: bool,
) -> CoverageEvidence {
    manifest.status = if parsed.as_ref().is_some_and(usable_coverage) {
        if degraded
            || parsed.as_ref().is_some_and(|evidence| evidence.status == EvidenceStatus::Partial)
        {
            "partial".into()
        } else {
            "complete".into()
        }
    } else {
        "failed".into()
    };
    let (manifest_path, manifest_error) = manifest_result(output, &manifest);
    let Some(mut evidence) = parsed else {
        return failed_from_manifest(manifest, manifest_path, manifest_error);
    };
    if !usable_coverage(&evidence) {
        evidence.error = Some(failed_report_detail(manifest.profile_count, &manifest));
    } else if degraded || evidence.status == EvidenceStatus::Partial {
        evidence.status = EvidenceStatus::Partial;
        evidence.error = Some(append_error(evidence.error.take(), partial_detail(&manifest)));
    }
    if let Some(error) = manifest_error {
        evidence.error = Some(append_error(evidence.error.take(), error));
        evidence.status = EvidenceStatus::Partial;
    }
    apply_manifest_fields(&mut evidence, &manifest, manifest_path);
    evidence
}

fn failed_from_manifest(
    manifest: CoverageManifest,
    manifest_path: Option<String>,
    manifest_error: Option<String>,
) -> CoverageEvidence {
    let unavailable = manifest
        .attempts
        .iter()
        .filter(|attempt| attempt.stage != "metadata")
        .all(|attempt| attempt.outcome == "unavailable");
    CoverageEvidence {
        status: if unavailable { EvidenceStatus::Unavailable } else { EvidenceStatus::Failed },
        error: Some(append_error(
            Some(failed_report_detail(manifest.profile_count, &manifest)),
            manifest_error.unwrap_or_default(),
        )),
        failure_manifest: manifest_path,
        scope_percent: scope_percent(manifest.covered_source_loc, manifest.eligible_source_loc),
        eligible_packages: manifest.eligible_packages,
        covered_packages: manifest.covered_packages,
        failed_packages: manifest.failed_packages,
        not_applicable_packages: manifest.not_applicable_packages,
        eligible_source_loc: manifest.eligible_source_loc,
        covered_source_loc: manifest.covered_source_loc,
        profile_count: manifest.profile_count,
        ..CoverageEvidence::default()
    }
}

fn apply_manifest_fields(
    evidence: &mut CoverageEvidence,
    manifest: &CoverageManifest,
    manifest_path: Option<String>,
) {
    evidence.scope_percent =
        scope_percent(manifest.covered_source_loc, manifest.eligible_source_loc);
    evidence.eligible_packages = manifest.eligible_packages;
    evidence.covered_packages = manifest.covered_packages;
    evidence.failed_packages = manifest.failed_packages;
    evidence.not_applicable_packages = manifest.not_applicable_packages;
    evidence.eligible_source_loc = manifest.eligible_source_loc;
    evidence.covered_source_loc = manifest.covered_source_loc;
    evidence.profile_count = manifest.profile_count;
    evidence.failure_manifest = manifest_path;
}

pub(super) fn metadata_error(output: &Path, error: String) -> CoverageEvidence {
    let mut manifest =
        CoverageManifest { schema: 1, status: "failed".into(), ..CoverageManifest::default() };
    manifest.attempts.push(metadata_failure(error.clone()));
    let (manifest_path, manifest_error) = manifest_result(output, &manifest);
    CoverageEvidence {
        status: EvidenceStatus::Failed,
        error: Some(append_error(Some(error), manifest_error.unwrap_or_default())),
        failure_manifest: manifest_path,
        ..CoverageEvidence::default()
    }
}

pub(super) fn failed(error: String) -> CoverageEvidence {
    CoverageEvidence {
        status: EvidenceStatus::Failed,
        error: Some(error),
        ..CoverageEvidence::default()
    }
}

pub(super) fn restore_manifest(output: &Path, evidence: &mut CoverageEvidence) -> bool {
    super::super::manifest::restore_manifest(output, evidence)
}
