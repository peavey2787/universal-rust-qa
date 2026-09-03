use super::*;
use qa_model::{
    CoverageSummary, EvidenceRecord, EvidenceStatus, Finding, FuzzSummary, MutationItem,
    MutationSummary, QaReport, Severity, SummaryMetrics,
};

fn empty_report() -> QaReport {
    QaReport {
        schema: 21,
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
                ..CoverageSummary::default()
            },
            mutation: MutationSummary {
                status: EvidenceStatus::Available,
                caught: 0,
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
    }
}

#[test]
fn dashboard_action_table_exit_and_location_helpers_are_exact() {
    for choice in ["1", "2", "3", "4", "5", "6", "7", "8", "r", "s", "e"] {
        assert!(main_action(choice).is_some(), "missing dashboard action {choice}");
    }
    assert!(main_action("x").is_none());
    assert!(is_exit(""));
    assert!(is_exit("q"));
    assert!(!is_exit("1"));
    assert_eq!(health_color(100.0), GREEN);
    assert_eq!(health_color(90.0), GREEN);
    assert_eq!(health_color(89.9), YELLOW);
    assert_eq!(health_color(75.0), YELLOW);
    assert_eq!(health_color(74.9), RED);
    assert_eq!(crap_average(Some(6.125)), "avg         6.12");
    assert_eq!(crap_average(None), "avg          N/A");
    assert_eq!(crap_excess(Some(3), 15.0), "3 functions exceed 15.0");
    assert_eq!(crap_excess(None, 15.0), "requires coverage evidence");
    assert_eq!(coverage_percent_label(Some(89.9999)), "89.99%");
    assert_eq!(coverage_percent_label(Some(89.954)), "89.95%");
    assert_eq!(coverage_percent_label(Some(90.0)), "90.00%");
    assert_eq!(floor_percent(89.9999), 89.99);
    assert_eq!(coverage_percent_label(None), "N/A");
    assert_eq!(percent_label(Some(85.12)), "85.1%");
    assert_eq!(percent_label(None), "N/A");
    let mut summary = empty_report().summary;
    summary.health_is_provisional = false;
    assert_eq!(provisional_text(&summary), "");

    summary.health_is_provisional = true;
    summary.coverage.status = qa_model::EvidenceStatus::Partial;
    summary.coverage.percent = Some(96.0);
    summary.coverage.scope_percent = Some(71.4);
    summary.coverage.covered_packages = 73;
    summary.coverage.eligible_packages = 83;
    assert!(provisional_text(&summary).contains("coverage collection PARTIAL"));
    assert!(coverage_label(&summary).contains("96.00% PARTIAL (scope 71.4%, 73/83)"));
    summary.coverage.status = qa_model::EvidenceStatus::Failed;
    assert!(provisional_text(&summary).contains("coverage collection FAILED"));
    summary.coverage.status = qa_model::EvidenceStatus::Unavailable;
    assert!(provisional_text(&summary).contains("coverage evidence is unavailable"));
    summary.coverage.status = qa_model::EvidenceStatus::Disabled;
    assert!(provisional_text(&summary).contains("coverage is disabled"));
    assert!(row_text(1, "LOC", "a".into(), "b".into()).contains("#1"));

    let finding = Finding {
        rule_id: "QA-X".into(),
        severity: Severity::High,
        message: "bad".into(),
        path: Some("src/lib.rs".into()),
        line: Some(7),
        detail: None,
    };
    assert_eq!(finding_location(&finding), "src/lib.rs:7");
    let finding_without_line = Finding { line: None, ..finding.clone() };
    assert_eq!(finding_location(&finding_without_line), "src/lib.rs");
    let finding_without_path = Finding { path: None, ..finding };
    assert_eq!(finding_location(&finding_without_path), "workspace");

    let mutation = MutationItem {
        outcome: "MissedMutant".into(),
        path: Some("src/lib.rs".into()),
        line: Some(9),
        mutation: "replace + with -".into(),
    };
    assert_eq!(mutation_location(&mutation), "src/lib.rs:9");
    let mutation_without_line = MutationItem { line: None, ..mutation.clone() };
    assert_eq!(mutation_location(&mutation_without_line), "src/lib.rs");
    let mutation_without_path = MutationItem { path: None, ..mutation };
    assert_eq!(mutation_location(&mutation_without_path), "workspace");
}

#[test]
fn failed_coverage_dashboard_surfaces_the_actual_backend_diagnostic_and_manifest() {
    let mut report = empty_report();
    report.summary.health_is_provisional = true;
    report.summary.coverage.status = EvidenceStatus::Failed;
    report.summary.coverage.percent = None;
    report.summary.coverage.failure_manifest = Some("qa-out/coverage-failures.json".into());
    report.evidence.push(EvidenceRecord {
        family: "COV".into(),
        check: "workspace".into(),
        status: EvidenceStatus::Failed,
        source: None,
        detail: Some("cargo llvm-cov failed: linker.exe was not found".into()),
    });

    let diagnostic = coverage_diagnostic_text(&report);
    assert!(diagnostic.contains("cargo llvm-cov failed: linker.exe was not found"));
    assert!(diagnostic.contains("qa-out/coverage-failures.json"));
}

#[test]
fn live_dashboard_renders_pending_running_paused_and_complete_states() {
    let config = QaConfig::default();
    let pending = ProgressSnapshot {
        running: true,
        paused: false,
        completed: 0,
        total: 18,
        category: "coverage".into(),
        item: "cargo llvm-cov".into(),
        process_active: true,
        skip_category_pending: false,
        summary: None,
        finding_count: 0,
        evidence_count: 0,
        elapsed_seconds: 65,
    };
    let pending_text = live_dashboard_text(&config, &pending);
    assert!(pending_text.contains("UNIVERSAL RUST QA r76"));
    assert!(pending_text.contains("HEALTH   N/A"));
    assert!(pending_text.contains("coverage"));
    assert!(pending_text.contains("cargo llvm-cov"));
    assert!(pending_text.contains("0/18"));
    assert_eq!(elapsed_label(65), "00:01:05");
    assert_eq!(elapsed_label(3_661), "01:01:01");
    let (bar, percent, completed, total) = progress_bar(&pending);
    assert_eq!(bar.chars().count(), 44);
    assert_eq!((percent, completed, total), (0, 0, 18));
    assert_eq!(bar.chars().filter(|character| *character == '●').count(), 1);
    assert!(progress_state(&pending).contains("RUNNING"));
    assert!(running_progress_note(&pending).is_none());

    let halfway = ProgressSnapshot { completed: 9, ..pending.clone() };
    let (bar, percent, completed, total) = progress_bar(&halfway);
    assert_eq!((percent, completed, total), (50, 9, 18));
    assert_eq!(bar.chars().filter(|character| *character == '━').count(), 22);
    assert_eq!(bar.chars().filter(|character| *character == '●').count(), 1);
    assert_eq!(bar.chars().filter(|character| *character == '─').count(), 21);

    let almost = ProgressSnapshot { completed: 17, ..pending.clone() };
    let (bar, percent, completed, total) = progress_bar(&almost);
    assert_eq!((percent, completed, total), (94, 17, 18));
    assert_eq!(bar.chars().count(), 44);

    let clamped = ProgressSnapshot { completed: 99, ..pending.clone() };
    assert_eq!(progress_bar(&clamped).1, 100);
    assert_eq!(progress_bar(&clamped).2, 18);
    let zero_total = ProgressSnapshot { total: 0, completed: 0, ..pending.clone() };
    assert_eq!(progress_bar(&zero_total).1, 0);
    assert_eq!(progress_bar(&zero_total).3, 1);

    assert_eq!(progress_marker(&pending, 0, 44), "●");
    assert_eq!(progress_marker(&pending, 43, 44), "●");
    assert_eq!(progress_marker(&pending, 44, 44), "");
    let stopped = ProgressSnapshot { running: false, ..pending.clone() };
    assert_eq!(progress_marker(&stopped, 0, 44), "");

    let summary = SummaryMetrics {
        health_score: 91.0,
        health_is_provisional: false,
        average_file_loc: 10.0,
        files_over_loc: 0,
        average_cc: 2.0,
        functions_over_cc: 0,
        average_crap: Some(2.0),
        functions_over_crap: Some(0),
        total_tests: 10,
        invalid_tests: 0,
        coverage: CoverageSummary {
            percent: Some(95.0),
            functions_below_threshold: Some(0),
            source: None,
            status: EvidenceStatus::Available,
            ..CoverageSummary::default()
        },
        mutation: MutationSummary {
            status: EvidenceStatus::Available,
            caught: 9,
            missed: 1,
            timeout: 0,
            unviable: 0,
            score_percent: Some(90.0),
            source: None,
        },
        fuzz: FuzzSummary {
            target_count: 1,
            critical_targets_missing: 0,
            regression_artifacts: 0,
            unpersisted_crashes: 0,
            property_test_count: 1,
            status: EvidenceStatus::Available,
        },
        duplicate_percent: 1.0,
        dead_code_percent: 1.0,
        high_findings: 1,
        critical_findings: 0,
    };
    let paused = ProgressSnapshot {
        paused: true,
        completed: 9,
        process_active: true,
        summary: Some(summary.clone()),
        ..pending.clone()
    };
    let paused_text = live_dashboard_text(&config, &paused);
    assert!(paused_text.contains("HEALTH  91.0%"));
    for expected in [
        "avg file    10.0",
        "avg fn      2.00",
        "avg         2.00",
        "   10 total",
        "0 flagged | coverage 95.00%",
        "1.00%",
        "critical 0",
        "high 1",
        "score 90.0% | caught 9 | missed 1 | timeout 0",
    ] {
        assert!(paused_text.contains(expected), "missing live summary fragment: {expected}");
    }
    assert!(paused_text.contains("PAUSED"));
    assert!(progress_state(&paused).contains("PAUSED"));
    assert_eq!(
        progress_note(&paused),
        Some("active process tree is suspended; resume with P or Space")
    );
    let paused_between = ProgressSnapshot { process_active: false, ..paused.clone() };
    assert_eq!(
        progress_note(&paused_between),
        Some("pause queued; in-process work stops at the next controllable boundary")
    );
    let complete = ProgressSnapshot {
        running: false,
        paused: false,
        completed: 18,
        process_active: false,
        summary: Some(summary),
        ..pending
    };
    let complete_text = live_dashboard_text(&config, &complete);
    assert!(complete_text.contains("100%   18/18"));
    assert!(complete_text.contains("COMPLETE"));
    assert!(progress_state(&complete).contains("COMPLETE"));
    assert_eq!(progress_marker(&complete, 44, 44), "");
    assert_eq!(progress_note(&complete), None);

    let between = ProgressSnapshot { running: true, process_active: false, ..complete.clone() };
    assert_eq!(
        progress_note(&between),
        Some("in-process or between child commands; external-check controls remain armed")
    );
    let skipping = ProgressSnapshot { running: true, skip_category_pending: true, ..complete };
    assert!(progress_state(&skipping).contains("SKIPPING CATEGORY"));
}

#[test]
fn blocker_helpers_cover_empty_and_populated_collections() {
    let finding = Finding {
        rule_id: "QA-X".into(),
        severity: Severity::High,
        message: "bad".into(),
        path: None,
        line: None,
        detail: None,
    };
    let record = EvidenceRecord {
        family: "SAN".into(),
        check: "address".into(),
        status: EvidenceStatus::Failed,
        source: None,
        detail: Some("failure".into()),
    };
    assert!(no_blockers(&[], &[]));
    assert!(!no_blockers(&[&finding], &[]));
    assert!(!no_blockers(&[], &[&record]));

    let finding_text = finding_blockers_text(&[&finding]);
    assert!(finding_text.contains("QA-X"));
    assert!(finding_text.contains("bad"));
    assert!(finding_text.contains("[workspace]"));

    let evidence_text = evidence_blockers_text(&[&record]);
    assert!(evidence_text.contains("SAN:address"));
    assert!(evidence_text.contains("failure"));

    assert_eq!(mutation_blockers_text(&[]), "");
    let mutation_text = mutation_blockers_text(&[MutationItem {
        outcome: "Timeout".into(),
        path: None,
        line: None,
        mutation: "timeout mutation".into(),
    }]);
    assert!(mutation_text.contains("Mutation survivors/timeouts:"));
    assert!(mutation_text.contains("Timeout  timeout mutation  [workspace]"));
    assert_eq!(remaining_text("item", 20), "");
    assert_eq!(remaining_text("item", 21), "      ... 1 more item(s)\n");
}

#[test]
fn blocker_renderer_is_empty_for_clean_reports_and_complete_for_failures() {
    let mut report = empty_report();
    report.findings.push(Finding {
        rule_id: "QA-INFO".into(),
        severity: Severity::Medium,
        message: "non-blocking".into(),
        path: None,
        line: None,
        detail: None,
    });
    report.evidence.push(EvidenceRecord {
        family: "SAN".into(),
        check: "clean".into(),
        status: EvidenceStatus::Available,
        source: None,
        detail: None,
    });
    assert_eq!(blockers_text(&report), "");
    assert_eq!(print_blockers(&report), "");
    report.findings.clear();
    report.evidence.clear();

    report.findings.push(Finding {
        rule_id: "QA-COV-001".into(),
        severity: Severity::High,
        message: "coverage low".into(),
        path: Some("qa-out/llvm-cov.json".into()),
        line: None,
        detail: None,
    });
    report.evidence.push(EvidenceRecord {
        family: "SAN".into(),
        check: "address".into(),
        status: EvidenceStatus::Unavailable,
        source: None,
        detail: None,
    });
    report.mutations.push(MutationItem {
        outcome: "MissedMutant".into(),
        path: Some("src/lib.rs".into()),
        line: Some(12),
        mutation: "replace > with <".into(),
    });

    let text = blockers_text(&report);
    assert!(text.starts_with(&format!("  {RED}{BOLD}Blocking details{RESET}\n")));
    assert!(text.contains("QA-COV-001"));
    assert!(text.contains("qa-out/llvm-cov.json"));
    assert!(text.contains("SAN:address  no detail"));
    assert!(text.contains("MissedMutant  replace > with <  [src/lib.rs:12]"));
    assert!(text.ends_with("\n\n"));
    assert_eq!(print_blockers(&report), text);
}
