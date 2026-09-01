use qa_model::*;

#[test]
fn severity_ordering_and_evidence_default_are_stable() {
    assert!(Severity::Critical > Severity::High);
    assert!(Severity::High > Severity::Medium);
    assert_eq!(EvidenceStatus::default(), EvidenceStatus::Unknown);
}

#[test]
fn registry_find_returns_exact_rule_only() {
    let registry = RuleRegistry {
        rules: vec![RuleDefinition {
            id: "QA-X-001".into(),
            name: "X".into(),
            family: "X".into(),
            evidence: EvidenceKind::Static,
            description: "desc".into(),
        }],
    };
    assert_eq!(registry.find("QA-X-001").unwrap().name, "X");
    assert!(registry.find("QA-X-002").is_none());
}

#[test]
fn representative_model_types_roundtrip_through_json() {
    let finding = Finding {
        rule_id: "QA-X-001".into(),
        severity: Severity::High,
        message: "message".into(),
        path: Some("src/lib.rs".into()),
        line: Some(7),
        detail: Some("detail".into()),
    };
    let encoded = serde_json::to_vec(&finding).unwrap();
    let decoded: Finding = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(decoded.rule_id, finding.rule_id);
    assert_eq!(decoded.severity, Severity::High);

    let registry = RuleRegistry::default();
    assert!(registry.rules.is_empty());
    let status: EvidenceStatus = serde_json::from_str("\"NotApplicable\"").unwrap();
    assert_eq!(status, EvidenceStatus::NotApplicable);
}
