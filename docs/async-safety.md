# Async and concurrency QA

Phase 9 checks critical cancellation annotations, synchronous blocking APIs in async functions, detached tasks, likely blocking guards across `.await`, panic-capable `Drop`, unsafe `Send`/`Sync` rationale, shared `static mut`, and relaxed atomics in critical concurrency paths. MIR evidence in Phase 14 can refine source-level uncertainty.
