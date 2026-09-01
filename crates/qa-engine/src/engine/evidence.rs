use qa_model::{EvidenceRecord, EvidenceStatus};

pub(super) fn coverage_record(
    coverage: &qa_backends::coverage::CoverageEvidence,
) -> EvidenceRecord {
    EvidenceRecord {
        family: "COV".into(),
        check: "workspace".into(),
        status: coverage.status.clone(),
        source: coverage.source.clone(),
        detail: Some(
            coverage
                .error
                .clone()
                .or_else(|| {
                    coverage.percent.map(|percent| format!("workspace line coverage {percent:.2}%"))
                })
                .unwrap_or_else(|| "coverage evidence collected".into()),
        ),
    }
}

pub(super) fn mutation_record(
    mutation: &qa_backends::mutation::MutationEvidence,
) -> EvidenceRecord {
    EvidenceRecord {
        family: "MUT".into(),
        check: "workspace".into(),
        status: mutation.status.clone(),
        source: mutation.source.clone(),
        detail: Some(
            mutation
                .error
                .clone()
                .or_else(|| {
                    mutation.score_percent.map(|score| format!("mutation score {score:.1}%"))
                })
                .unwrap_or_else(|| "mutation evidence collected".into()),
        ),
    }
}

pub(super) fn fuzz_record(
    rules: &qa_rules::RuleOutput,
    fuzz: &qa_backends::fuzz::FuzzBackend,
) -> EvidenceRecord {
    let failed = fuzz
        .targets
        .values()
        .any(|status| matches!(status, EvidenceStatus::Failed | EvidenceStatus::Unavailable));
    let status = if rules.fuzz_targets.is_empty() {
        EvidenceStatus::NotApplicable
    } else if failed {
        EvidenceStatus::Failed
    } else {
        EvidenceStatus::Available
    };
    let detail = if rules.fuzz_targets.is_empty() {
        "no fuzz targets are required by the current source inventory".into()
    } else if fuzz.errors.is_empty() {
        format!("{} fuzz target(s) evaluated", rules.fuzz_targets.len())
    } else {
        fuzz.errors
            .iter()
            .map(|(target, error)| format!("{target}: {error}"))
            .collect::<Vec<_>>()
            .join("; ")
    };
    EvidenceRecord {
        family: "FUZZ".into(),
        check: "targets".into(),
        status,
        source: None,
        detail: Some(detail),
    }
}

pub(super) fn avg(i: impl Iterator<Item = usize>, n: usize) -> f64 {
    if n == 0 { 0.0 } else { i.sum::<usize>() as f64 / n as f64 }
}
pub(super) fn ratio(a: usize, b: usize) -> f64 {
    if b == 0 { 0.0 } else { 100.0 * a as f64 / b as f64 }
}
pub(super) fn pen(bad: usize, total: usize, max: f64) -> f64 {
    if total == 0 { 0.0 } else { (bad as f64 / total as f64 * max).min(max) }
}
