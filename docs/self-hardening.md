# Self-Hardening — Phase 20

The framework analyzes itself and validates the complete 39-family rule registry, report/config schemas, source-file sprawl ceiling, top-level cross-platform launchers and Git cleanliness. The full launchers additionally run formatting, check, Clippy with warnings denied, tests, doctests, toolchain doctor, and `cargo qa self-hardening`.

## Linux

Double-click/execute `run-all-tests.sh`, or run:

```sh
./run-all-tests.sh
```

## Windows

Double-click `run-all-tests.cmd`. It invokes the PowerShell runner with transcript capture.

Both write transcripts to `qa-out/self-hardening/`. Set `QA_SKIP_TOOL_INSTALL=1` only when the required Cargo QA tools are already installed. The launchers collect formatting, check, Clippy, test, doctest, and doctor failures first; if any prerequisite is red they skip the multi-hour self-hardening/mutation campaign so an already-invalid source tree cannot waste hours on mutant execution.

### Mutation campaign scope

Self-hardening mutation runs explicitly mutate the complete workspace and run the complete workspace test suite with all features, so integration tests in downstream crates can detect mutants in shared lower-level crates. Strict campaigns remove prior mutation evidence before execution rather than accepting previous outcomes as current verification. Local mode reads cargo-mutants evidence from `mutants.out/`; isolated external mode reads it from `mutations/mutants.out/`. If a completed campaign cannot provide readable `outcomes.json`, the captured cargo-mutants summary and printed missed/time-out outcomes are parsed as fail-safe evidence instead of discarding the completed run. Source-backed unit-test modules are gated once at their parent `mod tests` declaration; cargo-mutants already excludes code under `cfg(test)`, so redundant file-level `#![cfg(test)]` attributes are intentionally avoided. Raw terminal read/write adapters and synthetic prompt-loop mutations may be narrowly excluded when mutation would only bypass I/O or force a non-terminating interactive loop; semantic parsing, action selection, rendering, and persistence behavior remain in scope.

