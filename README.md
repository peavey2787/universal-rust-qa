# Universal Rust QA

A policy-driven Rust QA framework that correlates source analysis, compiler evidence, dynamic backends, and runtime testing into one report and terminal health dashboard.

**Implemented through Phase 20**: the complete 39-family non-formal QA contract, including hardware, performance/bloat, final binary hardening, release engineering, reproducibility, and framework self-hardening.

## Commands

```text
cargo qa
cargo qa --version
cargo qa coverage
cargo qa mutants
cargo qa fuzz
cargo qa concurrency
cargo qa constant-time
cargo qa sanitizers
cargo qa full
cargo qa differential
cargo qa fault
cargo qa mir
cargo qa platform
cargo qa hardware
cargo qa performance
cargo qa performance-baseline
cargo qa hardening
cargo qa release
cargo qa self-hardening
cargo qa doctor
cargo qa settings
cargo qa exceptions
cargo qa reports
cargo qa export-config FILE
cargo qa import-config FILE
```

Standard `cargo qa` runs generate fresh `cargo-llvm-cov` coverage automatically when `[coverage] mode = "auto"` (the default). For ordinary implicit-host/default-feature coverage, QA uses the same direct JSON contract that works manually before doing QA-specific workspace bookkeeping: ensure `cargo-llvm-cov` is installed, run `cargo llvm-cov --json --output-path ...` directly in the inspected workspace with the system Cargo command and cargo-llvm-cov's normal target-directory behavior, then parse and retain the resulting JSON. If that plain run produces no usable JSON, QA retries once with `--ignore-run-fail`. If a direct workspace command exits nonzero but still emits usable JSON for only part of the workspace, QA keeps that report immediately and isolates **only the unmeasured members** instead of either discarding the report or declaring every untouched member failed. Packages already represented in the direct JSON are never rerun by this tail recovery. The broader progressive raw-profile pipeline remains the fallback when direct JSON cannot establish usable evidence. The direct path never depends on deleting QA's fallback target directories or on package-source LOC enumeration, and valid LLVM JSON is retained as numeric **Partial** evidence even if later Cargo metadata or package-root attribution is incomplete. One bookkeeping failure, native-build failure, WASM/target incompatibility, failing test, or bad profile therefore cannot erase trustworthy coverage already emitted by LLVM. CRAP is calculated only for functions with measured line evidence; unmeasured files remain `coverage = None`. Repositories without a root `Cargo.toml` report Rust coverage **NotApplicable** rather than Failed. Other expensive/nightly backend families still require their focused command or `full`/`release`. Pass `--existing-coverage` (alias `--reuse-coverage`) when you deliberately want to reuse prior evidence, or set `[coverage] mode = "existing"` to make reuse persistent. `[coverage] mode = "off"` remains the explicit way to disable coverage.

## Using Universal Rust QA on another project

Install or update the Cargo subcommand from this repository. Development revisions keep semantic version `0.1.0`, so use `--force` when replacing an existing install and verify the revision before testing:

```text
cargo install --path crates/cargo-qa --force
cargo qa --version
```

Plain `cargo qa` intentionally preserves the original local workflow: run it from a Rust project and QA-owned reports/evidence remain in that project (`qa-out/`, `mutants.out/`, and the normal Cargo target directory). Non-interactive completion is the default. When attached to a real terminal, `cargo qa` still shows the live progress dashboard while work is running and then exits automatically; pass `--interactive` to keep the post-run dashboard/menu interactive.

For an isolated external-project run, point the CLI at the project instead of changing directories. The standard command generates fresh coverage automatically:

```text
cargo qa --project-dir C:\projects\foo
cargo qa full --project-dir C:\projects\foo
```

Fresh coverage is self-provisioning: if the `cargo-llvm-cov` Cargo subcommand is missing, QA installs it with stable Cargo (bootstrapping the stable rustup toolchain when necessary), then cargo-llvm-cov provisions a missing `llvm-tools-preview` component for the inspected Rust toolchain non-interactively.

To reuse coverage from a previous run instead of invoking `cargo llvm-cov`, add the explicit reuse flag:

```text
cargo qa --project-dir C:\projects\foo --existing-coverage
cargo qa full --project-dir C:\projects\foo --reuse-coverage
```

In external-project mode coverage evidence lives under `<state>/coverage/`. `llvm-cov.json` contains the accepted line evidence and `coverage-failures.json` records the machine-readable coverage plan: package, target, feature configuration, command, exit code, failure stage/category, profile counts, and concise diagnostics for every attempt. Normal host/default-feature coverage writes `llvm-cov.json` from the direct `cargo llvm-cov --json --output-path ...` path. A parseable report is retained even when its child process exits nonzero. When that nonzero run covers only a subset of eligible workspace members, QA performs bounded tail recovery for the missing members only, merges any recovered line evidence into the canonical report, and counts a member as failed only after targeted recovery cannot obtain usable evidence. If direct JSON cannot establish usable scoped line evidence at all, QA enters the broader progressive fallback: compatible package grouping, strict/tolerant shared LLVM profile reporting, workspace direct recovery, and finally package-level isolation. The failed workspace attempt remains in the manifest and keeps recovered evidence **Partial** instead of discarding genuine line coverage. Relative filenames in cargo-llvm-cov JSON are anchored to its absolute manifest path before package filtering, and nested workspace roots are filtered by most-specific ownership so a parent package cannot absorb an incompatible child. Rescue raw profiles remain under `llvm-cov-rescue/` for diagnostics. Reuse restores the JSON plus scope manifest. If only an old/manual `llvm-cov.json` exists, its measured function coverage remains usable but is marked **Partial** because package/source scope cannot be proven. If no trustworthy JSON can be produced, coverage stays unavailable; Universal Rust QA never fabricates a numeric value.

Non-coverage Cargo-backed checks use QA's inspected-workspace toolchain resolver. Coverage deliberately invokes the system `cargo` command directly from the inspected workspace for metadata, tool probing, installation, collection, and reporting so its behavior matches a manual `cargo llvm-cov` run; on rustup installations, the Cargo shim still honors that workspace's `rust-toolchain.toml`, `rust-toolchain`, and directory overrides automatically. Explicit Cargo `+toolchain` jobs such as MSRV checks remain explicit.

Coverage planning is configurable in `qa.toml`:

```toml
[coverage]
mode = "auto"
all_features = false
include_packages = []
exclude_packages = []
features = []
no_default_features = false
targets = []
adaptive = true
```

`all_features = true` adds an additional package-scoped all-features configuration after default coverage; it does **not** replace the default build or allow a failed all-features attempt to masquerade as complete coverage. `include_packages` and `exclude_packages` control the eligible Cargo workspace set, `features`/`no_default_features` add an explicit feature configuration, `targets` selects explicit Rust target triples instead of implicit-host coverage, and `adaptive` controls compatible-group to per-package fallback. Set only `mode = "existing"` when persistent evidence reuse is desired.

`--project` is an alias for `--project-dir`. External-project mode keeps QA transient state and Cargo build artifacts outside the inspected repository. With no explicit output/state paths it uses `UNIVERSAL_QA_STATE_HOME` when set, otherwise the platform state directory and a stable per-project hash:

```text
<state-home>/projects/<project-hash>/
    reports/
    coverage/
    mutations/
        mutants.out/
    differential/
    fault/
    mir/
    repro/
    build/target/
```

On Windows the default state home is `%LOCALAPPDATA%\UniversalRustQA`; on Linux it is `$XDG_STATE_HOME/universal-rust-qa` or `~/.local/state/universal-rust-qa`; on macOS it is `~/Library/Application Support/UniversalRustQA`.

Custom routing is also first-class:

```text
cargo qa --project-dir C:\projects\foo --output-dir C:\qa\foo-reports
cargo qa --project-dir C:\projects\foo --state-dir C:\qa\foo-state
cargo qa --project-dir C:\projects\foo --output-dir C:\qa\reports --state-dir C:\qa\state
```

If `--output-dir` is supplied without `--state-dir`, transient state is placed in `<output-dir>/state`. If `--state-dir` is supplied without `--output-dir`, reports are placed in `<state-dir>/reports`. Explicit `--output-dir` and `--state-dir` take precedence over `UNIVERSAL_QA_STATE_HOME`.

## One-click full test + self-hardening

Linux: `./run-all-tests.sh`

Windows: double-click `run-all-tests.cmd`.

Both bootstrap Rust when needed, install the configured QA extensions by default, run format/check/Clippy/tests/doctests, then execute `cargo qa self-hardening`. Full transcripts are written under `qa-out/self-hardening/`. Set `QA_SKIP_TOOL_INSTALL=1` only if the external QA tools are already installed.

QA commands auto-exit by default, but a real terminal still receives the live dashboard while the run is active. Pass `--interactive` when you also want the post-run dashboard/menu to remain interactive. During live progress the normal health/LOC/CC/CRAP/test/coverage/mutation results stay visible while a progress bar, current category, elapsed time, and latest child-process status update underneath. Press `P` or Space to pause/resume the active external process tree, `S` to skip the current external test/check, or `C` to skip the current backend category. If a purely in-process Rust phase is active, pause is queued and takes effect at the next controllable category boundary rather than unsafely suspending an arbitrary Rust thread. Skipped work remains fail-closed and is reported as incomplete evidence; it never converts an incomplete run into a pass.

## Reports

In local mode, `qa-out/summary.txt` mirrors the terminal summary. In external mode the same report set is written to the resolved `reports/` directory or explicit `--output-dir`. JSON reports include the full report plus structural/test evidence and dedicated state, async, concurrency, errors, secrets, constant-time, sanitizer, differential, fault, MIR, platform, build, layout, FFI, hardware, performance, bloat, hardening, snapshots, documentation, dependencies, API, generated-output, reproducibility, self-hardening, mutation, fuzz, duplicate, dead-code, evidence, and findings outputs.

Coverage reporting distinguishes complete, partial, failed, unavailable, disabled, and not-applicable evidence. Partial summaries include measured line coverage, covered/eligible package counts, covered/eligible source LOC, failed and not-applicable package counts, and retained raw profile count. Source filtering uses the most-specific workspace package root, so a failed nested crate cannot inherit a parent package's covered status. A failed final LLVM export can therefore say that tests produced profiles but report extraction failed, instead of collapsing all execution evidence to `N/A`. Partial coverage is still a strict blocking condition for the overall QA gate.

Strict mutation runs are deliberately fresh verification runs: Universal Rust QA removes the prior cargo-mutants evidence directory before starting a requested campaign, then ingests the new `outcomes.json`. Local mode uses `<project>/mutants.out/`; external mode uses `<state>/mutations/mutants.out/`. Completed cargo-mutants process output is also parsed as a fail-safe so final counts remain reportable if the machine-readable file cannot be read.
A finalized cargo-mutants campaign is also a shutdown boundary: after `outcomes.json` contains an `end_time` and complete internally consistent outcome counts, Windows descendant cleanup starts while the cargo-mutants parent is still addressable, then QA allows a short parent-exit grace period and retains a bounded process/pipe cleanup fallback instead of waiting indefinitely on inherited handles. The completion probe is throttled and reads only the JSON tail until the final marker appears, avoiding repeated multi-megabyte parses during long campaigns. Finalized disk evidence remains usable if cleanup itself reports an error; incomplete or inconsistent evidence remains fail-closed.

Dead-code source-graph analysis is conservative around Rust indirection: direct/qualified calls, method calls, function pointers, turbofish references, and function identifiers carried by macro invocation token streams count as live references. String literals inside macros are not treated as code, an unused `macro_rules!` definition alone does not make a function live, and trait-implementation methods are not classified as source-unreferenced because trait dispatch can invoke them without a direct function call.

## Phase 8 — State machines

Critical state machines are checked for wildcard/unhandled transitions, explicit invalid-transition rejection, terminal-state review, and async transition atomicity.

## Phase 9 — Async/concurrency

Checks cancellation contracts, blocking calls in async functions, detached tasks, lock/await hazards, panic-capable `Drop`, unsafe `Send`/`Sync` rationale, shared mutable statics, relaxed atomics in critical concurrency code, and optional Loom/model-test execution.

## Phase 10 — Error/security

Checks swallowed important results, lost error context and broken error chains, secret logging/formatting and zeroization contracts, source-level secret-dependent branch/index hazards, and an optional repository-defined timing/constant-time evidence command.

## Phase 11 — Native sanitizers

`cargo qa sanitizers` executes configured ASan/LSan/TSan/MSan campaigns on a pinned/available nightly toolchain and records per-sanitizer status instead of assuming unsupported targets pass.

## Phase 12 — Differential testing

Configured reference/candidate commands consume the same deterministically sorted corpus. Identical entry commands fail the basic oracle-independence check. Exact, trimmed, and canonical-JSON equivalence are supported, and divergences are persisted as replayable JSON evidence.

## Phase 13 — Fault injection

`qa-fault-runtime` provides deterministic `(seed, kind, fail_at)` scheduling for I/O, allocation, partial-I/O, latency, and clock faults. `cargo qa fault` enumerates configured fail points and persists failing schedules for exact replay.

## Phase 14 — MIR analysis

`cargo qa mir` emits MIR per package with a pinned nightly, persists the aggregate IR, and correlates no-panic, no-allocation, drop-cleanup, zeroization, and async-retention signals back to annotated source functions.

## Phase 15 — Platform/build/layout/FFI

Checks default/no-default/all-feature compilation, optional each-feature and configured target matrices, declared MSRV compilation, build-script/proc-macro hermeticity signals, explicit critical layouts and raw-byte hazards, and FFI ABI/safety/panic contracts.

## Phases 16–20

- **16 Hardware:** MMIO, ISR stack/operation safety, DMA contracts, target/linker evidence.
- **17 Performance/Bloat:** false-sharing heuristics, explicit vectorization contracts, instruction baselines, cargo-bloat and LLVM-lines evidence.
- **18 Binary hardening:** release overflow checks and final ELF/PE/Mach-O mitigation/path-disclosure inspection.
- **19 Release engineering:** snapshots, doctests/examples, dependency audits, API/SemVer, generated-output determinism, reproducible builds.
- **20 Self-hardening:** registry/schema/source-sprawl/launcher/Git integrity plus the full cross-platform test harness.

Formal verification systems such as Kani, Verus, and Creusot remain intentionally outside the base `cargo qa` contract.

## License

Universal Rust QA is licensed solely under the GNU General Public License version 2.0 (`GPL-2.0-only`). See `LICENSE`. Third-party dependencies retain their own licenses; the license allowlist in `deny.toml` is dependency policy and does not add another license to this project.
