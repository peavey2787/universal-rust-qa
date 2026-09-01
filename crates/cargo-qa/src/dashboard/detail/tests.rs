use super::*;
use qa_model::{
    CoverageSummary, DeadItem, DuplicateGroup, EvidenceRecord, EvidenceStatus, FileMetric, Finding,
    FunctionMetric, FuzzSummary, MutationSummary, SourceSpan, SummaryMetrics,
};

fn report_with_files() -> QaReport {
    QaReport {
        schema: 20,
        generated_unix_seconds: 0,
        workspace: ".".into(),
        profile: "strict".into(),
        summary: SummaryMetrics {
            health_score: 100.0,
            health_is_provisional: false,
            average_file_loc: 0.0,
            files_over_loc: 0,
            average_cc: 0.0,
            functions_over_cc: 0,
            average_crap: Some(0.0),
            functions_over_crap: Some(0),
            total_tests: 0,
            invalid_tests: 0,
            coverage: CoverageSummary {
                percent: Some(100.0),
                functions_below_threshold: Some(0),
                source: None,
                status: EvidenceStatus::Available,
            },
            mutation: MutationSummary {
                status: EvidenceStatus::Available,
                caught: 1,
                missed: 0,
                timeout: 0,
                unviable: 0,
                score_percent: Some(100.0),
                source: None,
            },
            fuzz: FuzzSummary {
                target_count: 0,
                critical_targets_missing: 0,
                regression_artifacts: 0,
                unpersisted_crashes: 0,
                property_test_count: 0,
                status: EvidenceStatus::Available,
            },
            duplicate_percent: 0.0,
            dead_code_percent: 0.0,
            high_findings: 0,
            critical_findings: 0,
        },
        files: vec![
            FileMetric {
                path: "large.rs".into(),
                logical_loc: 30,
                physical_loc: 30,
                function_count: 1,
                average_cyclomatic: 1.0,
                max_cyclomatic: 1,
                average_cognitive: 0.0,
                max_cognitive: 0,
            },
            FileMetric {
                path: "small.rs".into(),
                logical_loc: 10,
                physical_loc: 10,
                function_count: 1,
                average_cyclomatic: 1.0,
                max_cyclomatic: 1,
                average_cognitive: 0.0,
                max_cognitive: 0,
            },
            FileMetric {
                path: "medium.rs".into(),
                logical_loc: 20,
                physical_loc: 20,
                function_count: 1,
                average_cyclomatic: 1.0,
                max_cyclomatic: 1,
                average_cognitive: 0.0,
                max_cognitive: 0,
            },
        ],
        functions: vec![],
        types: vec![],
        interfaces: vec![],
        mutations: vec![],
        fuzz_targets: vec![],
        duplicates: vec![],
        dead_items: vec![],
        evidence: vec![],
        findings: vec![],
    }
}

fn function(cyclomatic: usize, crap: Option<f64>) -> FunctionMetric {
    FunctionMetric {
        path: "src/lib.rs".into(),
        name: "f".into(),
        qualified_name: "f".into(),
        line: 1,
        end_line: 2,
        logical_loc: 1,
        statements: 1,
        parameters: 0,
        generic_parameters: 0,
        cyclomatic,
        cognitive: 0,
        coverage_percent: Some(100.0),
        crap,
        is_test: false,
        is_public: false,
        is_async: false,
        attributes: vec![],
    }
}

#[test]
fn severity_rank_orders_all_levels_and_resolve_preserves_absolute_paths() {
    assert!(rank(Severity::Critical) > rank(Severity::High));
    assert!(rank(Severity::High) > rank(Severity::Medium));
    assert!(rank(Severity::Medium) > rank(Severity::Low));
    assert!(rank(Severity::Low) > rank(Severity::Info));
    let workspace = Path::new("workspace");
    assert_eq!(resolve(workspace, "src/lib.rs"), workspace.join("src/lib.rs"));
    let absolute = std::env::temp_dir().join("absolute.rs");
    assert_eq!(resolve(workspace, absolute.to_str().unwrap()), absolute);
}

#[test]
fn one_based_selection_and_back_navigation_are_exact() {
    assert_eq!(one_based_index("1", 3), Some(0));
    assert_eq!(one_based_index("3", 3), Some(2));
    assert_eq!(one_based_index("0", 3), None);
    assert_eq!(one_based_index("4", 3), None);
    assert_eq!(one_based_index("x", 3), None);
    assert_eq!(numbered_action("2", &[10u8, 20, 30]), Some(20));
    assert_eq!(numbered_action("9", &[10u8, 20, 30]), None);
    assert!(is_back(""));
    assert!(is_back("b"));
    assert!(is_back("B"));
    assert!(!is_back("1"));
}

#[test]
fn metric_kind_and_file_row_ordering_preserve_dashboard_semantics() {
    let metric = function(7, Some(12.5));
    assert_eq!(MetricKind::Cyclomatic.value(&metric), 7.0);
    assert_eq!(MetricKind::Crap.value(&metric), 12.5);
    let missing = function(2, None);
    assert_eq!(MetricKind::Crap.value(&missing), -1.0);

    let report = report_with_files();
    let descending = file_rows(&report, false, None);
    assert_eq!(
        descending.iter().map(|file| file.logical_loc).collect::<Vec<_>>(),
        vec![30, 20, 10]
    );
    let ascending = file_rows(&report, true, Some(2));
    assert_eq!(ascending.iter().map(|file| file.logical_loc).collect::<Vec<_>>(), vec![10, 20]);
}

#[test]
fn dashboard_row_renderers_preserve_indices_values_and_empty_state() {
    let mut report = report_with_files();
    report.duplicates = vec![DuplicateGroup {
        fingerprint: "fp".into(),
        kind: "exact".into(),
        similarity: 0.875,
        occurrences: vec![
            SourceSpan { path: "src/a.rs".into(), line: 4 },
            SourceSpan { path: "src/b.rs".into(), line: 9 },
        ],
        logical_lines: 8,
    }];
    let groups = duplicate_groups_text(&report);
    assert_eq!(groups, "  1. exact | 88% similar | 8 LOC | 2 occurrences\n");
    assert_eq!(
        duplicate_occurrences_text(&report.duplicates[0]),
        "  1. src/a.rs:4\n  2. src/b.rs:9\n"
    );
    assert!(duplicate_groups_text(&report_with_files()).is_empty());

    report.dead_items = vec![DeadItem {
        path: "src/dead.rs".into(),
        line: 12,
        name: "unused".into(),
        kind: "function".into(),
        confidence: "high".into(),
    }];
    assert_eq!(dead_items_text(&report), "  1. [high] unused — src/dead.rs:12\n");

    report.findings = vec![Finding {
        rule_id: "QA-X-001".into(),
        severity: Severity::High,
        message: "precise finding".into(),
        path: Some("src/lib.rs".into()),
        line: Some(7),
        detail: None,
    }];
    assert_eq!(
        finding_rows_text(&[&report.findings[0]]),
        "   1. [High] QA-X-001 — precise finding\n"
    );

    report.evidence = vec![EvidenceRecord {
        family: "COV".into(),
        check: "llvm".into(),
        status: EvidenceStatus::Available,
        source: None,
        detail: Some("91.0%".into()),
    }];
    let evidence = evidence_rows_text(&report);
    assert!(evidence.starts_with("   1. [Available] COV"));
    assert!(evidence.contains("llvm"));
    assert!(evidence.ends_with("91.0%\n"));

    let files = file_rows(&report, false, Some(2));
    assert_eq!(file_rows_text(&files), "   1.    30 LOC  large.rs\n   2.    20 LOC  medium.rs\n");

    let covered = function(7, Some(12.5));
    assert_eq!(
        metric_rows_text(&[&covered], MetricKind::Cyclomatic),
        "   1.     7.00  f — src/lib.rs:1\n"
    );
    assert_eq!(
        metric_rows_text(&[&covered], MetricKind::Crap),
        "   1.    12.50  f — src/lib.rs:1\n"
    );
    assert_eq!(function_rows_text(&[&covered]), "   1.  100.0%  f — src/lib.rs:1\n");
    assert_eq!(coverage_label(None), "N/A");
    assert_eq!(coverage_label(Some(3.25)), "3.2%");
}

#[test]
fn metric_rows_exclude_tests_and_sort_high_to_low() {
    let mut report = report_with_files();
    let mut low = function(2, Some(3.0));
    low.qualified_name = "low".into();
    let mut high = function(9, Some(20.0));
    high.qualified_name = "high".into();
    let mut test = function(99, Some(999.0));
    test.qualified_name = "test_only".into();
    test.is_test = true;
    report.functions = vec![low, test, high];

    let cc = metric_rows(&report, MetricKind::Cyclomatic);
    assert_eq!(
        cc.iter().map(|row| row.qualified_name.as_str()).collect::<Vec<_>>(),
        vec!["high", "low"]
    );
    let crap = metric_rows(&report, MetricKind::Crap);
    assert_eq!(
        crap.iter().map(|row| row.qualified_name.as_str()).collect::<Vec<_>>(),
        vec!["high", "low"]
    );
}
