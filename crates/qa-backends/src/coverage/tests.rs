use super::*;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(1);

fn temp_dir(name: &str) -> PathBuf {
    let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
    let path =
        std::env::temp_dir().join(format!("urqa-coverage-{name}-{}-{id}", std::process::id()));
    if path.exists() {
        fs::remove_dir_all(&path).unwrap();
    }
    fs::create_dir_all(&path).unwrap();
    path
}

fn write_fixture(path: &Path) {
    let value = serde_json::json!({
        "data": [{
            "totals": {"lines": {"percent": 75.0}},
            "files": [{
                "filename": "C:\\work\\src\\lib.rs",
                "segments": [[10,1,1],[11,1,0],[11,2,3],[12,1,0],[null,1,7]]
            }]
        }]
    });
    fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
}

#[test]
fn parses_coverage_and_computes_function_percentages() {
    let root = temp_dir("parse");
    let path = root.join("llvm-cov.json");
    write_fixture(&path);
    let evidence = parse::parse(&path);
    assert_eq!(evidence.status, EvidenceStatus::Available);
    assert_eq!(evidence.percent, Some(75.0));
    assert_eq!(function_percent(&evidence, "C:\\work\\src\\lib.rs", 10, 12), Some(200.0 / 3.0));
    assert_eq!(function_percent(&evidence, "src/lib.rs", 10, 11), Some(100.0));
    assert_eq!(function_percent(&evidence, "src/lib.rs", 20, 21), None);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn partial_evidence_keeps_known_function_coverage_and_unknown_files_unknown() {
    let root = temp_dir("partial-functions");
    let path = root.join("llvm-cov.json");
    write_fixture(&path);
    let mut evidence = parse::parse(&path);
    evidence.status = EvidenceStatus::Partial;
    assert_eq!(function_percent(&evidence, "src/lib.rs", 10, 12), Some(200.0 / 3.0));
    assert_eq!(function_percent(&evidence, "failed-package/src/lib.rs", 1, 20), None);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn collect_honors_disabled_missing_and_existing_evidence() {
    let root = temp_dir("collect-existing");
    let out = root.join("qa-out");
    fs::create_dir_all(&out).unwrap();
    let mut config = QaConfig::default();
    config.coverage.mode = "off".into();
    assert_eq!(collect(&root, &config, &out, false).status, EvidenceStatus::Disabled);

    config.coverage.mode = "existing".into();
    let missing = collect(&root, &config, &out, false);
    assert_eq!(missing.status, EvidenceStatus::Unavailable);
    assert!(missing.error.as_deref().is_some_and(|error| error.contains("not found")));

    write_fixture(&out.join("llvm-cov.json"));
    let partial = collect(&root, &config, &out, false);
    assert_eq!(partial.status, EvidenceStatus::Partial);
    assert_eq!(partial.percent, Some(75.0));
    assert!(partial.scope_percent.is_none());
    assert!(partial.error.as_deref().is_some_and(|error| error.contains("scope is unknown")));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn existing_mode_restores_partial_scope_manifest_instead_of_upgrading_to_complete() {
    let root = temp_dir("partial-existing");
    let out = root.join("qa-out");
    fs::create_dir_all(&out).unwrap();
    write_fixture(&out.join("llvm-cov.json"));
    let manifest = serde_json::json!({
        "schema":1,
        "status":"partial",
        "workspace_packages":5,
        "eligible_packages":4,
        "covered_packages":3,
        "failed_packages":1,
        "not_applicable_packages":1,
        "eligible_source_loc":100,
        "covered_source_loc":75,
        "profile_count":8,
        "covered_package_roots":["C:/work"],
        "attempts":[]
    });
    fs::write(out.join("coverage-failures.json"), serde_json::to_vec(&manifest).unwrap()).unwrap();
    let mut config = QaConfig::default();
    config.coverage.mode = "existing".into();
    let evidence = collect(&root, &config, &out, true);
    assert_eq!(evidence.status, EvidenceStatus::Partial);
    assert_eq!(evidence.covered_packages, 3);
    assert_eq!(evidence.failed_packages, 1);
    assert_eq!(evidence.scope_percent, Some(75.0));
    assert_eq!(evidence.profile_count, 8);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn old_partial_manifest_without_package_roots_withholds_per_function_coverage() {
    let root = temp_dir("partial-existing-without-roots");
    let out = root.join("qa-out");
    fs::create_dir_all(&out).unwrap();
    write_fixture(&out.join("llvm-cov.json"));
    let manifest = serde_json::json!({
        "schema":1,
        "status":"partial",
        "eligible_packages":2,
        "covered_packages":1,
        "failed_packages":1,
        "eligible_source_loc":100,
        "covered_source_loc":50,
        "profile_count":4,
        "attempts":[]
    });
    fs::write(out.join("coverage-failures.json"), serde_json::to_vec(&manifest).unwrap()).unwrap();
    let mut config = QaConfig::default();
    config.coverage.mode = "existing".into();
    let evidence = collect(&root, &config, &out, true);
    assert_eq!(evidence.status, EvidenceStatus::Partial);
    assert!(function_percent(&evidence, "C:/work/src/lib.rs", 10, 12).is_none());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn malformed_or_missing_coverage_is_failed_and_has_no_function_percent() {
    let root = temp_dir("bad");
    let path = root.join("llvm-cov.json");
    fs::write(&path, "not-json").unwrap();
    let bad = parse::parse(&path);
    assert_eq!(bad.status, EvidenceStatus::Failed);
    assert!(bad.error.as_deref().is_some_and(|error| error.contains("expected")));
    assert!(function_percent(&bad, "anything.rs", 1, 2).is_none());
    let missing = parse::parse(&root.join("absent.json"));
    assert_eq!(missing.status, EvidenceStatus::Failed);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn coverage_detail_never_hides_partial_scope() {
    let evidence = CoverageEvidence {
        status: EvidenceStatus::Partial,
        percent: Some(96.0),
        scope_percent: Some(71.4),
        eligible_packages: 83,
        covered_packages: 73,
        failed_packages: 6,
        profile_count: 75,
        ..CoverageEvidence::default()
    };
    let text = detail(&evidence);
    assert!(text.contains("coverage partial"));
    assert!(text.contains("71.4% source scope"));
    assert!(text.contains("6 failed package"));
    assert!(text.contains("75 raw profile"));
}
