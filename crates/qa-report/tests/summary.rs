use qa_model::{
    CoverageSummary, EvidenceStatus, FuzzSummary, MutationSummary, QaReport, SummaryMetrics,
};
use qa_policy::QaConfig;

#[test]
fn summary_never_fabricates_missing_coverage_or_crap() {
    let report = QaReport {
        schema: 21,
        generated_unix_seconds: 0,
        workspace: ".".into(),
        profile: "strict".into(),
        summary: SummaryMetrics {
            health_score: 96.0,
            health_is_provisional: true,
            average_file_loc: 100.0,
            files_over_loc: 0,
            average_cc: 2.0,
            functions_over_cc: 0,
            average_crap: None,
            functions_over_crap: None,
            total_tests: 10,
            invalid_tests: 0,
            coverage: CoverageSummary {
                percent: None,
                functions_below_threshold: None,
                source: None,
                status: EvidenceStatus::Unavailable,
                ..CoverageSummary::default()
            },
            mutation: MutationSummary {
                status: EvidenceStatus::Unavailable,
                caught: 0,
                missed: 0,
                timeout: 0,
                unviable: 0,
                score_percent: None,
                source: None,
            },
            fuzz: FuzzSummary {
                target_count: 0,
                critical_targets_missing: 0,
                regression_artifacts: 0,
                unpersisted_crashes: 0,
                property_test_count: 0,
                status: EvidenceStatus::NotApplicable,
            },
            duplicate_percent: 1.0,
            dead_code_percent: 0.0,
            high_findings: 0,
            critical_findings: 0,
        },
        files: vec![],
        functions: vec![],
        types: vec![],
        interfaces: vec![],
        mutations: vec![],
        fuzz_targets: vec![],
        duplicates: vec![],
        dead_items: vec![],
        evidence: vec![],
        findings: vec![],
    };
    let text = qa_report::summary_text(&report, &QaConfig::default());
    assert!(text.contains("Health: 96.0%"));
    assert!(text.contains("coverage N/A"));
    assert!(text.contains("CRAP"));
    assert!(text.contains("N/A"));
}
