use qa_fault_runtime::{FaultKind, FaultSchedule};
#[test]
fn schedule_fails_exactly_at_requested_point() {
    let s = FaultSchedule::new(7, 2, FaultKind::Io);
    assert!(!s.should_fail(FaultKind::Io));
    assert!(!s.should_fail(FaultKind::Io));
    assert!(s.should_fail(FaultKind::Io));
    assert!(!s.should_fail(FaultKind::Io));
    s.reset();
    assert!(!s.should_fail(FaultKind::Io));
}
#[test]
fn different_fault_kind_does_not_trigger() {
    let s = FaultSchedule::new(1, 0, FaultKind::Allocation);
    assert!(!s.should_fail(FaultKind::Io));
}
