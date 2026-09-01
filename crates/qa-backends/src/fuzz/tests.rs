use super::*;

#[test]
fn pending_targets_are_unknown_and_empty_runs_are_empty() {
    let config = QaConfig::default();
    let names = vec!["parser".to_string(), "decoder".to_string()];
    let pending = check(Path::new("."), &config, &names, false);
    assert_eq!(pending.targets.len(), 2);
    assert_eq!(pending.targets["parser"], EvidenceStatus::Unknown);
    assert!(pending.errors.is_empty());
    let empty = check(Path::new("."), &config, &[], false);
    assert!(empty.targets.is_empty());
    assert!(empty.errors.is_empty());
}

#[test]
fn builder_results_map_to_available_failed_and_unavailable_exactly() {
    let names = vec!["ok".to_string(), "bad".to_string(), "missing".to_string()];
    let result = check_with(&names, true, |name| match name {
        "ok" => Ok(true),
        "bad" => Ok(false),
        "missing" => Err("tool missing".into()),
        _ => unreachable!(),
    });
    assert_eq!(result.targets["ok"], EvidenceStatus::Available);
    assert_eq!(result.targets["bad"], EvidenceStatus::Failed);
    assert_eq!(result.targets["missing"], EvidenceStatus::Unavailable);
    assert_eq!(result.errors.get("missing").map(String::as_str), Some("tool missing"));
    assert_eq!(result.errors.len(), 1);
}
