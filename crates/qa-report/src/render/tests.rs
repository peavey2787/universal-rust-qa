use super::*;
use qa_model::{
    CoverageSummary, EvidenceRecord, Finding, FuzzSummary, MutationSummary, Severity,
    SummaryMetrics,
};

fn report() -> QaReport {
    QaReport {
        schema: 20,
        generated_unix_seconds: 0,
        workspace: ".".into(),
        profile: "strict".into(),
        summary: SummaryMetrics {
            health_score: 88.25,
            health_is_provisional: false,
            average_file_loc: 123.4,
            files_over_loc: 2,
            average_cc: 3.5,
            functions_over_cc: 1,
            average_crap: Some(7.25),
            functions_over_crap: Some(3),
            total_tests: 20,
            invalid_tests: 2,
            coverage: CoverageSummary {
                percent: Some(91.2),
                functions_below_threshold: Some(4),
                source: Some("cov.json".into()),
                status: EvidenceStatus::Available,
            },
            mutation: MutationSummary {
                status: EvidenceStatus::Available,
                caught: 90,
                missed: 10,
                timeout: 0,
                unviable: 5,
                score_percent: Some(90.0),
                source: Some("mutants.out/outcomes.json".into()),
            },
            fuzz: FuzzSummary {
                target_count: 2,
                critical_targets_missing: 1,
                regression_artifacts: 0,
                unpersisted_crashes: 0,
                property_test_count: 3,
                status: EvidenceStatus::Available,
            },
            duplicate_percent: 1.5,
            dead_code_percent: 0.5,
            high_findings: 1,
            critical_findings: 1,
        },
        files: vec![],
        functions: vec![],
        types: vec![],
        interfaces: vec![],
        mutations: vec![],
        fuzz_targets: vec![],
        duplicates: vec![],
        dead_items: vec![],
        evidence: vec![EvidenceRecord {
            family: "SAN".into(),
            check: "address".into(),
            status: EvidenceStatus::Available,
            source: None,
            detail: Some("clean".into()),
        }],
        findings: vec![
            Finding {
                rule_id: "QA-SAFE-001".into(),
                severity: Severity::Critical,
                message: "critical".into(),
                path: None,
                line: None,
                detail: None,
            },
            Finding {
                rule_id: "QA-SAFE-002".into(),
                severity: Severity::High,
                message: "high".into(),
                path: None,
                line: None,
                detail: None,
            },
            Finding {
                rule_id: "MALFORMED".into(),
                severity: Severity::Low,
                message: "other".into(),
                path: None,
                line: None,
                detail: None,
            },
        ],
    }
}

#[test]
fn summary_renders_numeric_evidence_families_and_backend_details() {
    let text = summary_text(&report(), &QaConfig::default());
    assert!(text.contains("Health: 88.2%"));
    assert!(text.contains("coverage 91.20%"));
    assert!(text.contains("avg 7.25"));
    assert!(text.contains("Mutation: 90.0% (Available)"));
    assert!(text.contains("SAFE       critical 1   high 1"));
    assert!(text.contains("OTHER"));
    assert!(text.contains("Available SAN"));
    assert!(text.contains("clean"));
}

#[test]
fn provisional_missing_metrics_render_na_without_fabrication() {
    let mut report = report();
    report.summary.health_is_provisional = true;
    report.summary.coverage.percent = None;
    report.summary.average_crap = None;
    report.summary.functions_over_crap = None;
    report.summary.mutation.score_percent = None;
    report.summary.mutation.status = EvidenceStatus::Unavailable;
    report.findings.clear();
    report.evidence.clear();
    let text = summary_text(&report, &QaConfig::default());
    assert!(text.contains("(provisional)"));
    assert!(text.contains("coverage N/A"));
    assert!(text.contains("CRAP     avg N/A"));
    assert!(text.contains("Mutation: N/A (Unavailable)"));
}

#[test]
fn coverage_display_never_rounds_a_below_threshold_value_up_to_the_threshold() {
    assert_eq!(floor_percent(89.9999), 89.99);
    let mut report = report();
    report.summary.coverage.percent = Some(89.9999);
    assert!(summary_text(&report, &QaConfig::default()).contains("coverage 89.99%"));
}

#[test]
fn mutation_text_covers_scored_and_unscored_statuses() {
    assert_eq!(mutation_text(&EvidenceStatus::Available, Some(95.5)), "95.5% (Available)");
    assert_eq!(mutation_text(&EvidenceStatus::Disabled, None), "N/A (Disabled)");
}

#[test]
fn summary_counts_multiple_nonblocking_findings_in_the_same_family() {
    let mut report = report();
    report.findings = vec![
        Finding {
            rule_id: "QA-DOC-998".into(),
            severity: Severity::Medium,
            message: "first".into(),
            path: None,
            line: None,
            detail: None,
        },
        Finding {
            rule_id: "QA-DOC-999".into(),
            severity: Severity::Low,
            message: "second".into(),
            path: None,
            line: None,
            detail: None,
        },
    ];
    let text = summary_text(&report, &QaConfig::default());
    assert!(text.contains("DOC        critical 0   high 0   other 2"));
}
