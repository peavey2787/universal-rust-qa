# Architecture

Phase 1 separates stable domain models, repository policy, Rust source discovery, rule execution, orchestration, reporting, SDK facade, proc-macro metadata, and terminal UX. Rule families remain modules in `qa-rules` to avoid micro-crate proliferation.
