use qa_policy::QaConfig;

#[test]
fn defaults_are_strict_and_editor_is_vscode() {
    let cfg = QaConfig::default();
    assert_eq!(cfg.metrics.file_loc, 400);
    assert_eq!(cfg.metrics.cyclomatic, 12);
    assert_eq!(cfg.metrics.crap, 15.0);
    assert_eq!(cfg.metrics.coverage_percent, 90.0);
    assert_eq!(cfg.viewer.command, "code");
    assert!(cfg.exceptions.require_reason);
    assert!(cfg.exceptions.require_expiry);
}
