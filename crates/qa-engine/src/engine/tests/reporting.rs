use super::function_metric;
use crate::engine::findings::push_coverage_threshold_finding;
use crate::engine::*;
use qa_backends::{coverage::CoverageEvidence, mutation::MutationEvidence};
use qa_model::{EvidenceRecord, EvidenceStatus};
use qa_policy::QaConfig;
use qa_rules::RuleOutput;

#[test]
fn report_helper_branches_preserve_clean_and_blocking_semantics() {
    let config = QaConfig::default();
    let production = function_metric(false, 2, Some(100.0));
    let test = function_metric(true, 2, Some(100.0));
    assert_eq!(production_crap(&production), Some(2.0));
    assert_eq!(production_crap(&test), None);
    assert!(crap_finding(&config, &production).is_none());

    let mut risky = function_metric(false, 4, Some(0.0));
    risky.crap = Some(20.0);
    assert_eq!(crap_finding(&config, &risky).unwrap().rule_id, "QA-METRIC-004");

    assert_eq!(optional_average(&[]), None);
    assert_eq!(optional_average(&[2.0, 4.0]), Some(3.0));
    assert_eq!(optional_count_over(&[], 3.0), None);
    assert_eq!(optional_count_over(&[2.0, 4.0], 3.0), Some(1));

    let mut rules = RuleOutput::default();
    let unavailable =
        MutationEvidence { status: EvidenceStatus::Unavailable, ..Default::default() };
    apply_mutation_findings(&mut rules, &config, &unavailable);
    assert!(rules.findings.is_empty());

    let clean = MutationEvidence {
        status: EvidenceStatus::Available,
        caught: 10,
        score_percent: Some(100.0),
        ..Default::default()
    };
    apply_mutation_findings(&mut rules, &config, &clean);
    assert!(rules.findings.is_empty());

    let passing_with_survivors = MutationEvidence {
        status: EvidenceStatus::Available,
        caught: 93,
        missed: 7,
        score_percent: Some(93.0),
        ..Default::default()
    };
    apply_mutation_findings(&mut rules, &config, &passing_with_survivors);
    assert!(!rules.findings.iter().any(|finding| finding.rule_id == "QA-MUT-001"));
    let survivor = rules.findings.iter().find(|finding| finding.rule_id == "QA-MUT-002").unwrap();
    assert_eq!(survivor.severity, qa_model::Severity::Medium);
}

#[test]
fn health_and_coverage_summary_helpers_cover_available_and_missing_evidence() {
    let config = QaConfig::default();
    let perfect = HealthInputs {
        files_over: 0,
        file_count: 10,
        funcs_over: 0,
        fn_count: 10,
        invalid_tests: 0,
        tests: 10,
        duplicate_percent: 0.0,
        dead_percent: 0.0,
        high: 0,
        critical: 0,
        failed_evidence: 0,
    };
    assert_eq!(health_score(&config, perfect), 100.0);
    assert!(health_score(&config, HealthInputs { high: 1, ..perfect }) < 100.0);

    let mut rules = RuleOutput::default();
    rules.functions.push(function_metric(false, 1, Some(95.0)));
    rules.functions.push(function_metric(false, 1, Some(50.0)));
    rules.functions.push(function_metric(true, 1, Some(0.0)));

    let unavailable =
        CoverageEvidence { status: EvidenceStatus::Unavailable, ..Default::default() };
    assert_eq!(functions_below_coverage(&rules, &config, &unavailable), None);
    let available = CoverageEvidence { status: EvidenceStatus::Available, ..Default::default() };
    assert_eq!(functions_below_coverage(&rules, &config, &available), Some(1));
    assert_eq!(functions_over_cc(&rules, &config), 0);
}

fn file_metric(logical_loc: usize) -> qa_model::FileMetric {
    qa_model::FileMetric {
        path: format!("src/{logical_loc}.rs"),
        logical_loc,
        physical_loc: logical_loc,
        function_count: 1,
        average_cyclomatic: 1.0,
        max_cyclomatic: 1,
        average_cognitive: 0.0,
        max_cognitive: 0,
    }
}

fn dead_item(name: &str) -> qa_model::DeadItem {
    qa_model::DeadItem {
        path: "src/lib.rs".into(),
        line: 1,
        name: name.into(),
        kind: "function".into(),
        confidence: "high".into(),
    }
}

#[test]
fn report_threshold_helpers_are_strict_at_the_exact_boundary() {
    let mut config = QaConfig::default();
    config.metrics.cyclomatic = 12;
    config.metrics.crap = 15.0;
    config.metrics.coverage_percent = 90.0;

    let mut rules = RuleOutput::default();
    rules.functions.push(function_metric(false, 12, Some(90.0)));
    rules.functions.push(function_metric(false, 13, Some(89.99)));
    rules.functions.push(function_metric(true, 12, Some(0.0)));
    rules.functions.push(function_metric(false, 1, None));

    assert_eq!(functions_over_cc(&rules, &config), 1);
    assert_eq!(optional_count_over(&[15.0, 15.01], 15.0), Some(1));
    let available = CoverageEvidence { status: EvidenceStatus::Available, ..Default::default() };
    assert_eq!(functions_below_coverage(&rules, &config, &available), Some(1));

    let failed = CoverageEvidence { status: EvidenceStatus::Failed, ..Default::default() };
    assert_eq!(functions_below_coverage(&rules, &config, &failed), None);
}

#[test]
fn health_score_components_have_exact_independent_weights() {
    let mut config = QaConfig::default();
    let inputs = HealthInputs {
        files_over: 1,
        file_count: 4,
        funcs_over: 1,
        fn_count: 2,
        invalid_tests: 1,
        tests: 4,
        duplicate_percent: 7.5,
        dead_percent: 12.25,
        high: 1,
        critical: 1,
        failed_evidence: 1,
    };

    config.summary.health_weights.structure = 1;
    config.summary.health_weights.tests = 0;
    config.summary.health_weights.duplication = 0;
    config.summary.health_weights.dead_code = 0;
    config.summary.health_weights.findings = 0;
    assert_eq!(health_score(&config, inputs), 70.0);

    config.summary.health_weights.structure = 0;
    config.summary.health_weights.tests = 1;
    assert_eq!(health_score(&config, inputs), 82.5);

    config.summary.health_weights.tests = 0;
    config.summary.health_weights.duplication = 1;
    assert_eq!(health_score(&config, inputs), 92.5);

    config.summary.health_weights.duplication = 0;
    config.summary.health_weights.dead_code = 1;
    assert_eq!(health_score(&config, inputs), 87.75);

    config.summary.health_weights.dead_code = 0;
    config.summary.health_weights.findings = 1;
    assert_eq!(health_score(&config, inputs), 62.0);

    config.summary.health_weights.findings = 0;
    assert_eq!(health_score(&config, inputs), 0.0);
}

#[test]
fn build_summary_preserves_exact_counts_statuses_and_sources() {
    let mut config = QaConfig::default();
    config.metrics.file_loc = 400;
    config.metrics.cyclomatic = 12;
    config.metrics.crap = 15.0;
    config.metrics.coverage_percent = 90.0;

    let mut rules = RuleOutput {
        files: vec![file_metric(400), file_metric(401)],
        functions: vec![
            function_metric(false, 12, Some(90.0)),
            function_metric(false, 13, Some(89.0)),
            function_metric(true, 50, Some(1.0)),
        ],
        ..Default::default()
    };
    rules.functions[0].crap = Some(15.0);
    rules.functions[1].crap = Some(16.0);
    rules.functions[2].crap = None;
    rules.total_logical_loc = 100;
    rules.duplicate_logical_loc = 5;
    rules.dead_items.push(dead_item("dead"));
    rules.invalid_tests = 1;
    rules.findings.push(qa_model::Finding {
        rule_id: "HIGH".into(),
        severity: qa_model::Severity::High,
        message: "high".into(),
        path: None,
        line: None,
        detail: None,
    });
    rules.findings.push(qa_model::Finding {
        rule_id: "CRIT".into(),
        severity: qa_model::Severity::Critical,
        message: "critical".into(),
        path: None,
        line: None,
        detail: None,
    });
    rules.findings.push(qa_model::Finding {
        rule_id: "LOW".into(),
        severity: qa_model::Severity::Low,
        message: "low".into(),
        path: None,
        line: None,
        detail: None,
    });

    let coverage = CoverageEvidence {
        status: EvidenceStatus::Available,
        percent: Some(90.28),
        source: Some("coverage.json".into()),
        ..Default::default()
    };
    let mutation = MutationEvidence {
        status: EvidenceStatus::Available,
        caught: 90,
        missed: 9,
        timeout: 1,
        unviable: 3,
        score_percent: Some(90.0),
        source: Some("mutants.out/outcomes.json".into()),
        ..Default::default()
    };
    let evidence = vec![
        EvidenceRecord {
            family: "A".into(),
            check: "ok".into(),
            status: EvidenceStatus::Available,
            source: None,
            detail: None,
        },
        EvidenceRecord {
            family: "B".into(),
            check: "failed".into(),
            status: EvidenceStatus::Failed,
            source: None,
            detail: None,
        },
        EvidenceRecord {
            family: "C".into(),
            check: "unknown".into(),
            status: EvidenceStatus::Unknown,
            source: None,
            detail: None,
        },
    ];

    let summary =
        crate::engine::report::build_summary(&config, &rules, &coverage, &mutation, &evidence);
    assert!(!summary.health_is_provisional);
    assert_eq!(summary.average_file_loc, 400.5);
    assert_eq!(summary.files_over_loc, 1);
    assert_eq!(summary.functions_over_cc, 2);
    assert_eq!(summary.average_crap, Some(15.5));
    assert_eq!(summary.functions_over_crap, Some(1));
    assert_eq!(summary.total_tests, 1);
    assert_eq!(summary.invalid_tests, 1);
    assert_eq!(summary.coverage.percent, Some(90.28));
    assert_eq!(summary.coverage.functions_below_threshold, Some(1));
    assert_eq!(summary.coverage.source.as_deref(), Some("coverage.json"));
    assert_eq!(summary.coverage.status, EvidenceStatus::Available);
    assert_eq!((summary.mutation.caught, summary.mutation.missed), (90, 9));
    assert_eq!((summary.mutation.timeout, summary.mutation.unviable), (1, 3));
    assert_eq!(summary.mutation.source.as_deref(), Some("mutants.out/outcomes.json"));
    assert_eq!(summary.duplicate_percent, 5.0);
    assert_eq!(summary.dead_code_percent, 50.0);
    assert_eq!(summary.high_findings, 1);
    assert_eq!(summary.critical_findings, 1);
    assert!((summary.health_score - 54.116_666_666_666_67).abs() < 1e-10);

    let failed = CoverageEvidence { status: EvidenceStatus::Failed, ..Default::default() };
    let failed_summary =
        crate::engine::report::build_summary(&config, &rules, &failed, &mutation, &evidence);
    assert!(failed_summary.health_is_provisional);
    assert_eq!(failed_summary.coverage.functions_below_threshold, None);
}

#[test]
fn coverage_and_crap_mutation_boundaries_are_exact() {
    let mut config = QaConfig::default();
    config.metrics.coverage_percent = 90.0;

    let mut rules = RuleOutput::default();
    let failed_low = CoverageEvidence {
        status: EvidenceStatus::Failed,
        percent: Some(10.0),
        ..Default::default()
    };
    push_coverage_threshold_finding(&mut rules, &config, &failed_low);
    assert!(rules.findings.is_empty());

    let available_high = CoverageEvidence {
        status: EvidenceStatus::Available,
        percent: Some(95.0),
        ..Default::default()
    };
    push_coverage_threshold_finding(&mut rules, &config, &available_high);
    assert!(rules.findings.is_empty());

    let available_low = CoverageEvidence {
        status: EvidenceStatus::Available,
        percent: Some(50.0),
        ..Default::default()
    };
    push_coverage_threshold_finding(&mut rules, &config, &available_low);
    assert_eq!(rules.findings.len(), 1);
    assert_eq!(rules.findings[0].rule_id, "QA-COV-001");
    assert!((crap(4, 50.0) - 6.0).abs() < f64::EPSILON);
}

#[test]
fn summary_counts_multiple_loc_and_production_coverage_violations() {
    let mut config = QaConfig::default();
    config.metrics.file_loc = 400;
    config.metrics.coverage_percent = 90.0;
    let rules = RuleOutput {
        files: vec![file_metric(399), file_metric(401), file_metric(402)],
        functions: vec![
            function_metric(false, 1, Some(89.0)),
            function_metric(false, 1, Some(88.0)),
            function_metric(true, 1, Some(1.0)),
        ],
        ..Default::default()
    };
    let coverage = CoverageEvidence {
        status: EvidenceStatus::Available,
        percent: Some(93.0),
        ..Default::default()
    };
    let summary = crate::engine::report::build_summary(
        &config,
        &rules,
        &coverage,
        &MutationEvidence::default(),
        &[],
    );
    assert_eq!(summary.files_over_loc, 2);
    assert_eq!(summary.coverage.functions_below_threshold, Some(2));
}
