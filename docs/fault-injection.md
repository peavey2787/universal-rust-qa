# Deterministic fault injection

Phase 13 uses `(seed, kind, fail_at)` schedules. `qa-fault-runtime` exposes deterministic I/O, allocation, partial-I/O, latency, and wall-clock injection primitives; `cargo qa fault` enumerates configured fail points through the target workspace's `qa-fault-injection` feature. Failing schedules are written to the run artifact root at `fault/failures.jsonl` (normally `qa-out/fault/failures.jsonl` in local mode) for exact replay.
