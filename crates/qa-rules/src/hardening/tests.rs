use super::*;
use crate::test_support::{cleanup, discover};

#[test]
fn release_profile_requires_explicit_overflow_checks() {
    let (root, source) = discover(&[(
        "Cargo.toml",
        "[package]\nname='x'\nversion='0.1.0'\n[profile.release]\noverflow-checks=false\n",
    )]);
    let mut findings = Vec::new();
    analyze(&source, &QaConfig::default(), &mut findings);
    assert_eq!(findings[0].rule_id, "QA-HARDEN-001");
    assert_eq!(findings[0].severity, Severity::High);
    cleanup(&root);
}

#[test]
fn missing_release_profile_is_medium_and_explicit_true_is_clean() {
    let (root, source) = discover(&[("Cargo.toml", "[package]\nname='x'\nversion='0.1.0'\n")]);
    let mut findings = Vec::new();
    analyze(&source, &QaConfig::default(), &mut findings);
    assert_eq!(findings[0].severity, Severity::Medium);
    cleanup(&root);

    let (root, source) = discover(&[(
        "Cargo.toml",
        "[package]\nname='x'\nversion='0.1.0'\n[profile.release]\noverflow-checks=true\n",
    )]);
    findings.clear();
    analyze(&source, &QaConfig::default(), &mut findings);
    assert!(findings.is_empty());
    cleanup(&root);
}

#[test]
fn disabled_hardening_skips_manifest_checks() {
    let (root, source) = discover(&[("Cargo.toml", "[profile.release]\noverflow-checks=false\n")]);
    let mut config = QaConfig::default();
    config.hardening.enabled = false;
    let mut findings = Vec::new();
    analyze(&source, &config, &mut findings);
    assert!(findings.is_empty());
    cleanup(&root);
}
