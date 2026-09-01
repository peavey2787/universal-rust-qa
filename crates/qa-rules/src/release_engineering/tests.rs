use super::*;
use crate::test_support::{cleanup, discover, ids, workspace};

#[test]
fn snapshot_dependency_documentation_and_api_contracts_are_enforced() {
    let root = workspace(&[
        (
            "src/lib.rs",
            r#"
pub unsafe fn unsafe_api() {}
#[qa_attr::critical]
pub fn critical_api() -> Result<(), ()> { Ok(()) }
pub fn leak() -> crate::internal::Thing { todo!() }
"#,
        ),
        (
            "Cargo.toml",
            "[package]\nname='fixture'\nversion='0.1.0'\n[dependencies]\nwild='*'\ngitdep={git='https://example.invalid/repo'}\n",
        ),
        ("pending.snap.new", "pending"),
        ("secret.snap", "private_key = 'redacted'"),
        ("ci.yml", "run: cargo insta accept"),
    ]);
    let source = qa_syntax::discover(&root);
    let mut config = QaConfig::default();
    config.dependencies.deny_git_dependencies = true;
    config.api.public_missing_docs = "deny".into();
    let mut findings = Vec::new();
    analyze(&source, &config, &mut findings);
    let found = ids(&findings);
    for expected in [
        "QA-SNAP-003",
        "QA-SNAP-005",
        "QA-SNAP-001",
        "QA-DEP-004",
        "QA-DEP-003",
        "QA-DOC-001",
        "QA-DOC-002",
        "QA-API-005",
        "QA-API-006",
        "QA-API-003",
    ] {
        assert!(found.contains(&expected), "missing {expected}: {found:?}");
    }
    cleanup(&root);
}

#[test]
fn documented_snapshot_and_dependency_allow_paths_do_not_emit_strict_findings() {
    let (root, source) = discover(&[
        (
            "src/lib.rs",
            r#"
/// Docs
/// # Safety
/// safe because fixture
pub unsafe fn unsafe_api() {}
/// Docs
/// # Examples
/// ```
/// assert_eq!(2, 1 + 1);
/// ```
#[qa_attr::critical]
#[must_use]
pub fn critical_api() -> Result<(), ()> { Ok(()) }
"#,
        ),
        ("Cargo.toml", "[package]\nname='fixture'\nversion='0.1.0'\n[dependencies]\nserde='1'\n"),
        ("approved.snap", "ordinary output"),
    ]);
    let mut findings = Vec::new();
    analyze(&source, &QaConfig::default(), &mut findings);
    let found = ids(&findings);
    assert!(!found.contains(&"QA-SNAP-003"));
    assert!(!found.contains(&"QA-SNAP-005"));
    assert!(!found.contains(&"QA-DEP-004"));
    assert!(!found.contains(&"QA-API-005"));
    assert!(!found.contains(&"QA-API-006"));
    cleanup(&root);
}

#[test]
fn release_helpers_cover_automation_manifest_dependency_and_exclusion_logic() {
    assert!(automation_file("ci.yml"));
    assert!(automation_file("check.ps1"));
    assert!(!automation_file("README.md"));
    assert!(cargo_manifest(true, Path::new("Cargo.toml")));
    assert!(!cargo_manifest(false, Path::new("Cargo.toml")));
    assert!(!cargo_manifest(true, Path::new("Other.toml")));
    assert!(excluded(Path::new("target/generated.txt")));
    assert!(!excluded(Path::new("src/lib.rs")));

    let mut findings = Vec::new();
    let mut config = QaConfig::default();
    check_dependency(
        Path::new("Cargo.toml"),
        "x",
        &toml::Value::String("*".into()),
        &config,
        &mut findings,
    );
    assert_eq!(findings[0].rule_id, "QA-DEP-004");
    findings.clear();
    config.dependencies.deny_wildcards = false;
    check_dependency(
        Path::new("Cargo.toml"),
        "x",
        &toml::Value::String("*".into()),
        &config,
        &mut findings,
    );
    assert!(findings.is_empty());
}

#[test]
fn release_filters_require_the_exact_conjunctions_and_disjunctions() {
    let root = workspace(&[
        ("src/lib.rs", "pub fn ordinary() {}\n"),
        ("target/pending.snap.new", "private_key = 'ignored'"),
        (
            "target/Cargo.toml",
            "[package]\nname='ignored'\nversion='0.1.0'\n[dependencies]\nx='*'\n",
        ),
        ("notes.txt", "cargo insta accept\nprivate_key = 'not-a-snapshot'\n"),
    ]);
    let source = qa_syntax::discover(&root);
    let mut findings = Vec::new();
    analyze(&source, &QaConfig::default(), &mut findings);
    let found = ids(&findings);
    assert!(!found.contains(&"QA-SNAP-003"));
    assert!(!found.contains(&"QA-SNAP-005"));
    assert!(!found.contains(&"QA-SNAP-001"));
    assert!(!found.contains(&"QA-DEP-004"));
    cleanup(&root);
}

#[test]
fn git_dependency_requires_both_policy_and_git_metadata() {
    let path = Path::new("Cargo.toml");
    let git = toml::from_str::<toml::Value>("git='https://example.invalid/repo'").unwrap();
    let table = git.as_table().unwrap();
    let dependency = toml::Value::Table(table.clone());
    let mut config = QaConfig::default();
    let mut findings = Vec::new();
    check_dependency(path, "gitdep", &dependency, &config, &mut findings);
    assert!(!ids(&findings).contains(&"QA-DEP-003"));

    config.dependencies.deny_git_dependencies = true;
    check_dependency(path, "gitdep", &dependency, &config, &mut findings);
    assert_eq!(findings.iter().filter(|finding| finding.rule_id == "QA-DEP-003").count(), 1);

    findings.clear();
    check_dependency(path, "plain", &toml::Value::String("1".into()), &config, &mut findings);
    assert!(!ids(&findings).contains(&"QA-DEP-003"));
}

#[test]
fn internal_type_leak_requires_both_public_surface_and_internal_path() {
    let (root, source) = discover(&[(
        "src/lib.rs",
        r#"
pub fn public_plain() -> u32 { 1 }
fn private_internal() -> crate::internal::Thing { todo!() }
pub fn public_internal() -> crate::internal::Thing { todo!() }
"#,
    )]);
    let mut findings = Vec::new();
    for name in ["public_plain", "private_internal"] {
        let function = source.functions.iter().find(|function| function.name == name).unwrap();
        check_internal_type_leak(function, &mut findings);
    }
    assert!(findings.is_empty());
    let leaking =
        source.functions.iter().find(|function| function.name == "public_internal").unwrap();
    check_internal_type_leak(leaking, &mut findings);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].rule_id, "QA-API-003");
    cleanup(&root);
}

#[test]
fn critical_example_requirement_is_scoped_to_public_critical_apis() {
    let (root, source) = discover(&[(
        "src/lib.rs",
        r#"
/// ordinary docs
pub fn public_ordinary() {}
/// critical docs
#[qa_attr::critical]
pub fn critical_missing_example() {}
/// critical docs
/// # Examples
#[qa_attr::critical]
pub fn critical_examples_heading() {}
/// critical docs
/// ```
/// critical_fenced_code();
/// ```
#[qa_attr::critical]
pub fn critical_fenced_code() {}
#[qa_attr::critical]
fn private_critical() {}
"#,
    )]);
    let mut findings = Vec::new();
    docs(&source, &QaConfig::default(), &mut findings);
    let examples =
        findings.iter().filter(|finding| finding.rule_id == "QA-DOC-002").collect::<Vec<_>>();
    assert_eq!(examples.len(), 1);
    assert!(examples[0].message.contains("critical_missing_example"));
    cleanup(&root);
}

#[test]
fn public_docs_finding_requires_both_missing_docs_and_non_allow_policy() {
    let root = workspace(&[(
        "src/lib.rs",
        "/// documented\npub fn documented() {}\npub fn undocumented() {}\n",
    )]);
    let source = qa_syntax::discover(&root);
    let mut config = QaConfig::default();
    let mut findings = Vec::new();
    docs(&source, &config, &mut findings);
    let missing =
        findings.iter().filter(|finding| finding.rule_id == "QA-DOC-001").collect::<Vec<_>>();
    assert_eq!(missing.len(), 1);
    assert!(missing[0].message.contains("undocumented"));

    config.api.public_missing_docs = "allow".into();
    findings.clear();
    docs(&source, &config, &mut findings);
    assert!(!findings.iter().any(|finding| finding.rule_id == "QA-DOC-001"));
    cleanup(&root);
}
