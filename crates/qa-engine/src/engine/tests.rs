use super::*;
use qa_backends::{coverage::CoverageEvidence, fuzz::FuzzBackend, mutation::MutationEvidence};
use qa_rules::RuleOutput;
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(1);

fn workspace() -> PathBuf {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("urqa-engine-{}-{id}", std::process::id()));
    if root.exists() {
        fs::remove_dir_all(&root).unwrap();
    }
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("qa-out")).unwrap();
    fs::create_dir_all(root.join("mutants.out")).unwrap();
    fs::write(
        root.join("src/lib.rs"),
        "pub fn covered(x: bool) -> u8 { if x { 1 } else { 2 } }\n#[test] fn test_covered(){ assert_eq!(covered(true),1); }\n",
    )
    .unwrap();
    root
}

#[test]
fn default_run_options_generate_fresh_coverage_and_existing_override_reuses_it() {
    assert!(RunOptions::default().force_coverage);
    assert!(!RunOptions::existing_coverage().force_coverage);
}

#[test]
fn crap_tracks_complexity_and_uncovered_risk() {
    assert!((crap(4, 0.0) - 20.0).abs() < f64::EPSILON);
    assert!((crap(4, 100.0) - 4.0).abs() < f64::EPSILON);
    assert!((crap(4, 150.0) - 4.0).abs() < f64::EPSILON);
    assert!((crap(4, -10.0) - 20.0).abs() < f64::EPSILON);
}

#[test]
fn arithmetic_helpers_cover_empty_nonempty_and_capped_penalties() {
    assert_eq!(avg([1usize, 2, 3].into_iter(), 3), 2.0);
    assert_eq!(avg(std::iter::empty(), 0), 0.0);
    assert_eq!(ratio(1, 4), 25.0);
    assert_eq!(ratio(1, 0), 0.0);
    assert_eq!(pen(1, 4, 40.0), 10.0);
    assert_eq!(pen(100, 1, 40.0), 40.0);
    assert_eq!(pen(1, 0, 40.0), 0.0);
}

#[test]
fn evidence_record_helpers_preserve_scores_errors_and_fuzz_status() {
    let coverage = CoverageEvidence {
        status: EvidenceStatus::Available,
        percent: Some(91.25),
        ..Default::default()
    };
    assert!(coverage_record(&coverage).detail.unwrap().contains("91.25%"));
    let coverage = CoverageEvidence {
        status: EvidenceStatus::Failed,
        error: Some("boom".into()),
        ..Default::default()
    };
    assert_eq!(coverage_record(&coverage).detail.as_deref(), Some("boom"));

    let mutation = MutationEvidence {
        status: EvidenceStatus::Available,
        score_percent: Some(92.5),
        ..Default::default()
    };
    assert!(mutation_record(&mutation).detail.unwrap().contains("92.5%"));
    let mutation = MutationEvidence {
        status: EvidenceStatus::Failed,
        error: Some("bad".into()),
        ..Default::default()
    };
    assert_eq!(mutation_record(&mutation).detail.as_deref(), Some("bad"));

    let rules = RuleOutput::default();
    let fuzz = FuzzBackend { targets: BTreeMap::new(), errors: BTreeMap::new() };
    assert_eq!(fuzz_record(&rules, &fuzz).status, EvidenceStatus::NotApplicable);

    let mut rules = RuleOutput::default();
    rules.fuzz_targets.push(qa_model::FuzzTargetEvidence {
        name: "parser".into(),
        path: "fuzz.rs".into(),
        line: 1,
        reaches_production: true,
        critical_targets: vec![],
        build_status: EvidenceStatus::Unknown,
    });
    let mut fuzz = FuzzBackend { targets: BTreeMap::new(), errors: BTreeMap::new() };
    fuzz.targets.insert("parser".into(), EvidenceStatus::Available);
    assert_eq!(fuzz_record(&rules, &fuzz).status, EvidenceStatus::Available);
    fuzz.targets.insert("parser".into(), EvidenceStatus::Failed);
    fuzz.errors.insert("parser".into(), "failed".into());
    let record = fuzz_record(&rules, &fuzz);
    assert_eq!(record.status, EvidenceStatus::Failed);
    assert!(record.detail.unwrap().contains("parser: failed"));
}

#[test]
fn run_with_existing_evidence_computes_coverage_crap_and_mutation_blockers() {
    let root = workspace();
    let filename = root.join("src/lib.rs").display().to_string().replace('\\', "/");
    let segments = (1..=2)
        .map(|line| serde_json::json!([line, 1, if line == 1 { 1 } else { 0 }, true, true, false]))
        .collect::<Vec<_>>();
    let coverage = serde_json::json!({
        "data": [{
            "totals": {"lines": {"percent": 50.0}},
            "files": [{"filename": filename, "segments": segments}]
        }]
    });
    fs::write(root.join("qa-out/llvm-cov.json"), serde_json::to_vec(&coverage).unwrap()).unwrap();
    let mutants = serde_json::json!({
        "outcomes": [
            {"summary": "CaughtMutant"},
            {
                "summary": "MissedMutant",
                "mutant": {"file": "src/lib.rs", "line": 1, "description": "flip branch"}
            },
            {
                "summary": "Timeout",
                "mutant": {"file": "src/lib.rs", "line": 1, "description": "timeout branch"}
            }
        ]
    });
    fs::write(root.join("mutants.out/outcomes.json"), serde_json::to_vec(&mutants).unwrap())
        .unwrap();

    let mut config = QaConfig::default();
    config.metrics.coverage_percent = 90.0;
    config.metrics.crap = 1.0;
    config.sanitizers.mode = "off".into();
    config.mir.mode = "off".into();
    config.constant_time.mode = "off".into();
    config.hardening.enabled = false;
    config.reproducibility.enabled = false;
    config.generated.verify = false;
    config.self_hardening.enabled = false;
    let report = run_with_options(&root, &config, &RunOptions::existing_coverage());
    assert_eq!(report.summary.coverage.percent, Some(50.0));
    assert_eq!(report.summary.mutation.caught, 1);
    assert_eq!(report.summary.mutation.missed, 1);
    assert_eq!(report.summary.mutation.timeout, 1);
    let coverage_finding = report.findings.iter().find(|f| f.rule_id == "QA-COV-001").unwrap();
    assert!(coverage_finding.message.contains("50.00% is below 90.00%"));
    assert!(report.findings.iter().any(|f| f.rule_id == "QA-MUT-001"));
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.rule_id == "QA-MUT-002" && f.severity == qa_model::Severity::Medium)
    );
    assert!(report.findings.iter().any(|f| f.rule_id == "QA-MUT-003"));
    assert!(report.findings.iter().any(|f| f.rule_id == "QA-METRIC-004"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn coverage_display_floors_below_threshold_values_instead_of_rounding_them_up() {
    assert_eq!(floor_percent(89.9999), 89.99);
    assert_eq!(floor_percent(90.0), 90.0);
}

#[test]
fn progress_phase_helpers_advance_categories_and_refresh_summary() {
    let control = RunControl::new(3);
    let value = run_phase!(Some(&control), "first", 7usize);
    assert_eq!(value, 7);

    let mut evidence = Vec::new();
    evidence.push(run_phase!(
        Some(&control),
        "second",
        EvidenceRecord {
            family: "TEST".into(),
            check: "record".into(),
            status: EvidenceStatus::Available,
            source: None,
            detail: None,
        }
    ));
    assert_eq!(evidence.len(), 1);

    let rules = RuleOutput::default();
    let coverage = CoverageEvidence::default();
    let mutation = MutationEvidence::default();
    refresh_progress(Some(&control), &QaConfig::default(), &rules, &coverage, &mutation, &evidence);
    let snapshot = control.snapshot();
    assert_eq!(snapshot.completed, 2);
    assert_eq!(snapshot.evidence_count, 1);
    assert!(snapshot.summary.is_some());
}

#[test]
fn forced_coverage_failure_skips_expensive_mutation_but_not_successful_coverage() {
    let options = RunOptions { force_coverage: true, run_mutation: true, ..Default::default() };
    for status in [EvidenceStatus::Failed, EvidenceStatus::Unavailable] {
        let coverage = CoverageEvidence { status, ..Default::default() };
        assert!(should_skip_mutation_after_coverage(&options, &coverage));
        let skipped = skipped_mutation_after_coverage(&coverage);
        assert_eq!(skipped.status, EvidenceStatus::Unknown);
        assert!(skipped.error.as_deref().is_some_and(|error| error.contains("coverage")));
    }

    let available = CoverageEvidence { status: EvidenceStatus::Available, ..Default::default() };
    assert!(!should_skip_mutation_after_coverage(&options, &available));
    let mutation_only = RunOptions { run_mutation: true, ..RunOptions::existing_coverage() };
    let failed = CoverageEvidence { status: EvidenceStatus::Failed, ..Default::default() };
    assert!(!should_skip_mutation_after_coverage(&mutation_only, &failed));
}

#[test]
fn live_summary_accepts_coverage_and_crap_before_mutation_starts() {
    let control = RunControl::new(RUN_CATEGORY_COUNT);
    let mut rules =
        RuleOutput { functions: vec![function_metric(false, 4, None)], ..Default::default() };
    let mut files = BTreeMap::new();
    files.insert("src/lib.rs".into(), BTreeMap::from([(1, 1)]));
    let coverage = CoverageEvidence {
        status: EvidenceStatus::Available,
        percent: Some(95.0),
        files,
        ..Default::default()
    };
    apply_coverage(&mut rules, &QaConfig::default(), &coverage);
    refresh_progress(
        Some(&control),
        &QaConfig::default(),
        &rules,
        &coverage,
        &MutationEvidence::default(),
        &[],
    );

    let summary = control.snapshot().summary.unwrap();
    assert!(!summary.health_is_provisional);
    assert_eq!(summary.coverage.percent, Some(95.0));
    assert_eq!(summary.average_crap, Some(4.0));
    assert_eq!(summary.functions_over_crap, Some(0));
}

#[test]
fn progress_run_completes_every_category_without_forcing_external_tools() {
    let root = workspace();
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"progress-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .unwrap();
    let mut config = QaConfig::default();
    config.coverage.mode = "off".into();
    config.mutation.mode = "off".into();
    config.sanitizers.mode = "off".into();
    config.mir.mode = "off".into();
    config.constant_time.mode = "off".into();
    config.hardening.enabled = false;
    config.reproducibility.enabled = false;
    config.generated.verify = false;
    config.self_hardening.enabled = false;
    let control = RunControl::new(RUN_CATEGORY_COUNT);
    let report = run_with_progress(&root, &config, &RunOptions::default(), &control);
    let snapshot = control.snapshot();
    assert!(!snapshot.running);
    assert_eq!(snapshot.completed, RUN_CATEGORY_COUNT);
    assert_eq!(snapshot.category, "complete");
    assert_eq!(report.summary.coverage.status, EvidenceStatus::Disabled);
    assert_eq!(report.summary.mutation.status, EvidenceStatus::Disabled);
    fs::remove_dir_all(root).unwrap();
}

fn function_metric(
    is_test: bool,
    cyclomatic: usize,
    coverage_percent: Option<f64>,
) -> qa_model::FunctionMetric {
    qa_model::FunctionMetric {
        path: "src/lib.rs".into(),
        name: "f".into(),
        qualified_name: "f".into(),
        line: 1,
        end_line: 1,
        logical_loc: 1,
        statements: 1,
        parameters: 0,
        generic_parameters: 0,
        cyclomatic,
        cognitive: 0,
        coverage_percent,
        crap: coverage_percent.map(|percent| crap(cyclomatic, percent)),
        is_test,
        is_public: false,
        is_async: false,
        attributes: vec![],
    }
}

#[test]
fn fuzz_status_and_dynamic_refresh_are_observable_before_later_phases() {
    let mut rules = RuleOutput::default();
    rules.fuzz_targets.push(qa_model::FuzzTargetEvidence {
        name: "parser".into(),
        path: "fuzz.rs".into(),
        line: 1,
        reaches_production: true,
        critical_targets: vec![],
        build_status: EvidenceStatus::Unknown,
    });
    rules.fuzz_targets.push(qa_model::FuzzTargetEvidence {
        name: "unchanged".into(),
        path: "fuzz.rs".into(),
        line: 2,
        reaches_production: true,
        critical_targets: vec![],
        build_status: EvidenceStatus::Unknown,
    });
    let mut fuzz = FuzzBackend { targets: BTreeMap::new(), errors: BTreeMap::new() };
    fuzz.targets.insert("parser".into(), EvidenceStatus::Available);
    apply_fuzz_status(&mut rules, &fuzz);
    assert_eq!(rules.fuzz_targets[0].build_status, EvidenceStatus::Available);
    assert_eq!(rules.fuzz_targets[1].build_status, EvidenceStatus::Unknown);

    let root = workspace();
    let control = RunControl::new(RUN_CATEGORY_COUNT);
    let coverage = CoverageEvidence {
        status: EvidenceStatus::Available,
        percent: Some(95.0),
        ..Default::default()
    };
    let mutation = MutationEvidence {
        status: EvidenceStatus::Available,
        caught: 10,
        score_percent: Some(100.0),
        ..Default::default()
    };
    let config = QaConfig::default();
    let options = RunOptions::default();
    let context = DynamicEvidenceContext {
        workspace: &root,
        config: &config,
        options: &options,
        progress: Some(&control),
        coverage: &coverage,
        mutation: &mutation,
        artifact_root: &root,
    };
    let evidence = vec![EvidenceRecord {
        family: "TEST".into(),
        check: "refresh".into(),
        status: EvidenceStatus::Available,
        source: None,
        detail: None,
    }];
    refresh_dynamic_progress(&context, &rules, &evidence);
    let snapshot = control.snapshot();
    assert_eq!(snapshot.evidence_count, 1);
    assert_eq!(snapshot.summary.unwrap().coverage.percent, Some(95.0));
    fs::remove_dir_all(root).unwrap();
}

mod reporting;

#[test]
fn dynamic_evidence_collection_cannot_be_silently_skipped() {
    let root = workspace();
    let config = QaConfig::default();
    let options = RunOptions::existing_coverage();
    let coverage = CoverageEvidence::default();
    let mutation = MutationEvidence::default();
    let artifact_root = root.join("qa-out");
    let context = DynamicEvidenceContext {
        workspace: &root,
        config: &config,
        options: &options,
        progress: None,
        coverage: &coverage,
        mutation: &mutation,
        artifact_root: &artifact_root,
    };
    let mut rules = RuleOutput::default();
    let mut evidence = Vec::new();
    collect_dynamic_evidence(&context, &mut rules, &mut evidence);
    assert!(!evidence.is_empty());
    assert!(evidence.iter().any(|record| record.family == "CONC"));
    assert!(evidence.iter().any(|record| record.family == "MIR"));
    fs::remove_dir_all(root).unwrap();
}
