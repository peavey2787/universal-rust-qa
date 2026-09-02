use qa_model::EvidenceStatus;
use qa_policy::QaConfig;
use std::{collections::BTreeMap, path::Path};

mod execute;
mod manifest;
mod model;
mod parse;
mod plan;
mod runner;
mod tooling;

#[derive(Debug, Clone, Default)]
pub struct CoverageEvidence {
    pub status: EvidenceStatus,
    pub percent: Option<f64>,
    pub source: Option<String>,
    pub error: Option<String>,
    pub files: BTreeMap<String, BTreeMap<usize, u64>>,
    pub scope_percent: Option<f64>,
    pub eligible_packages: usize,
    pub covered_packages: usize,
    pub failed_packages: usize,
    pub not_applicable_packages: usize,
    pub eligible_source_loc: usize,
    pub covered_source_loc: usize,
    pub profile_count: usize,
    pub failure_manifest: Option<String>,
}

pub fn collect(
    workspace: &Path,
    config: &QaConfig,
    output: &Path,
    force: bool,
) -> CoverageEvidence {
    if config.coverage.mode == "off" {
        return CoverageEvidence {
            status: EvidenceStatus::Disabled,
            ..CoverageEvidence::default()
        };
    }
    if force && config.coverage.mode != "existing" {
        return runner::collect_progressive(workspace, config, output);
    }
    collect_existing(output)
}

fn collect_existing(output: &Path) -> CoverageEvidence {
    let path = output.join("llvm-cov.json");
    if !path.exists() {
        return CoverageEvidence {
            status: EvidenceStatus::Unavailable,
            error: Some(
                "existing cargo-llvm-cov JSON evidence not found; rerun without the coverage reuse flag or set [coverage] mode = \"auto\" to generate fresh coverage"
                    .into(),
            ),
            ..CoverageEvidence::default()
        };
    }
    let mut evidence = parse::parse(&path);
    let has_manifest = runner::restore_manifest(output, &mut evidence);
    if evidence.status == EvidenceStatus::Available && !has_manifest {
        evidence.status = EvidenceStatus::Partial;
        evidence.error = Some(
            "coverage JSON is usable, but coverage-failures.json is missing; package/source scope is unknown"
                .into(),
        );
    }
    evidence
}

pub fn function_percent(
    evidence: &CoverageEvidence,
    path: &str,
    start: usize,
    end: usize,
) -> Option<f64> {
    if !matches!(evidence.status, EvidenceStatus::Available | EvidenceStatus::Partial) {
        return None;
    }
    let key = parse::normalize(path);
    let lines = evidence.files.get(&key).or_else(|| {
        evidence
            .files
            .iter()
            .find(|(candidate, _)| candidate.ends_with(&key) || key.ends_with(candidate.as_str()))
            .map(|(_, lines)| lines)
    })?;
    let relevant = lines.range(start..=end).collect::<Vec<_>>();
    if relevant.is_empty() {
        return None;
    }
    let covered = relevant.iter().filter(|(_, count)| **count > 0).count();
    Some(100.0 * covered as f64 / relevant.len() as f64)
}

pub fn detail(evidence: &CoverageEvidence) -> String {
    match evidence.status {
        EvidenceStatus::Available => format!(
            "coverage complete: {:.2}% line coverage; {}/{} eligible packages; {:.1}% source scope; source LOC {}/{}; {} raw profile(s)",
            evidence.percent.unwrap_or(0.0),
            evidence.covered_packages,
            evidence.eligible_packages,
            evidence.scope_percent.unwrap_or(100.0),
            evidence.covered_source_loc,
            evidence.eligible_source_loc,
            evidence.profile_count
        ),
        EvidenceStatus::Partial => {
            let scope = evidence
                .scope_percent
                .map(|value| format!("{value:.1}%"))
                .unwrap_or_else(|| "unknown".into());
            let packages = if evidence.eligible_packages > 0 {
                format!(
                    "{}/{} eligible packages",
                    evidence.covered_packages, evidence.eligible_packages
                )
            } else {
                "package scope unknown".into()
            };
            let manifest = evidence
                .failure_manifest
                .as_deref()
                .map(|path| format!("; manifest {path}"))
                .unwrap_or_default();
            format!(
                "coverage partial: {:.2}% measured line coverage; {packages}; {scope} source scope; source LOC {}/{}; {} failed package(s); {} raw profile(s){manifest}",
                evidence.percent.unwrap_or(0.0),
                evidence.covered_source_loc,
                evidence.eligible_source_loc,
                evidence.failed_packages,
                evidence.profile_count
            )
        }
        _ => evidence.error.clone().unwrap_or_else(|| "coverage evidence not available".into()),
    }
}

#[cfg(test)]
mod tests;
