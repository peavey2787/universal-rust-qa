use super::*;

#[test]
fn disabled_and_pending_loom_states_are_reported() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut config = QaConfig::default();
    assert_eq!(run(root, &config, true).status, EvidenceStatus::Disabled);
    config.concurrency.loom_enabled = true;
    assert_eq!(run(root, &config, false).status, EvidenceStatus::Unknown);
}

#[test]
fn record_has_concurrency_identity() {
    let item = record(EvidenceStatus::Failed, "bad");
    assert_eq!(item.family, "CONC");
    assert_eq!(item.check, "loom/model tests");
}

#[test]
fn loom_status_maps_process_success_without_policy_ambiguity() {
    assert_eq!(loom_status(true), EvidenceStatus::Available);
    assert_eq!(loom_status(false), EvidenceStatus::Failed);
}
