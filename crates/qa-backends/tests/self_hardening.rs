use qa_policy::{QaConfig, SelfHardeningConfig};
use std::{
    fmt::Write as _,
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_WORKSPACE_ID: AtomicU64 = AtomicU64::new(1);

fn workspace() -> PathBuf {
    let id = NEXT_WORKSPACE_ID.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("urqa-self-{}-{id}", std::process::id()));
    if root.exists() {
        fs::remove_dir_all(&root).unwrap();
    }
    fs::create_dir_all(root.join("schemas")).unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/lib.rs"), "pub fn healthy() {}\n").unwrap();
    for name in ["qa-config.schema.json", "qa-report.schema.json", "rule-registry.schema.json"] {
        fs::write(root.join("schemas").join(name), "{}\n").unwrap();
    }
    fs::write(root.join("run-all-tests.sh"), "#!/bin/sh\n").unwrap();
    fs::write(root.join("run-all-tests.cmd"), "@echo off\r\n").unwrap();
    fs::create_dir_all(root.join("scripts")).unwrap();
    fs::write(
        root.join("scripts/install-qa-tools.ps1"),
        "function Install-CargoTool([string]$Executable) { Get-Command $Executable -ErrorAction SilentlyContinue }\r\n",
    ).unwrap();
    fs::write(
        root.join("scripts/install-qa-tools.sh"),
        "install_tool(){ local executable=\"$1\"; command -v \"$executable\" >/dev/null 2>&1; }\n",
    )
    .unwrap();
    fs::create_dir_all(root.join("fixtures/pass/mir")).unwrap();
    fs::create_dir_all(root.join("fixtures/fail/mir")).unwrap();
    fs::write(root.join("fixtures/pass/mir/no_panic_no_alloc.rs"), "fn ok() {}\n").unwrap();
    fs::write(root.join("fixtures/fail/mir/panic_and_alloc.rs"), "fn bad(){ panic!(\"x\"); }\n")
        .unwrap();
    fs::create_dir_all(root.join("crates/qa-rules/src")).unwrap();
    let mut registry = String::new();
    for rule in &qa_rules::rule_registry().rules {
        writeln!(registry, "\"{}\"", rule.id).unwrap();
    }
    fs::write(root.join("crates/qa-rules/src/registry.rs"), registry).unwrap();
    root
}

#[test]
fn self_hardening_validates_registry_schemas_sprawl_and_launchers() {
    let root = workspace();
    let config = QaConfig {
        self_hardening: SelfHardeningConfig {
            require_clean_tree: false,
            ..SelfHardeningConfig::default()
        },
        ..QaConfig::default()
    };
    let records =
        qa_backends::self_hardening::run(&root, &config, true, &qa_rules::rule_registry());
    assert!(records.iter().all(|r| r.status != qa_model::EvidenceStatus::Failed));
    for check in [
        "rule-registry",
        "source-sprawl",
        "launcher:run-all-tests.sh",
        "launcher:run-all-tests.cmd",
        "launcher-tool-probe",
    ] {
        assert!(records.iter().any(|r| r.check == check), "missing {check}");
    }
    fs::remove_dir_all(root).expect("remove self-hardening fixture workspace");
}
