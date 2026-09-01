use super::*;
use qa_model::{EvidenceKind, RuleDefinition};
use qa_policy::SelfHardeningConfig;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(1);

fn temp_dir(name: &str) -> PathBuf {
    let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
    let path =
        std::env::temp_dir().join(format!("urqa-self-unit-{name}-{}-{id}", std::process::id()));
    if path.exists() {
        fs::remove_dir_all(&path).unwrap();
    }
    fs::create_dir_all(&path).unwrap();
    path
}

fn rule(id: &str, family: &str) -> RuleDefinition {
    RuleDefinition {
        id: id.into(),
        name: id.into(),
        family: family.into(),
        evidence: EvidenceKind::Static,
        description: String::new(),
    }
}

#[test]
fn disabled_and_pending_suite_states_are_reported() {
    let root = temp_dir("states");
    let registry = qa_rules::rule_registry();
    let mut config = QaConfig::default();
    config.self_hardening.enabled = false;
    assert_eq!(run(&root, &config, true, &registry)[0].status, EvidenceStatus::Disabled);
    config.self_hardening.enabled = true;
    assert_eq!(run(&root, &config, false, &registry)[0].status, EvidenceStatus::Unknown);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rule_registry_check_detects_duplicate_ids_missing_families_and_source_drift() {
    let root = temp_dir("registry");
    fs::create_dir_all(root.join("crates/qa-rules/src")).unwrap();
    fs::write(root.join("crates/qa-rules/src/registry.rs"), "\"QA-METRIC-001\"\n").unwrap();
    let registry = RuleRegistry {
        rules: vec![rule("QA-METRIC-001", "METRIC"), rule("QA-METRIC-001", "METRIC")],
    };
    let mut records = Vec::new();
    check_rule_registry(&root, &registry, &mut records);
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].status, EvidenceStatus::Failed);
    assert_eq!(records[1].status, EvidenceStatus::Available);

    fs::write(root.join("crates/qa-rules/src/registry.rs"), "\"QA-OTHER-001\"\n").unwrap();
    records.clear();
    check_rule_registry(&root, &registry, &mut records);
    assert_eq!(records[1].status, EvidenceStatus::Failed);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn schema_sprawl_launcher_probe_and_golden_checks_cover_failure_and_success() {
    let root = temp_dir("checks");
    let mut out = Vec::new();
    check_schemas(&root, &mut out);
    assert_eq!(out.len(), 3);
    assert!(out.iter().all(|record| record.status == EvidenceStatus::Failed));

    fs::create_dir_all(root.join("schemas")).unwrap();
    for name in ["qa-config.schema.json", "qa-report.schema.json", "rule-registry.schema.json"] {
        fs::write(root.join("schemas").join(name), "{}\n").unwrap();
    }
    out.clear();
    check_schemas(&root, &mut out);
    assert_eq!(out.len(), 3);
    assert!(out.iter().all(|record| record.status == EvidenceStatus::Available));

    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/small.rs"), "fn small() {}\n").unwrap();
    let mut config = QaConfig::default();
    config.self_hardening.max_source_file_loc = 2;
    out.clear();
    check_source_sprawl(&root, &config, &mut out);
    assert_eq!(out[0].status, EvidenceStatus::Available);
    fs::write(root.join("src/large.rs"), "1\n2\n3\n").unwrap();
    out.clear();
    check_source_sprawl(&root, &config, &mut out);
    assert_eq!(out[0].status, EvidenceStatus::Failed);

    out.clear();
    check_launchers(&root, &mut out);
    assert!(out.iter().all(|record| record.status == EvidenceStatus::Failed));
    fs::write(root.join("run-all-tests.sh"), "#!/bin/sh\n").unwrap();
    fs::write(root.join("run-all-tests.cmd"), "@echo off\r\n").unwrap();
    out.clear();
    check_launchers(&root, &mut out);
    assert!(out.iter().all(|record| record.status == EvidenceStatus::Available));

    fs::create_dir_all(root.join("scripts")).unwrap();
    fs::write(
        root.join("scripts/install-qa-tools.ps1"),
        "Get-Command $Executable -ErrorAction SilentlyContinue\n",
    )
    .unwrap();
    fs::write(root.join("scripts/install-qa-tools.sh"), "command -v \"$executable\"\n").unwrap();
    out.clear();
    check_tool_installer_probe_contract(&root, &mut out);
    assert_eq!(out[0].status, EvidenceStatus::Available);
    fs::write(root.join("scripts/install-qa-tools.sh"), "cargo \"$sub\" --version\n").unwrap();
    out.clear();
    check_tool_installer_probe_contract(&root, &mut out);
    assert_eq!(out[0].status, EvidenceStatus::Failed);

    out.clear();
    check_golden_mir_fixtures(&root, &mut out);
    assert_eq!(out.len(), 2);
    assert!(out.iter().all(|record| record.status == EvidenceStatus::Failed));
    fs::create_dir_all(root.join("fixtures/pass/mir")).unwrap();
    fs::create_dir_all(root.join("fixtures/fail/mir")).unwrap();
    fs::write(root.join("fixtures/pass/mir/no_panic_no_alloc.rs"), "fn ok(){}\n").unwrap();
    fs::write(root.join("fixtures/fail/mir/panic_and_alloc.rs"), "fn bad(){}\n").unwrap();
    out.clear();
    check_golden_mir_fixtures(&root, &mut out);
    assert_eq!(out.len(), 2);
    assert!(out.iter().all(|record| record.status == EvidenceStatus::Available));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn helper_parsers_and_clean_tree_check_are_deterministic() {
    let ids = extract_rule_ids("\"QA-ONE-001\" \"QA-TWO-002\" \"qa-lower-1\" \"QA-ONE-001\"");
    assert_eq!(ids.len(), 2);
    assert!(ids.contains("QA-ONE-001"));
    assert!(excluded(Path::new("root/target/file.rs")));
    assert!(!excluded(Path::new("root/src/file.rs")));

    let root = temp_dir("git");
    let mut out = Vec::new();
    check_git_clean(&root, &mut out);
    assert_eq!(out.len(), 1);
    assert!(matches!(out[0].status, EvidenceStatus::Available | EvidenceStatus::Unknown));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn run_can_disable_individual_integrity_checks() {
    let root = temp_dir("minimal");
    let config = QaConfig {
        self_hardening: SelfHardeningConfig {
            require_clean_tree: false,
            require_rule_registry_integrity: false,
            require_report_schema: false,
            max_source_file_loc: 600,
            enabled: true,
        },
        ..QaConfig::default()
    };
    let records = run(&root, &config, true, &qa_rules::rule_registry());
    assert!(records.iter().all(|record| record.check != "rule-registry"));
    assert!(records.iter().all(|record| !record.check.starts_with("schema:")));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn registry_status_requires_no_duplicates_and_every_family() {
    let root = temp_dir("registry-boundaries");
    let complete = RuleRegistry {
        rules: FAMILIES
            .iter()
            .enumerate()
            .map(|(index, family)| rule(&format!("QA-{index:03}"), family))
            .collect(),
    };
    let mut out = Vec::new();
    check_rule_registry(&root, &complete, &mut out);
    assert_eq!(out[0].status, EvidenceStatus::Available);

    let mut duplicate = complete.clone();
    duplicate.rules.push(duplicate.rules[0].clone());
    out.clear();
    check_rule_registry(&root, &duplicate, &mut out);
    assert_eq!(out[0].status, EvidenceStatus::Failed);

    let mut missing = complete.clone();
    missing.rules.pop();
    out.clear();
    check_rule_registry(&root, &missing, &mut out);
    assert_eq!(out[0].status, EvidenceStatus::Failed);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn source_sprawl_is_exact_and_ignores_non_source_or_excluded_trees() {
    let root = temp_dir("sprawl-boundaries");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("target")).unwrap();
    fs::write(root.join("src/exact.rs"), "one\ntwo\n").unwrap();
    fs::write(root.join("target/huge.rs"), "1\n2\n3\n4\n").unwrap();
    fs::write(root.join("src/huge.txt"), "1\n2\n3\n4\n").unwrap();
    let mut config = QaConfig::default();
    config.self_hardening.max_source_file_loc = 2;
    let mut out = Vec::new();
    check_source_sprawl(&root, &config, &mut out);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].status, EvidenceStatus::Available);

    fs::write(root.join("src/over.rs"), "1\n2\n3\n").unwrap();
    out.clear();
    check_source_sprawl(&root, &config, &mut out);
    assert_eq!(out[0].status, EvidenceStatus::Failed);
    assert!(out[0].detail.as_deref().is_some_and(|detail| detail.contains("over.rs")));
    assert!(out[0].detail.as_deref().is_some_and(|detail| !detail.contains("huge.rs")));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn installer_probe_requires_each_positive_and_negative_contract() {
    let root = temp_dir("installer-boundaries");
    fs::create_dir_all(root.join("scripts")).unwrap();
    let ps = root.join("scripts/install-qa-tools.ps1");
    let sh = root.join("scripts/install-qa-tools.sh");
    fs::write(&ps, "Get-Command $Executable -ErrorAction SilentlyContinue\n").unwrap();
    fs::write(&sh, "command -v \"$executable\"\n").unwrap();
    let mut out = Vec::new();
    check_tool_installer_probe_contract(&root, &mut out);
    assert_eq!(out[0].status, EvidenceStatus::Available);

    fs::write(&ps, "Write-Host no-probe\n").unwrap();
    out.clear();
    check_tool_installer_probe_contract(&root, &mut out);
    assert_eq!(out[0].status, EvidenceStatus::Failed);

    fs::write(&ps, "Get-Command $Executable -ErrorAction SilentlyContinue\n").unwrap();
    fs::write(&sh, "echo no-probe\n").unwrap();
    out.clear();
    check_tool_installer_probe_contract(&root, &mut out);
    assert_eq!(out[0].status, EvidenceStatus::Failed);
    fs::remove_dir_all(root).unwrap();
}
