use super::*;
use crate::test_support::{cleanup, discover, ids};

#[test]
fn hardware_rules_cover_mmio_interrupt_and_dma_contracts() {
    let (root, source) = discover(&[(
        "src/lib.rs",
        r#"
#[qa_attr::mmio]
unsafe fn reg(){ let p=0x60000000usize as *mut u32; *p=1; }
#[qa_attr::interrupt]
fn irq(){ let x=[0u8;4096]; let _=x; let _=Vec::<u8>::new(); std::thread::sleep(std::time::Duration::from_millis(1)); panic!("bad"); }
#[qa_attr::dma_buffer]
fn dma(buf: &mut [u8]) { let _=buf; }
"#,
    )]);
    let mut config = QaConfig::default();
    config.hardware.enabled = true;
    config.hardware.interrupt_stack_budget_bytes = 1024;
    let mut findings = Vec::new();
    analyze(&source, &config, &mut findings);
    let found = ids(&findings);
    assert!(found.contains(&"QA-HW-001"));
    assert!(found.contains(&"QA-HW-002"));
    assert!(found.iter().filter(|id| **id == "QA-HW-004").count() >= 3);
    assert!(found.contains(&"QA-HW-006"));
    cleanup(&root);
}

#[test]
fn volatile_mmio_small_interrupt_and_aligned_dma_are_accepted() {
    let (root, source) = discover(&[(
        "src/lib.rs",
        r#"
#[qa_attr::mmio]
unsafe fn reg(){ let p=0x60000000usize as *mut u32; core::ptr::write_volatile(p,1); }
#[qa_attr::interrupt]
fn irq(){ let x=[0u8;16]; let _=x; }
#[repr(align(32))]
#[qa_attr::dma_buffer]
fn dma(buf: &mut [u8]) { let _=buf; }
"#,
    )]);
    let mut config = QaConfig::default();
    config.hardware.enabled = true;
    let mut findings = Vec::new();
    analyze(&source, &config, &mut findings);
    let found = ids(&findings);
    assert!(!found.contains(&"QA-HW-001"));
    assert!(!found.contains(&"QA-HW-002"));
    assert!(!found.contains(&"QA-HW-006"));
    cleanup(&root);
}

#[test]
fn hardware_helpers_parse_addresses_volatile_access_and_stack_arrays() {
    assert!(looks_like_mmio("let p = 0x4000usize as *mut u32;"));
    assert!(!looks_like_mmio("let x = 4;"));
    assert!(!looks_like_mmio("let x = 0x4000usize;"));
    assert!(!looks_like_mmio("let p = value as *mut u32;"));
    assert!(raw_access_without_volatile("p as *mut u32"));
    assert!(!raw_access_without_volatile("write_volatile(p, 1)"));
    assert_eq!(array_length("u8; 4096]"), Some(4096));
    assert_eq!(array_length("u8]"), None);
    assert_eq!(estimated_stack_bytes("let a=[0u8; 32]; let b=[0u8; 64];"), 96);
}

#[test]
fn mmio_attribute_and_interrupt_classification_are_independently_observable() {
    let (root, source) = discover(&[(
        "src/lib.rs",
        r#"
#[qa_attr::mmio]
fn mapped(register: *mut u32) { unsafe { *register = 1; } }
fn ordinary() { let _ = Vec::<u8>::new(); std::thread::sleep(std::time::Duration::from_millis(1)); panic!("ordinary"); }
"#,
    )]);
    let mut config = QaConfig::default();
    config.hardware.enabled = true;
    let mut findings = Vec::new();
    analyze(&source, &config, &mut findings);
    assert!(findings.iter().any(|finding| finding.rule_id == "QA-HW-001"));
    assert!(!findings.iter().any(|finding| finding.rule_id == "QA-HW-002"));
    assert!(!findings.iter().any(|finding| finding.rule_id == "QA-HW-004"));
    cleanup(&root);
}
