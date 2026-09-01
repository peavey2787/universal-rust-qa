use qa_model::EvidenceKind;
use qa_rules::rule_registry;

#[test]
fn rule_registry_is_unique_complete_and_preserves_evidence_classes() {
    let registry = rule_registry();
    let mut ids = registry.rules.iter().map(|rule| rule.id.as_str()).collect::<Vec<_>>();
    let original = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), original);
    assert_eq!(original, 123);

    let static_rule = registry.find("QA-SPRAWL-001").expect("static rule");
    assert_eq!(static_rule.name, "File line limit");
    assert_eq!(static_rule.family, "SPRAWL");
    assert_eq!(static_rule.description, static_rule.name);
    assert!(matches!(static_rule.evidence, EvidenceKind::Static));

    let compiler_rule = registry.find("QA-COV-001").expect("compiler rule");
    assert_eq!(compiler_rule.name, "Coverage threshold");
    assert_eq!(compiler_rule.family, "COV");
    assert!(matches!(compiler_rule.evidence, EvidenceKind::Compiler));

    let dynamic_rule = registry.find("QA-MUT-001").expect("dynamic rule");
    assert_eq!(dynamic_rule.name, "Mutation threshold");
    assert_eq!(dynamic_rule.family, "MUT");
    assert!(matches!(dynamic_rule.evidence, EvidenceKind::Dynamic));

    assert!(registry.find("QA-DUP-002").is_some());
    assert!(registry.find("QA-DEAD-001").is_some());
    assert!(registry.find("QA-NOT-A-RULE").is_none());
}
