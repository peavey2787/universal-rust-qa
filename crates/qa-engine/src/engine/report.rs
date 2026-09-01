use super::{avg, pen, ratio};
use qa_model::{
    CoverageSummary, EvidenceRecord, EvidenceStatus, FuzzSummary, MutationSummary, QaReport,
    Severity, SummaryMetrics,
};
use qa_policy::QaConfig;
use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

pub(super) fn build_report(
    workspace: &Path,
    config: &QaConfig,
    rules: qa_rules::RuleOutput,
    coverage: qa_backends::coverage::CoverageEvidence,
    mutation: qa_backends::mutation::MutationEvidence,
    evidence: Vec<EvidenceRecord>,
) -> QaReport {
    let summary = build_summary(config, &rules, &coverage, &mutation, &evidence);
    QaReport {
        schema: 20,
        generated_unix_seconds: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        workspace: workspace.display().to_string(),
        profile: config.profile.clone(),
        summary,
        files: rules.files,
        functions: rules.functions,
        types: rules.types,
        interfaces: rules.interfaces,
        mutations: mutation.items,
        fuzz_targets: rules.fuzz_targets,
        duplicates: rules.duplicates,
        dead_items: rules.dead_items,
        evidence,
        findings: rules.findings,
    }
}

pub(super) fn build_summary(
    config: &QaConfig,
    rules: &qa_rules::RuleOutput,
    coverage: &qa_backends::coverage::CoverageEvidence,
    mutation: &qa_backends::mutation::MutationEvidence,
    evidence: &[EvidenceRecord],
) -> SummaryMetrics {
    let file_count = rules.files.len();
    let fn_count = rules.functions.len();
    let prod = rules.functions.iter().filter(|function| !function.is_test).count();
    let avg_loc = avg(rules.files.iter().map(|file| file.logical_loc), file_count);
    let avg_cc = avg(rules.functions.iter().map(|function| function.cyclomatic), fn_count);
    let files_over =
        rules.files.iter().filter(|file| file.logical_loc > config.metrics.file_loc).count();
    let funcs_over = functions_over_cc(rules, config);
    let crap_values: Vec<f64> =
        rules.functions.iter().filter_map(|function| function.crap).collect();
    let avg_crap = optional_average(&crap_values);
    let over_crap = optional_count_over(&crap_values, config.metrics.crap);
    let tests = rules.functions.iter().filter(|function| function.is_test).count();
    let duplicate_percent = ratio(rules.duplicate_logical_loc, rules.total_logical_loc);
    let dead_percent = ratio(rules.dead_items.len(), prod);
    let high = rules.findings.iter().filter(|finding| finding.severity == Severity::High).count();
    let critical =
        rules.findings.iter().filter(|finding| finding.severity == Severity::Critical).count();
    let failed_evidence =
        evidence.iter().filter(|record| record.status == EvidenceStatus::Failed).count();
    let health_score = health_score(
        config,
        HealthInputs {
            files_over,
            file_count,
            funcs_over,
            fn_count,
            invalid_tests: rules.invalid_tests,
            tests,
            duplicate_percent,
            dead_percent,
            high,
            critical,
            failed_evidence,
        },
    );
    let functions_below = functions_below_coverage(rules, config, coverage);

    SummaryMetrics {
        health_score,
        health_is_provisional: coverage.status != EvidenceStatus::Available,
        average_file_loc: avg_loc,
        files_over_loc: files_over,
        average_cc: avg_cc,
        functions_over_cc: funcs_over,
        average_crap: avg_crap,
        functions_over_crap: over_crap,
        total_tests: tests,
        invalid_tests: rules.invalid_tests,
        coverage: CoverageSummary {
            percent: coverage.percent,
            functions_below_threshold: functions_below,
            source: coverage.source.clone(),
            status: coverage.status.clone(),
        },
        mutation: MutationSummary {
            status: mutation.status.clone(),
            caught: mutation.caught,
            missed: mutation.missed,
            timeout: mutation.timeout,
            unviable: mutation.unviable,
            score_percent: mutation.score_percent,
            source: mutation.source.clone(),
        },
        fuzz: FuzzSummary {
            target_count: rules.fuzz_targets.len(),
            critical_targets_missing: rules.critical_fuzz_missing,
            regression_artifacts: rules.fuzz_regression_artifacts,
            unpersisted_crashes: rules.fuzz_unpersisted_crashes,
            property_test_count: rules.property_test_count,
            status: EvidenceStatus::Available,
        },
        duplicate_percent,
        dead_code_percent: dead_percent,
        high_findings: high,
        critical_findings: critical,
    }
}

pub(super) fn functions_over_cc(rules: &qa_rules::RuleOutput, config: &QaConfig) -> usize {
    rules
        .functions
        .iter()
        .filter(|function| {
            function.cyclomatic
                > qa_rules::structural::metrics::effective_cc_limit_for_attributes(
                    &function.attributes,
                    config,
                )
        })
        .count()
}

pub(super) fn optional_average(values: &[f64]) -> Option<f64> {
    if values.is_empty() { None } else { Some(values.iter().sum::<f64>() / values.len() as f64) }
}

pub(super) fn optional_count_over(values: &[f64], limit: f64) -> Option<usize> {
    if values.is_empty() {
        None
    } else {
        Some(values.iter().filter(|value| **value > limit).count())
    }
}

#[derive(Clone, Copy)]
pub(super) struct HealthInputs {
    pub(super) files_over: usize,
    pub(super) file_count: usize,
    pub(super) funcs_over: usize,
    pub(super) fn_count: usize,
    pub(super) invalid_tests: usize,
    pub(super) tests: usize,
    pub(super) duplicate_percent: f64,
    pub(super) dead_percent: f64,
    pub(super) high: usize,
    pub(super) critical: usize,
    pub(super) failed_evidence: usize,
}

pub(super) fn health_score(config: &QaConfig, inputs: HealthInputs) -> f64 {
    let structure = (100.0
        - pen(inputs.files_over, inputs.file_count, 40.0)
        - pen(inputs.funcs_over, inputs.fn_count, 40.0))
    .max(0.0);
    let test_score = (100.0 - pen(inputs.invalid_tests, inputs.tests, 70.0)).max(0.0);
    let finding_score = (100.0
        - (inputs.high as f64 * 8.0
            + inputs.critical as f64 * 20.0
            + inputs.failed_evidence as f64 * 10.0))
        .max(0.0);
    let weights = &config.summary.health_weights;
    let total = (weights.structure
        + weights.tests
        + weights.duplication
        + weights.dead_code
        + weights.findings)
        .max(1) as f64;
    (structure * weights.structure as f64
        + test_score * weights.tests as f64
        + (100.0 - inputs.duplicate_percent.min(100.0)) * weights.duplication as f64
        + (100.0 - inputs.dead_percent.min(100.0)) * weights.dead_code as f64
        + finding_score * weights.findings as f64)
        / total
}

pub(super) fn functions_below_coverage(
    rules: &qa_rules::RuleOutput,
    config: &QaConfig,
    coverage: &qa_backends::coverage::CoverageEvidence,
) -> Option<usize> {
    if coverage.status != EvidenceStatus::Available {
        return None;
    }
    Some(
        rules
            .functions
            .iter()
            .filter(|function| {
                !function.is_test
                    && function
                        .coverage_percent
                        .map(|percent| percent < config.metrics.coverage_percent)
                        .unwrap_or(false)
            })
            .count(),
    )
}
