use super::*;

#[test]
fn policy_and_execution_states_are_distinct() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut config = QaConfig::default();
    config.constant_time.enabled = false;
    assert_eq!(run(root, &config, true).status, EvidenceStatus::Disabled);

    config.constant_time.enabled = true;
    config.constant_time.command = None;
    assert_eq!(run(root, &config, true).status, EvidenceStatus::NotApplicable);

    config.constant_time.command = Some("exit 0".into());
    assert_eq!(run(root, &config, false).status, EvidenceStatus::Unknown);
    assert_eq!(run(root, &config, true).status, EvidenceStatus::Available);

    config.constant_time.command = Some("exit 7".into());
    assert_eq!(run(root, &config, true).status, EvidenceStatus::Failed);
}

#[test]
fn record_has_constant_time_identity() {
    let item = record(EvidenceStatus::Available, "ok");
    assert_eq!(item.family, "CT");
    assert_eq!(item.check, "timing harness");
    assert_eq!(item.detail.as_deref(), Some("ok"));
}
