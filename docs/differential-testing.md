# Differential testing

Phase 12 executes reference and candidate commands over the same sorted corpus. Supported equivalence policies are `exact`, `trimmed`, and `canonical-json`. Identical entry commands fail the basic oracle-independence check. Every divergence is persisted under the run artifact root at `differential/<target>/` (normally `qa-out/differential/<target>/` in local mode) with the input and both outcomes.
