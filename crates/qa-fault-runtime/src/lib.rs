use std::{
    env,
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaultKind {
    Io,
    Allocation,
    Latency,
    Clock,
    PartialIo,
}
impl FaultKind {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "io" => Some(Self::Io),
            "allocation" => Some(Self::Allocation),
            "latency" => Some(Self::Latency),
            "clock" => Some(Self::Clock),
            "partial_io" | "partial-io" => Some(Self::PartialIo),
            _ => None,
        }
    }
}

pub struct FaultSchedule {
    seed: u64,
    counter: AtomicUsize,
    fail_at: usize,
    kind: FaultKind,
}
impl FaultSchedule {
    pub const fn new(seed: u64, fail_at: usize, kind: FaultKind) -> Self {
        Self { seed, counter: AtomicUsize::new(0), fail_at, kind }
    }
    pub fn from_env() -> Option<Self> {
        let seed = env::var("QA_FAULT_SEED").ok()?.parse().ok()?;
        let fail_at = env::var("QA_FAULT_AT").ok()?.parse().ok()?;
        let kind = FaultKind::parse(&env::var("QA_FAULT_KIND").ok()?)?;
        Some(Self::new(seed, fail_at, kind))
    }
    pub fn should_fail(&self, kind: FaultKind) -> bool {
        let n = self.counter.fetch_add(1, Ordering::SeqCst);
        kind == self.kind && n == self.fail_at
    }
    pub fn seed(&self) -> u64 {
        self.seed
    }
    pub fn fail_at(&self) -> usize {
        self.fail_at
    }
    pub fn kind(&self) -> FaultKind {
        self.kind
    }
    pub fn reset(&self) {
        self.counter.store(0, Ordering::SeqCst)
    }
    pub fn injected_latency(&self, base: Duration) -> Duration {
        if self.should_fail(FaultKind::Latency) { base.saturating_mul(16) } else { base }
    }
    pub fn injected_wall_clock_offset_ms(&self) -> i64 {
        if self.should_fail(FaultKind::Clock) { -60_000 } else { 0 }
    }
    pub fn partial_len(&self, requested: usize) -> usize {
        if requested > 1 && self.should_fail(FaultKind::PartialIo) {
            requested / 2
        } else {
            requested
        }
    }
}
