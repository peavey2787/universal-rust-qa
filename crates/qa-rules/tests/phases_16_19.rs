use qa_policy::{HardwareConfig, PerformanceConfig, QaConfig};
use qa_rules::analyze;
use qa_syntax::discover;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_WORKSPACE_ID: AtomicU64 = AtomicU64::new(1);

fn workspace(source: &str) -> PathBuf {
    let id = NEXT_WORKSPACE_ID.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("urqa-p16-{}-{id}", std::process::id()));
    if root.exists() {
        fs::remove_dir_all(&root).unwrap();
    }
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/lib.rs"), source).unwrap();
    root
}

#[test]
fn hardware_mmio_and_isr_rules() {
    let root = workspace(
        r#"
#[qa_attr::mmio] unsafe fn reg(){ let p=0x60000000usize as *mut u32; *p=1; }
#[qa_attr::interrupt] fn irq(){ let x=[0u8;4096]; let _=x; panic!("bad"); }
"#,
    );
    let c = QaConfig {
        hardware: HardwareConfig {
            enabled: true,
            interrupt_stack_budget_bytes: 1024,
            ..HardwareConfig::default()
        },
        ..QaConfig::default()
    };
    let o = analyze(&discover(&root), &c);
    let ids = o.findings.iter().map(|f| f.rule_id.as_str()).collect::<Vec<_>>();
    assert!(ids.contains(&"QA-HW-001"));
    assert!(ids.contains(&"QA-HW-002"));
    assert!(ids.contains(&"QA-HW-004"));
    fs::remove_dir_all(root).expect("remove phase 16-19 fixture workspace");
}

#[test]
fn performance_false_sharing_rule() {
    let root =
        workspace("use std::sync::atomic::AtomicU64; struct Hot { a: AtomicU64, b: AtomicU64 }");
    let c = QaConfig {
        performance: PerformanceConfig { enabled: true, ..PerformanceConfig::default() },
        ..QaConfig::default()
    };
    let o = analyze(&discover(&root), &c);
    assert!(o.findings.iter().any(|f| f.rule_id == "QA-PERF-001"));
    fs::remove_dir_all(root).expect("remove phase 16-19 fixture workspace");
}

#[test]
fn release_profile_and_snapshot_rules() {
    let root = workspace("pub fn api()->Result<(),()>{Ok(())}");
    fs::write(root.join("Cargo.toml"),"[package]\nname='fixture'\nversion='0.1.0'\nedition='2024'\n[profile.release]\noverflow-checks=false\n").unwrap();
    fs::write(root.join("bad.snap.new"), "private_key = 'x'").unwrap();
    let c = QaConfig::default();
    let o = analyze(&discover(&root), &c);
    assert!(o.findings.iter().any(|f| f.rule_id == "QA-HARDEN-001"));
    assert!(o.findings.iter().any(|f| f.rule_id == "QA-SNAP-003"));
    fs::remove_dir_all(root).expect("remove phase 16-19 fixture workspace");
}
