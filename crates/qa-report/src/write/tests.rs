use super::*;
use qa_model::{
    CoverageSummary, EvidenceRecord, EvidenceStatus, Finding, FuzzSummary, MutationSummary,
    QaReport, Severity, SummaryMetrics,
};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(1);

fn temp_dir() -> PathBuf {
    let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("urqa-report-{}-{id}", std::process::id()));
    if path.exists() {
        fs::remove_dir_all(&path).unwrap();
    }
    fs::create_dir_all(&path).unwrap();
    path
}

fn report() -> QaReport {
    QaReport {
        schema: 21,
        generated_unix_seconds: 1,
        workspace: "workspace".into(),
        profile: "strict".into(),
        summary: SummaryMetrics {
            health_score: 100.0,
            health_is_provisional: false,
            average_file_loc: 1.0,
            files_over_loc: 0,
            average_cc: 1.0,
            functions_over_cc: 0,
            average_crap: Some(1.0),
            functions_over_crap: Some(0),
            total_tests: 1,
            invalid_tests: 0,
            coverage: CoverageSummary {
                percent: Some(100.0),
                functions_below_threshold: Some(0),
                source: Some("cov".into()),
                status: EvidenceStatus::Available,
                ..CoverageSummary::default()
            },
            mutation: MutationSummary {
                status: EvidenceStatus::Available,
                caught: 1,
                missed: 0,
                timeout: 0,
                unviable: 0,
                score_percent: Some(100.0),
                source: Some("mut".into()),
            },
            fuzz: FuzzSummary {
                target_count: 0,
                critical_targets_missing: 0,
                regression_artifacts: 0,
                unpersisted_crashes: 0,
                property_test_count: 0,
                status: EvidenceStatus::NotApplicable,
            },
            duplicate_percent: 0.0,
            dead_code_percent: 0.0,
            high_findings: 1,
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
        evidence: vec![EvidenceRecord {
            family: "SAN".into(),
            check: "address".into(),
            status: EvidenceStatus::Available,
            source: None,
            detail: Some("ok".into()),
        }],
        findings: vec![Finding {
            rule_id: "QA-SAFE-001".into(),
            severity: Severity::High,
            message: "unsafe".into(),
            path: Some("src/lib.rs".into()),
            line: Some(1),
            detail: None,
        }],
    }
}

#[test]
fn write_reports_emits_complete_machine_and_human_readable_bundle() {
    let root = temp_dir();
    let config = QaConfig::default();
    let out = write_reports(&root, &config, &report()).unwrap();
    for name in [
        "summary.txt",
        "report.json",
        "metrics.json",
        "coverage.json",
        "mutation.json",
        "fuzz.json",
        "duplicates.json",
        "dead-code.json",
        "findings.json",
        "evidence.json",
        "safety.json",
        "sanitizers.json",
        "effective-config.toml",
    ] {
        assert!(out.join(name).is_file(), "missing {name}");
    }
    let safety: serde_json::Value =
        serde_json::from_slice(&fs::read(out.join("safety.json")).unwrap()).unwrap();
    assert_eq!(safety.as_array().unwrap().len(), 1);
    let sanitizers: serde_json::Value =
        serde_json::from_slice(&fs::read(out.join("sanitizers.json")).unwrap()).unwrap();
    assert_eq!(sanitizers.as_array().unwrap().len(), 1);
    assert!(fs::read_to_string(out.join("summary.txt")).unwrap().contains("Health: 100.0%"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn family_and_json_helpers_filter_and_serialize() {
    let findings = vec![
        Finding {
            rule_id: "QA-A-001".into(),
            severity: Severity::Low,
            message: "a".into(),
            path: None,
            line: None,
            detail: None,
        },
        Finding {
            rule_id: "QA-B-001".into(),
            severity: Severity::Low,
            message: "b".into(),
            path: None,
            line: None,
            detail: None,
        },
    ];
    assert_eq!(family(&findings, "QA-A-").len(), 1);
    let root = temp_dir();
    let path = root.join("value.json");
    json(path.clone(), &serde_json::json!({"ok": true})).unwrap();
    let value = serde_json::from_slice::<serde_json::Value>(&fs::read(path).unwrap()).unwrap();
    assert_eq!(value["ok"], serde_json::Value::Bool(true));
    fs::remove_dir_all(root).unwrap();
}
