# MIR analysis

Phase 14 emits MIR per package using a pinned nightly `cargo rustc -- -Zunpretty=mir`. The backend correlates MIR sections with source annotations to find no-panic assert edges, no-allocation violations, expensive hot-path drop cleanup, zeroization disappearance signals, and critical async state-retention risks. MIR is compiler-version-coupled evidence and does not replace LLVM/final-code inspection.
