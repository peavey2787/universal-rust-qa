use super::{
    CoverageEvidence,
    model::{CoverageAttempt, CoverageManifest},
};
use qa_model::EvidenceStatus;
use std::{fs, path::Path};

pub(super) const MANIFEST_NAME: &str = "coverage-failures.json";

pub(super) fn write_manifest(output: &Path, manifest: &CoverageManifest) -> Result<String, String> {
    let path = output.join(MANIFEST_NAME);
    let bytes = serde_json::to_vec_pretty(manifest)
        .map_err(|error| format!("failed to serialize coverage failure manifest: {error}"))?;
    fs::write(&path, bytes).map_err(|error| {
        format!("failed to write coverage failure manifest {}: {error}", path.display())
    })?;
    Ok(path.display().to_string())
}

pub(super) fn not_applicable_evidence(
    output: &Path,
    workspace_packages: usize,
    not_applicable_package_names: Vec<String>,
    profile_count: usize,
    attempts: Vec<CoverageAttempt>,
    detail: &str,
) -> CoverageEvidence {
    let not_applicable_packages = not_applicable_package_names.len();
    let manifest = CoverageManifest {
        schema: 1,
        status: "not-applicable".into(),
        workspace_packages,
        not_applicable_packages,
        not_applicable_package_names,
        profile_count,
        attempts,
        ..CoverageManifest::default()
    };
    let (failure_manifest, error) = match write_manifest(output, &manifest) {
        Ok(path) => (Some(path), detail.to_string()),
        Err(error) => (None, format!("{detail}; {error}")),
    };
    CoverageEvidence {
        status: EvidenceStatus::NotApplicable,
        not_applicable_packages,
        profile_count,
        failure_manifest,
        error: Some(error),
        ..CoverageEvidence::default()
    }
}

pub(super) fn metadata_failure(error: String) -> CoverageAttempt {
    CoverageAttempt {
        package: None,
        target: None,
        configuration: "metadata".into(),
        features: vec![],
        no_default_features: false,
        all_features: false,
        command: vec![
            "cargo".into(),
            "metadata".into(),
            "--no-deps".into(),
            "--format-version".into(),
            "1".into(),
        ],
        exit_code: None,
        stage: "metadata".into(),
        outcome: "failed".into(),
        category: Some("metadata".into()),
        profiles_before: 0,
        profiles_after: 0,
        diagnostic: Some(error),
    }
}

pub(super) fn scope_percent(covered: usize, eligible: usize) -> Option<f64> {
    (eligible > 0).then_some(100.0 * covered as f64 / eligible as f64)
}

pub(super) fn partial_detail(manifest: &CoverageManifest) -> String {
    let failed_attempts =
        manifest.attempts.iter().filter(|attempt| attempt.outcome != "success").count();
    let failed_packages = package_list(&manifest.failed_package_names);
    let failure = first_failure_detail(&manifest.attempts);
    format!(
        "coverage partial: {:.1}% source scope; {}/{} eligible packages measured; {} package(s) had no usable line coverage evidence{failed_packages}; {failed_attempts} collection attempt(s) failed{failure}; {} raw profile(s) retained",
        scope_percent(manifest.covered_source_loc, manifest.eligible_source_loc).unwrap_or(0.0),
        manifest.covered_packages,
        manifest.eligible_packages,
        manifest.failed_packages,
        manifest.profile_count
    )
}

fn package_list(packages: &[String]) -> String {
    if packages.is_empty() {
        return String::new();
    }
    let visible = packages.iter().take(6).cloned().collect::<Vec<_>>();
    let suffix = if packages.len() > visible.len() { ", ..." } else { "" };
    format!(" [{}{}]", visible.join(", "), suffix)
}

fn first_failure_detail(attempts: &[CoverageAttempt]) -> String {
    let attempt = attempts
        .iter()
        .find(|attempt| attempt.outcome != "success" && attempt.package.is_some())
        .or_else(|| attempts.iter().find(|attempt| attempt.outcome != "success"));
    let Some(attempt) = attempt else {
        return String::new();
    };
    let package = attempt.package.as_deref().unwrap_or("workspace");
    let category = attempt.category.as_deref().unwrap_or("unclassified");
    let diagnostic = attempt
        .diagnostic
        .as_deref()
        .map(first_diagnostic_line)
        .filter(|value| !value.is_empty())
        .map(|value| format!(": {value}"))
        .unwrap_or_default();
    format!("; first failure {package}/{category}{diagnostic}")
}

fn first_diagnostic_line(diagnostic: &str) -> String {
    let lines = diagnostic
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !matches!(*line, "stdout:" | "stderr:"))
        .collect::<Vec<_>>();
    let line = lines
        .iter()
        .copied()
        .find(|line| diagnostic_signal(line))
        .or_else(|| lines.first().copied())
        .unwrap_or("");
    const LIMIT: usize = 160;
    if line.chars().count() <= LIMIT {
        return line.to_string();
    }
    format!("{}…", line.chars().take(LIMIT).collect::<String>())
}

fn diagnostic_signal(line: &str) -> bool {
    let line = line.to_ascii_lowercase();
    ["error", "failed", "cannot", "could not", "not found", "unsupported", "panicked"]
        .iter()
        .any(|signal| line.contains(signal))
}

pub(super) fn failed_report_detail(profile_count: usize, manifest: &CoverageManifest) -> String {
    let report_error = manifest
        .attempts
        .iter()
        .rev()
        .find(|attempt| attempt.stage == "report")
        .and_then(|attempt| attempt.diagnostic.as_deref())
        .unwrap_or("coverage report was not produced");
    let collection = manifest
        .attempts
        .iter()
        .find(|attempt| attempt.stage != "report" && attempt.outcome != "success")
        .map(collection_failure)
        .unwrap_or_default();
    format!(
        "coverage report finalization failed after collecting {profile_count} raw profile(s): \
         {report_error}{collection}"
    )
}

fn collection_failure(attempt: &CoverageAttempt) -> String {
    let package = attempt.package.as_deref().unwrap_or("workspace");
    let category = attempt.category.as_deref().unwrap_or("unclassified");
    let diagnostic = attempt
        .diagnostic
        .as_deref()
        .map(first_diagnostic_line)
        .filter(|value| !value.is_empty())
        .map(|value| format!(": {value}"))
        .unwrap_or_default();
    format!("; first collection failure {package}/{category}{diagnostic}")
}

pub(super) fn restore_manifest(output: &Path, evidence: &mut CoverageEvidence) -> bool {
    let path = output.join(MANIFEST_NAME);
    let Ok(text) = fs::read_to_string(&path) else {
        return false;
    };
    let Ok(manifest) = serde_json::from_str::<CoverageManifest>(&text) else {
        return false;
    };
    evidence.status = match manifest.status.as_str() {
        "partial" => EvidenceStatus::Partial,
        "failed" => EvidenceStatus::Failed,
        "not-applicable" => EvidenceStatus::NotApplicable,
        _ => evidence.status.clone(),
    };
    if matches!(evidence.status, EvidenceStatus::Available | EvidenceStatus::Partial) {
        if manifest.covered_package_roots.is_empty() {
            if evidence.status == EvidenceStatus::Partial {
                evidence.files.clear();
            }
        } else {
            super::parse::retain_package_scope(
                evidence,
                &manifest.covered_package_roots,
                &manifest.excluded_package_roots,
            );
        }
    }
    evidence.eligible_packages = manifest.eligible_packages;
    evidence.covered_packages = manifest.covered_packages;
    evidence.failed_packages = manifest.failed_packages;
    evidence.not_applicable_packages = manifest.not_applicable_packages;
    evidence.eligible_source_loc = manifest.eligible_source_loc;
    evidence.covered_source_loc = manifest.covered_source_loc;
    evidence.scope_percent =
        scope_percent(manifest.covered_source_loc, manifest.eligible_source_loc);
    evidence.profile_count = manifest.profile_count;
    evidence.failure_manifest = Some(path.display().to_string());
    if evidence.status == EvidenceStatus::Partial {
        evidence.error = Some(partial_detail(&manifest));
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_percent_never_invents_scope_for_an_empty_eligible_set() {
        assert_eq!(scope_percent(50, 100), Some(50.0));
        assert_eq!(scope_percent(0, 0), None);
    }

    #[test]
    fn partial_detail_names_failed_package_and_failure_category() {
        let manifest = CoverageManifest {
            eligible_packages: 2,
            covered_packages: 1,
            failed_packages: 1,
            eligible_source_loc: 100,
            covered_source_loc: 70,
            profile_count: 75,
            failed_package_names: vec!["librocksdb-sys".into()],
            attempts: vec![CoverageAttempt {
                package: Some("librocksdb-sys".into()),
                target: None,
                configuration: "default-package-retry".into(),
                features: vec![],
                no_default_features: false,
                all_features: false,
                command: vec!["cargo".into(), "llvm-cov".into()],
                exit_code: Some(101),
                stage: "instrument-build".into(),
                outcome: "failed".into(),
                category: Some("environment-native-build".into()),
                profiles_before: 75,
                profiles_after: 75,
                diagnostic: Some(
                    "stdout:\ncompiling dependency\nstderr:\nerror: libclang could not be loaded\nsecondary detail"
                        .into(),
                ),
            }],
            ..CoverageManifest::default()
        };
        let detail = partial_detail(&manifest);
        assert!(detail.contains("[librocksdb-sys]"));
        assert!(detail.contains("librocksdb-sys/environment-native-build"));
        assert!(detail.contains("libclang could not be loaded"));
        assert!(!detail.contains("secondary detail"));
    }
}
