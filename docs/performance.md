# Performance / Bloat QA — Phase 17

`QA-PERF-*` checks likely false sharing and annotated hot paths. `cargo qa performance` uses `cargo-asm` for instruction/vectorization evidence and compares instruction counts to an explicitly approved baseline. `QA-BLOAT-*` delegates binary and LLVM/codegen footprint evidence to `cargo-bloat` and `cargo-llvm-lines`.

Create/update the baseline only with `cargo qa performance-baseline`; normal runs never auto-approve drift.
