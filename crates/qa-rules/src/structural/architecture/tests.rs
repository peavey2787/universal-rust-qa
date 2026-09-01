use super::*;
use crate::test_support::{cleanup, discover};
use qa_policy::ArchitectureLayer;

#[test]
fn forbidden_layer_dependency_is_reported_but_allowed_dependency_is_not() {
    let (root, source) =
        discover(&[("src/domain/lib.rs", "use crate::infra::db; fn run(){ db(); }\n")]);
    let mut config = QaConfig::default();
    config.architecture.layer = vec![
        ArchitectureLayer {
            name: "domain".into(),
            paths: vec!["src/domain".into()],
            may_depend_on: vec![],
        },
        ArchitectureLayer {
            name: "infra".into(),
            paths: vec!["infra".into()],
            may_depend_on: vec![],
        },
    ];
    let mut findings = Vec::new();
    analyze(&source, &config, &mut findings);
    assert!(findings.iter().any(|f| f.rule_id == "QA-ARCH-001"));

    config.architecture.layer[0].may_depend_on.push("infra".into());
    findings.clear();
    analyze(&source, &config, &mut findings);
    assert!(findings.is_empty());
    cleanup(&root);
}

#[test]
fn configured_paths_match_windows_and_unix_separators() {
    assert!(path_matches(std::path::Path::new(r"C:\repo\src\domain\lib.rs"), "src/domain"));
    assert!(path_matches(std::path::Path::new("/repo/src/domain/lib.rs"), r"src\domain"));
}

#[test]
fn configured_path_matching_rejects_unrelated_prefixes() {
    assert!(!path_matches(std::path::Path::new("src/domainish/lib.rs"), "src/domain/"));
    assert!(!path_matches(std::path::Path::new("src/ui/view.rs"), "src/domain/"));
    assert!(!path_matches(std::path::Path::new("tests/domain.rs"), "src/domain/"));
}
