use qa_model::{EvidenceStatus, Finding, Severity};
use qa_policy::QaConfig;

pub(super) fn apply_coverage(
    rules: &mut qa_rules::RuleOutput,
    config: &QaConfig,
    coverage: &qa_backends::coverage::CoverageEvidence,
) {
    push_coverage_threshold_finding(rules, config, coverage);
    let mut crap_findings = Vec::new();
    for function in &mut rules.functions {
        function.coverage_percent = qa_backends::coverage::function_percent(
            coverage,
            &function.path,
            function.line,
            function.end_line,
        );
        function.crap = production_crap(function);
        if let Some(finding) = crap_finding(config, function) {
            crap_findings.push(finding);
        }
    }
    rules.findings.extend(crap_findings);
}

pub(super) fn push_coverage_threshold_finding(
    rules: &mut qa_rules::RuleOutput,
    config: &QaConfig,
    coverage: &qa_backends::coverage::CoverageEvidence,
) {
    if coverage.status != EvidenceStatus::Available
        || coverage.percent.unwrap_or(0.0) >= config.metrics.coverage_percent
    {
        return;
    }
    rules.findings.push(Finding {
        rule_id: "QA-COV-001".into(),
        severity: Severity::High,
        message: format!(
            "Workspace coverage {:.2}% is below {:.2}%",
            floor_percent(coverage.percent.unwrap_or(0.0)),
            config.metrics.coverage_percent
        ),
        path: coverage.source.clone(),
        line: None,
        detail: Some(
            "Strict coverage remains blocking; increase exercised production code rather than suppressing CRAP or coverage findings."
                .into(),
        ),
    });
}

pub(super) fn floor_percent(value: f64) -> f64 {
    (value * 100.0).floor() / 100.0
}

pub(super) fn production_crap(function: &qa_model::FunctionMetric) -> Option<f64> {
    if function.is_test {
        None
    } else {
        function.coverage_percent.map(|percent| crap(function.cyclomatic, percent))
    }
}

pub(super) fn crap_finding(
    config: &QaConfig,
    function: &qa_model::FunctionMetric,
) -> Option<Finding> {
    let value = function.crap?;
    if value <= config.metrics.crap {
        return None;
    }
    Some(Finding {
        rule_id: "QA-METRIC-004".into(),
        severity: Severity::High,
        message: format!(
            "CRAP {:.2} exceeds {:.2}: `{}`",
            value, config.metrics.crap, function.qualified_name
        ),
        path: Some(function.path.clone()),
        line: Some(function.line),
        detail: None,
    })
}

pub(super) fn apply_mutation_findings(
    rules: &mut qa_rules::RuleOutput,
    config: &QaConfig,
    mutation: &qa_backends::mutation::MutationEvidence,
) {
    if mutation.status != EvidenceStatus::Available {
        return;
    }
    push_mutation_threshold_finding(rules, config, mutation);
    push_survivor_finding(rules, mutation);
    push_timeout_finding(rules, mutation);
}

pub(super) fn push_mutation_threshold_finding(
    rules: &mut qa_rules::RuleOutput,
    config: &QaConfig,
    mutation: &qa_backends::mutation::MutationEvidence,
) {
    if mutation.score_percent.unwrap_or(100.0) >= config.mutation.minimum_kill_percent {
        return;
    }
    rules.findings.push(Finding {
        rule_id: "QA-MUT-001".into(),
        severity: Severity::High,
        message: format!(
            "Mutation score {:.1}% is below {:.1}% ({} caught, {} missed, {} timed out)",
            mutation.score_percent.unwrap_or(0.0),
            config.mutation.minimum_kill_percent,
            mutation.caught,
            mutation.missed,
            mutation.timeout
        ),
        path: mutation.source.clone(),
        line: None,
        detail: Some(
            "Individual mutation outcomes remain available in mutation.json; the gate reports aggregate mutation risk instead of duplicating one High finding per mutant."
                .into(),
        ),
    });
}

pub(super) fn push_survivor_finding(
    rules: &mut qa_rules::RuleOutput,
    mutation: &qa_backends::mutation::MutationEvidence,
) {
    if mutation.missed == 0 {
        return;
    }
    rules.findings.push(Finding {
        rule_id: "QA-MUT-002".into(),
        severity: Severity::Medium,
        message: format!("{} mutant(s) survived the test suite", mutation.missed),
        path: mutation.source.clone(),
        line: None,
        detail: Some(
            "Survivors remain actionable evidence, but the configured aggregate mutation-score threshold is the blocking mutation gate. See mutation.json for every surviving mutant and source location."
                .into(),
        ),
    });
}

pub(super) fn push_timeout_finding(
    rules: &mut qa_rules::RuleOutput,
    mutation: &qa_backends::mutation::MutationEvidence,
) {
    if mutation.timeout == 0 {
        return;
    }
    rules.findings.push(Finding {
        rule_id: "QA-MUT-003".into(),
        severity: Severity::High,
        message: format!("{} mutant(s) timed out", mutation.timeout),
        path: mutation.source.clone(),
        line: None,
        detail: Some(
            "Timed-out mutants remain blocking because they do not provide evidence that the mutated behavior is detected. See mutation.json for details."
                .into(),
        ),
    });
}

pub(super) fn crap(complexity: usize, coverage_percent: f64) -> f64 {
    let c = complexity as f64;
    let uncovered = 1.0 - (coverage_percent / 100.0).clamp(0.0, 1.0);
    c * c * uncovered.powi(3) + c
}
