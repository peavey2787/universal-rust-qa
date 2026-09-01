# State-machine QA

Phase 8 treats `#[qa_attr::critical_state]` and `#[qa_attr::state_machine]` as explicit contracts. The source analyzer checks invalid-transition handling, wildcard panic paths, critical state round-trip/restart test ownership, terminal variants, heuristic state reachability, and async mutation across cancellation boundaries. These checks are deterministic source evidence, not formal reachability proofs.
