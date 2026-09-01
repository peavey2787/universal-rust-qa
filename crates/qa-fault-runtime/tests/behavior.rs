use qa_fault_runtime::{FaultKind, FaultSchedule};
use std::time::Duration;

#[test]
fn fault_kind_parser_accepts_all_supported_spellings_and_rejects_unknown() {
    assert_eq!(FaultKind::parse("io"), Some(FaultKind::Io));
    assert_eq!(FaultKind::parse("allocation"), Some(FaultKind::Allocation));
    assert_eq!(FaultKind::parse("latency"), Some(FaultKind::Latency));
    assert_eq!(FaultKind::parse("clock"), Some(FaultKind::Clock));
    assert_eq!(FaultKind::parse("partial_io"), Some(FaultKind::PartialIo));
    assert_eq!(FaultKind::parse("partial-io"), Some(FaultKind::PartialIo));
    assert_eq!(FaultKind::parse("unknown"), None);
}

#[test]
fn schedule_accessors_reset_and_kind_matching_are_exact() {
    let schedule = FaultSchedule::new(99, 1, FaultKind::Io);
    assert_eq!(schedule.seed(), 99);
    assert_eq!(schedule.fail_at(), 1);
    assert_eq!(schedule.kind(), FaultKind::Io);
    assert!(!schedule.should_fail(FaultKind::Allocation));
    assert!(schedule.should_fail(FaultKind::Io));
    schedule.reset();
    assert!(!schedule.should_fail(FaultKind::Io));
    assert!(schedule.should_fail(FaultKind::Io));
}

#[test]
fn injected_latency_clock_and_partial_io_apply_only_at_scheduled_point() {
    let latency = FaultSchedule::new(1, 0, FaultKind::Latency);
    assert_eq!(latency.injected_latency(Duration::from_millis(5)), Duration::from_millis(80));
    assert_eq!(latency.injected_latency(Duration::from_millis(5)), Duration::from_millis(5));

    let clock = FaultSchedule::new(1, 0, FaultKind::Clock);
    assert_eq!(clock.injected_wall_clock_offset_ms(), -60_000);
    assert_eq!(clock.injected_wall_clock_offset_ms(), 0);

    let partial = FaultSchedule::new(1, 0, FaultKind::PartialIo);
    assert_eq!(partial.partial_len(8), 4);
    assert_eq!(partial.partial_len(8), 8);
    let one = FaultSchedule::new(1, 0, FaultKind::PartialIo);
    assert_eq!(one.partial_len(1), 1);
}

#[test]
fn schedule_from_env_accepts_valid_triplet_and_rejects_missing_invalid_values() {
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = ENV_LOCK.lock().unwrap();
    unsafe {
        std::env::set_var("QA_FAULT_SEED", "7");
        std::env::set_var("QA_FAULT_AT", "3");
        std::env::set_var("QA_FAULT_KIND", "allocation");
    }
    let schedule = FaultSchedule::from_env().unwrap();
    assert_eq!(schedule.seed(), 7);
    assert_eq!(schedule.fail_at(), 3);
    assert_eq!(schedule.kind(), FaultKind::Allocation);

    unsafe { std::env::set_var("QA_FAULT_SEED", "invalid") };
    assert!(FaultSchedule::from_env().is_none());
    unsafe {
        std::env::remove_var("QA_FAULT_SEED");
        std::env::remove_var("QA_FAULT_AT");
        std::env::remove_var("QA_FAULT_KIND");
    }
    assert!(FaultSchedule::from_env().is_none());
}
