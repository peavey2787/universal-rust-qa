use super::CoverageScope;
use super::super::{
    execute::{count_profiles, report_args, run_attempt, TestMode},
    manifest::{
        failed_report_detail, metadata_failure, partial_detail, scope_percent, write_manifest,
    },
    model::{CoverageAttempt, CoverageManifest},
    parse,
    CoverageEvidence,
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
    let profile_count = count_profiles(&target);
    let strict_report = run_attempt(
        workspace,
        &target,
        &env,
        None,
        None,
        "merged-report-strict",
        TestMode::Report,
        report_args(&report_path, false),
    );
    let strict_ok = strict_report.outcome == "success";
    let mut report_ok = strict_ok;
    attempts.push(strict_report);
    if !strict_ok && profile_count > 0 {
        let _ = fs::remove_file(&report_path);
        let tolerant_report = run_attempt(
            workspace,
            &target,
            &env,
            None,
            None,
            "merged-report-tolerant",
            TestMode::Report,
            report_args(&report_path, true),
        );
        report_ok = tolerant_report.outcome == "success";
        degraded = true;
        attempts.push(tolerant_report);
    } else {
        degraded |= !strict_ok;
    }

    let covered_roots = scope
        .covered
        .iter()
        .map(|package| package.root.clone())
        .collect::<Vec<_>>();
    let excluded_roots = scope
        .eligible
        .iter()
        .filter(|package| !scope.covered.iter().any(|covered| covered.name == package.name))
        .chain(scope.runtime_not_applicable.iter())
        .map(|package| package.root.clone())
        .collect::<Vec<_>>();
    let parsed = report_ok
        .then(|| parse_report(&report_path, &covered_roots, &excluded_roots))
        .flatten();
    let eligible_source_loc = scope.eligible.iter().map(|package| package.source_loc).sum();
    let covered_source_loc = scope.covered.iter().map(|package| package.source_loc).sum();
    let eligible_package_names = scope
        .eligible
        .iter()
        .map(|package| package.name.clone())
        .collect::<Vec<_>>();
    let covered_package_names = scope
        .covered
        .iter()
        .map(|package| package.name.clone())
        .collect::<Vec<_>>();
    let not_applicable_package_names = static_not_applicable
        .iter()
        .cloned()
        .chain(scope.runtime_not_applicable.iter().map(|package| package.name.clone()))
        .collect::<Vec<_>>();
    finish_collection(
        output,
        parsed,
        CoverageManifest {
            schema: 1,
            status: String::new(),
            workspace_packages: workspace_count,
            eligible_packages: scope.eligible.len(),
            covered_packages: scope.covered.len(),
            failed_packages: scope.failed_names.len(),
            not_applicable_packages: not_applicable_package_names.len(),
            eligible_source_loc,
            covered_source_loc,
            profile_count,
            eligible_package_names,
            covered_package_names,
            failed_package_names: scope.failed_names,
            not_applicable_package_names,
            covered_package_roots: covered_roots,
            excluded_package_roots: excluded_roots,
            attempts,
        },
        degraded,
    )
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
    manifest.status = if parsed.as_ref().is_some_and(|evidence| {
        evidence.status == EvidenceStatus::Available
    }) {
        if degraded { "partial".into() } else { "complete".into() }
    } else {
        "failed".into()
    };
    let manifest_path = write_manifest(output, &manifest).ok();
    let Some(mut evidence) = parsed else {
        return failed_from_manifest(manifest, manifest_path);
    };
    if evidence.status != EvidenceStatus::Available {
        evidence.error = Some(failed_report_detail(manifest.profile_count, &manifest));
    } else if degraded {
        evidence.status = EvidenceStatus::Partial;
        evidence.error = Some(partial_detail(&manifest));
    }
    apply_manifest_fields(&mut evidence, &manifest, manifest_path);
    evidence
}

fn failed_from_manifest(
    manifest: CoverageManifest,
    manifest_path: Option<String>,
) -> CoverageEvidence {
    let unavailable = manifest
        .attempts
        .iter()
        .filter(|attempt| attempt.stage != "metadata")
        .all(|attempt| attempt.outcome == "unavailable");
    CoverageEvidence {
        status: if unavailable { EvidenceStatus::Unavailable } else { EvidenceStatus::Failed },
        error: Some(failed_report_detail(manifest.profile_count, &manifest)),
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
    let mut manifest = CoverageManifest {
        schema: 1,
        status: "failed".into(),
        ..CoverageManifest::default()
    };
    manifest.attempts.push(metadata_failure(error.clone()));
    let manifest_path = write_manifest(output, &manifest).ok();
    CoverageEvidence {
        status: EvidenceStatus::Failed,
        error: Some(error),
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
