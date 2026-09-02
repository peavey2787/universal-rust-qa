## 2026-09-02 - r66 make direct coverage independent from QA bookkeeping

- Made ordinary automatic coverage genuinely direct-first. QA now verifies/installs `cargo-llvm-cov`, writes the plain manual JSON contract `cargo llvm-cov --json --output-path <fresh-report>` to a unique fresh report path, and parses that exact file before Cargo metadata parsing, package-source LOC enumeration, progressive target cleanup, package isolation, or raw-profile merging. If the plain run produces no usable JSON, QA retries once with `--ignore-run-fail` before entering the progressive fallback.
- Removed the last stale-output and fallback-state preconditions from the direct path. QA no longer deletes the prior canonical `llvm-cov.json`, failure manifest, progressive `llvm-cov-target`, or `llvm-cov-rescue` directories before the manual-style command. A valid fresh report is parsed first and only then copied to the canonical report path; even a locked old canonical report cannot erase the current numeric coverage. Progressive cleanup is deferred until direct JSON collection actually fails.
- Made trustworthy LLVM JSON stronger than QA's bookkeeping. Package/root attribution and eligible-source LOC are computed only after valid direct evidence exists; if that later metadata/scope work fails or cannot attribute files, the numeric LLVM percentage and raw line evidence remain `Partial` instead of collapsing to `N/A`. CRAP can still use any matching measured source paths.
- Kept the system `cargo` executable from `PATH`, cargo-llvm-cov-owned normal target lifecycle, automatic `llvm-tools-preview` setup, build-revision display, and first-screen failure diagnostics. Coverage failure manifests now retain the direct-attempt diagnostic even when later metadata also fails. Added regressions for the exact manual command, tolerant retry, fallback-directory independence, and unattributed valid JSON.
- No coverage/CRAP thresholds, package semantic versions, GPL-v2-only licensing, mutation policy, or Clippy `-D warnings` policy changed.

## 2026-09-02 - r65 make automatic coverage match manual Cargo invocation

- Removed the remaining execution-path mismatch between manual coverage and Universal Rust QA coverage. Coverage Cargo subprocesses—including `cargo metadata`, tool probing/install, collection, and reporting—now invoke the system `cargo` executable directly in the inspected workspace instead of going through QA's generic Cargo wrapper that first resolves a toolchain and re-launches Cargo via `rustup run`. The primary path is explicitly `cargo llvm-cov --ignore-run-fail --json --output-path <...>/llvm-cov.json`, matching the normal cargo-llvm-cov workflow while still setting `CARGO_LLVM_COV_SETUP=yes` for automatic `llvm-tools-preview` provisioning.
- Stopped overriding `CARGO_LLVM_COV_TARGET_DIR` and `CARGO_LLVM_COV_BUILD_DIR` on the normal direct-primary path. cargo-llvm-cov now owns its normal clean/instrumented target-directory lifecycle exactly as it does when run manually; QA controls only the JSON output path. The older isolated target/profile environment remains fallback-only for progressive recovery.
- Added an explicit build revision to the executable and dashboard (`r65`) plus `cargo qa --version`, so an external-project run can prove which development revision is actually installed even though package semantic versions intentionally remain pinned at `0.1.0`. Updated installation instructions to use `cargo install --path crates/cargo-qa --force` when replacing a prior development revision.
- Failed/unavailable coverage is no longer hidden behind a generic dashboard sentence. The final dashboard now prints the backend coverage diagnostic and, when present, the exact `coverage-failures.json` path so the subprocess failure is visible on the first run instead of requiring another speculative patch cycle.
- No coverage/CRAP thresholds, package semantic versions, GPL-v2-only licensing, mutation policy, or Clippy `-D warnings` policy changed.

## 2026-09-02 - r64 retain direct external-workspace coverage

- Fixed the remaining external-workspace `coverage N/A` regression in the r62/r63 direct-primary path. A valid `cargo llvm-cov` JSON report was previously discarded unless LLVM emitted attributable source-file records for every selected Cargo package. Real workspaces can legitimately contain proc-macro, helper, target-gated, generated, or otherwise non-executable members with no file record, so one absent package could throw away trustworthy coverage for the rest of the workspace and send collection into the older progressive fallback. The direct report is now authoritative whenever it contains usable scoped line evidence: complete scope remains Available, while incomplete scope keeps numeric line coverage and per-function CRAP as Partial instead of collapsing to N/A.
- Simplified the normal default-host primary command to the same plain manual contract users expect: `cargo llvm-cov --ignore-run-fail --json --output-path <qa-out>/llvm-cov.json`. Package-list `-p` synthesis is retained only for fallback/recovery paths that intentionally target a selected subset. This avoids making the ordinary self/external coverage path depend on a generated all-package command line.
- Direct finalization now derives covered/failed package names, roots, source LOC, and scope percentage from the package names actually represented by LLVM evidence. Missing nested package roots are excluded when recomputing measured coverage, so a covered parent cannot claim an unmeasured child. Partial diagnostics now say that a package had no usable line coverage evidence rather than incorrectly asserting that its baseline test command failed.
- Added regressions for the exact plain direct command and for preserving 50% numeric coverage/CRAP evidence when one of two selected packages has no LLVM source-file record. No strict thresholds, package semantic version, GPL-v2-only licensing, mutation policy, or Clippy `-D warnings` policy changed.

## 2026-09-02 - r63 r62 rustfmt repair

- Applied the exact stable Rust formatting reported by the Windows `cargo fmt --check` run to the `stable_install_command_matches_upstream_installation_contract` assertion in `coverage/tooling.rs`.
- No coverage behavior, direct-primary architecture, Kaspa workspace handling, automatic `cargo-llvm-cov` provisioning, package semantic version, strict threshold, GPL-v2-only licensing, mutation policy, or Clippy `-D warnings` policy changed.

## 2026-09-02 - r62 restore direct coverage as the primary path

- Restored the simple coverage architecture that worked before the r55 progressive rewrite: ordinary implicit-host/default-feature coverage now probes or installs `cargo-llvm-cov`, runs one isolated selected-workspace `cargo llvm-cov --json --output-path ... --ignore-run-fail` command first, parses that JSON directly, and returns immediately when every selected package is measured. The progressive raw-profile/merge/package-isolation machinery is fallback-only instead of being able to break normal self-hardening or external-project coverage before the direct command is tried.
- Kept r60's external-workspace correctness fixes on the primary path: relative cargo-llvm-cov filenames are anchored to `cargo_llvm_cov.manifest_path`, Windows verbatim paths are normalized, and nested package ownership uses the most-specific Cargo member root. This is specifically intended to make the normal direct command work for the attached Rusty Kaspa workspace rather than requiring a Kaspa-specific special case.
- Hardened automatic tooling provisioning. Any failed `cargo llvm-cov --version` probe now attempts repair/reinstallation instead of trusting a brittle error-string classifier. Installation first follows upstream's `cargo +stable install cargo-llvm-cov --locked` contract; if a stable toolchain cannot be used, the current workspace Cargo gets a final pinned `cargo-llvm-cov 0.6.21` fallback, whose documented install MSRV is Rust 1.81 and is therefore compatible with this repository's Rust 1.85 workspace pin. `CARGO_LLVM_COV_SETUP=yes` remains enabled so the inspected Rust toolchain can install its matching `llvm-tools-preview` component non-interactively.
- Added regressions that make direct workspace JSON the explicit default-host primary path, require every selected package before short-circuiting progressive fallback, reset the direct coverage target between runs, and preserve the upstream/pinned installation contracts. No coverage threshold, CRAP threshold, package semantic version, GPL-v2-only licensing, mutation policy, or Clippy `-D warnings` policy changed.

## 2026-09-02 - r61 r60 rustfmt repair

- Applied the exact stable Rust formatting reported by the Windows `cargo fmt --check` run across the r60 external-workspace coverage recovery changes.
- No coverage behavior, Kaspa workspace handling, automatic cargo-llvm-cov provisioning, package semantic version, strict threshold, GPL-v2-only licensing, mutation policy, or Clippy `-D warnings` policy changed.

## 2026-09-02 - r60 external-workspace coverage recovery

- Made fresh external-project coverage self-provisioning instead of assuming `cargo-llvm-cov` is already installed. Coverage now probes `cargo llvm-cov --version`, installs `cargo-llvm-cov` with stable Cargo when the subcommand is missing (bootstrapping the stable rustup toolchain when necessary), then continues to use `CARGO_LLVM_COV_SETUP=yes` so the inspected toolchain can provision its matching `llvm-tools-preview` component non-interactively. Real preflight errors still fail closed instead of being mislabeled as missing tooling.
- Added a clean workspace-level direct JSON recovery path that mirrors a normal manual `cargo llvm-cov --json --output-path ...` collection, explicitly scopes the command to every eligible package, and uses an isolated coverage target directory. This recovery is attempted when the shared raw-profile/report pipeline cannot produce usable JSON, before falling back to package-by-package recovery.
- Fixed a direct cause of false `coverage N/A`: cargo-llvm-cov JSON may identify source files with workspace-relative names such as `src/lib.rs` or `crate-a/src/lib.rs`, while Cargo metadata package roots are absolute. QA previously parsed those files and then discarded every one during package-scope filtering. Relative JSON filenames are now anchored to cargo-llvm-cov's absolute `cargo_llvm_cov.manifest_path`, with verbatim Windows paths and lexical `.`/`..` components normalized before scope and per-function matching.
- Fixed the r58/r59 recovery blind spots that could leave large external workspaces at `coverage N/A`: package rescue now runs for every still-unmeasured eligible package rather than only packages whose earlier baseline attempt succeeded, and a parseable/scoped direct JSON report is retained even when the cargo-llvm-cov process exits nonzero. The nonzero attempt remains recorded and degrades evidence to Partial; it no longer destroys real line evidence.
- Hardened nested-workspace attribution for large workspaces such as Rusty Kaspa (68 declared members in the inspected archive, with nested members under roots including `consensus`, `mining`, `database`, `utils`, `wasm`, and `crypto/txscript`). Direct recovery excludes other eligible/not-applicable package roots and attributes workspace JSON to the most-specific package root, preventing a parent member from claiming a failed or incompatible child. A failed strict shared-profile export now also explicitly degrades a tolerant/direct recovery result.
- Added regressions for manual-contract workspace JSON arguments, explicit multi-package selection, nested package ownership, incompatible multi-target workspace fallback, and missing-tool classification. No package semantic version, strict threshold, GPL-v2-only licensing, mutation policy, or Clippy `-D warnings` policy changed.

## 2026-09-02 - r59 progressive coverage build repair

- Applied stable Rust formatting to the r58 progressive coverage changes so `cargo fmt --check` passes on Windows and other supported hosts.
- Removed the obsolete `CoverageScope.failed_names` planning field and its stale test assertion. r58 finalization now derives failed packages from the packages that actually produced usable merged or direct-recovery evidence, so retaining the pre-recovery field was both unused and semantically superseded. The repair keeps Clippy/compile `-D warnings` strict rather than suppressing dead code.
- No coverage thresholds, recovery semantics, package semantic versions, licensing, mutation policy, or warning policy changed.

## 2026-09-02 - r58 isolated progressive coverage recovery

- Removed the remaining all-or-nothing failure point from progressive coverage finalization. The shared `--no-report`/`cargo llvm-cov report` path remains the fast path when raw profiles are healthy, but an empty, malformed, or unmergeable shared profile set now falls back to **isolated per-package direct JSON coverage runs**. Each rescue package receives its own cargo-llvm-cov target/profile directory, so one RocksDB/bindgen, WASM, platform-specific, malformed-profile, or other incompatible member cannot poison coverage evidence from unrelated members. Successful direct reports are source-filtered to their package roots, merged without double-counting line hits, and persisted as the canonical reusable `llvm-cov.json`.
- Direct report recovery uses cargo-llvm-cov's run-failure-tolerant reporting so coverage extraction can survive a failing test after instrumentation; the original failed test attempt remains in `coverage-failures.json` and keeps the resulting evidence Partial instead of pretending the test run passed. A complete package scope can return complete coverage only when no degrading collection/test condition remains; otherwise measured functions keep trustworthy coverage/CRAP while failed or unmeasured packages remain unknown.
- Non-Cargo repositories now report coverage **NotApplicable** instead of Failed. This makes repositories such as Bitcoin Core's C++/CMake tree explicit rather than attempting a Rust coverage backend that cannot apply. Cargo workspaces such as Kaspa continue through metadata planning and package isolation, retaining whatever compatible member coverage can be measured.
- Fixed the self-hardening regressions introduced by the r55 coverage backend: split coverage failure classification below the CC 12 threshold and replaced deliberately discarded manifest/process-cleanup results with explicit handling and diagnostics. Coverage manifest persistence failures now degrade evidence rather than disappearing silently.
- Added regressions for direct package JSON arguments/staging, retained test-failure coverage, direct-report line merging, non-Cargo NotApplicable classification, and usable Partial evidence. Split coverage execution tests into their own source file to keep all self-hosted Rust files below the 400 logical / 600 physical LOC gates. No strict threshold, package semantic version, GPL-v2-only licensing, mutation policy, or Clippy `-D warnings` policy changed.

## 2026-09-01 - r57 final progressive coverage rustfmt repair

- Applied the two remaining stable Rust 1.98 `rustfmt` changes reported by the r56 Windows run: removed redundant blank lines in coverage parse regressions and wrapped the progressive coverage runner import list at rustfmt's exact boundary.
- No runtime behavior, coverage semantics, strict thresholds, package semantic versions, GPL-v2-only licensing, or Clippy `-D warnings` policy changed. The r56 Windows run had already passed `cargo check`, the full workspace `cargo test` suite, doctests, and `cargo qa doctor`; only `cargo fmt --check` remained.

## 2026-09-01 - r56 progressive coverage Windows prerequisite repair

- Applied the exact stable Rust 1.98 `rustfmt` layout reported by the r55 Windows full-test run across the progressive coverage modules and dashboard.
- Removed the new coverage runner's Clippy `too_many_arguments` finding without suppressing warnings by introducing an `AttemptSpec` value that carries package, target, configuration, mode, and Cargo arguments into `run_attempt`. Existing callers now pass the explicit attempt specification while process/workspace/environment remain runner context.
- Rewrote Rust-source extension filtering with the equivalent `Option::is_none_or` predicate required by Clippy's `nonminimal_bool` lint. No coverage semantics, strict thresholds, package semantic versions, GPL-v2-only licensing, or Clippy `-D warnings` policy changed.
- The r55 Windows evidence had already passed `cargo check`, the full `cargo test` workspace suite, doctests, and `cargo qa doctor`; self-hardening was skipped only because prerequisite `cargo fmt --check` and Clippy failed.

## 2026-09-01 - r55 progressive evidence-preserving coverage

- Replaced the all-or-nothing workspace coverage command with a progressive plan: run the project's normal/default coverage first, enumerate Cargo workspace/default members, cover members not already exercised in compatible package groups, and adaptively isolate failures while retaining every raw LLVM profile from successful or partially executed tests. A failed project-default attempt goes directly to narrower per-package retries instead of rerunning an equivalent whole workspace. `--all-features` is no longer the default requirement; it is an additional opt-in package-scoped configuration.
- Added coverage policy for `include_packages`, `exclude_packages`, `features`, `no_default_features`, `targets`, and adaptive package fallback. Unknown explicitly included packages fail closed instead of silently shrinking the measured scope, while build-script-only members are reported not applicable.
- Split test execution from merged report extraction with cargo-llvm-cov no-report/report stages. Finalization first requires a strict profile merge; if malformed profiles prevent that, QA records the failed strict export and retries with cargo-llvm-cov's tolerant `--failure-mode all`, keeping the resulting evidence explicitly Partial instead of silently dropping bad profiles. Machine-readable `coverage-failures.json` records package, target, feature set, command, exit code, failure stage/category, profile counts, and diagnostics for every attempt. Native bindgen/RocksDB failures, unsupported targets/WASM, test failures, tooling failures, and generic instrumentation failures are classified separately.
- Added `Partial` evidence semantics and report schema 21. Partial coverage retains trustworthy per-function coverage/CRAP while unmeasured functions remain unknown, reports covered/eligible package and source-LOC scope, remains blocking at the strict gate, and never turns a narrower retry into a claim of whole-repository coverage. Covered and excluded package roots are persisted and filtering assigns nested files to the most-specific package root, so a failed child crate cannot be accidentally credited through a covered workspace-root package. Legacy partial manifests without trustworthy roots withhold per-function coverage. Existing JSON without a scope manifest is treated as Partial rather than silently assumed complete.
- Added regressions for one workspace member failing while another retains profiles, Cargo default-member accounting, default coverage surviving an optional all-features failure, strict-vs-tolerant report extraction, failed report extraction retaining raw-profile evidence, nested failed-package source filtering, native bindgen and WASM diagnostics, test-vs-build failure staging, malformed LLVM JSON, duplicate file records in merged output, and package/category diagnostics in the failure manifest. Source-scope enumeration now fails closed on unreadable Rust files instead of shrinking eligible LOC. Coverage commands explicitly clear QA's outer Cargo target override so cargo-llvm-cov keeps its isolated instrumented target directory. No package semantic versions, coverage threshold, CRAP threshold, mutation policy, or Clippy `-D warnings` policy changed.

## 2026-09-01 - r54 GPL-v2-only and interrupted-mutation survivor repair

- Made the repository license unambiguous GPL v2 only: workspace package metadata now uses `GPL-2.0-only`, the obsolete MIT and Apache project-license files were removed, and the README states that the `deny.toml` license allowlist applies only to third-party dependencies.
- Recovered actionable evidence from the power-interrupted r53 mutation campaign even though `outcomes.json` was corrupted: the durable text evidence had classified 1,421 of 2,481 enumerated mutants (1,206 caught, 210 unviable, 5 missed, 0 timed out) before the outage, leaving 1,060 unclassified. The campaign was incomplete, so these are the known survivors rather than a claim that all mutants ran.
- Eliminated all five known survivors without weakening mutation policy: MIR package aggregation is now directly testable without spawning nested Cargo, with aggregate/record/failure-rollup assertions that expose whole-function and `|=` mutations; mutation-watch completion probing now has a pure finalized/interval predicate with exact conjunction regressions; and finalized-process termination has a direct process-effect regression so a no-op cleanup cannot survive.
- No package semantic versions, strict thresholds, Clippy `-D warnings`, mutation timeout severity, or coverage policy changed.

## 2026-09-01 - r53 watchdog throttle regression repair

- Applied the Rust 1.98 `rustfmt` layout reported by the Windows self-hardening run.
- Removed the wall-clock-dependent mutation-watchdog probe-count test and replaced it with an exact one-second probe-interval boundary regression.
- Removed the extra completion-evidence probe after the watched parent has already exited; finalized campaigns still clean descendants immediately, while the bounded pipe cleanup remains the fallback for an exited parent whose final evidence was not observed by the periodic probe.
- No QA thresholds or package semantic versions changed.

## 2026-09-01 - r52 final self-hardening blocker and mutation-timeout repair

- Removed the remaining self-host CRAP blocker by splitting MIR package execution into small output-directory, package-emission, and finalization stages and adding direct ready/failure-path coverage; the CRAP threshold remains 15.
- Eliminated the r51 set of 11 mutation timeouts without excluding them: child stdin is now taken by the caller and passed by value to the writer so even a no-op writer closes EOF instead of stranding a child, lexical call scanners have explicit input-bound progress guards so constant-return helper mutants terminate, and calendar conversion uses saturating multiplication methods rather than mutation-prone `* -> /` operators.
- Hardened all five r51 missed-mutant sites: zero-mutant finalization is rejected explicitly, Cargo/non-Cargo command construction is centralized and directly tested, completion-grace boundary semantics have a pure regression, Windows descendant cleanup is tested by process effect rather than return value, and secret-index detection requires a real bracket pair.
- Improved finalized-mutation cleanup on Windows by terminating cargo-mutants descendants as soon as finalized evidence is observed while the parent process is still addressable, then retaining the bounded parent/pipe shutdown fallback. This addresses the r51 diagnostic where completed evidence was recovered but post-exit descendant enumeration could no longer release inherited handles.
- Corrected self-host dead-reference false positives: function identifiers inside macro invocation token streams now count as source references without treating string literals as code, `macro_rules!` definitions alone do not make a function live, and trait-implementation methods are excluded from closed source-call reachability because they can be invoked through trait dispatch.
- No package semantic versions, Clippy `-D warnings` policy, coverage threshold, mutation threshold, timeout severity, or 400 logical-LOC target were weakened.

## 2026-08-31 - r51 mutation finalization marker whitespace repair

- Fixed the finalized cargo-mutants tail prefilter so compact JSON (`"end_time":null`), pretty JSON (`"end_time": null`), arbitrary whitespace, and empty-string values are classified by token shape instead of one exact whitespace spelling.
- Kept the complete `outcomes.json` parse and count-consistency check authoritative before a campaign can be treated as finalized; this changes only the cheap 2 KiB prefilter and does not weaken fail-closed mutation evidence semantics.
- Added exact regressions for null, empty-string, and non-empty `end_time` values. No thresholds, package semantic versions, or Clippy warning policy changed.

## 2026-08-31 - r50 r49 Windows prerequisite repair

- Applied the exact stable Rust 1.98 `rustfmt` layouts reported by the r49 Windows prerequisite run.
- Restored the missing `std::time::Instant` import required by the new mutation-watchdog regression.
- Corrected the host-path source-line regression fixture so the discovered Rust source contains a literal Windows `C:\Users\...` path (via a Rust raw string) and therefore exercises `QA-ENV-002` instead of escaping the backslashes away from the detector.
- No watchdog semantics, mutation thresholds, Clippy `-D warnings` policy, coverage policy, or package semantic versions were weakened.

## 2026-08-31 - r49 finalized-mutation shutdown and survivor hardening

- Fixed a Windows post-campaign hang exposed by the completed r48 self-mutation run: once cargo-mutants has written a finalized `outcomes.json` with `end_time` and internally consistent caught/missed/timeout/unviable counts, Universal Rust QA gives the process a bounded 30-second shutdown grace, then terminates a lingering process tree instead of leaving the Mutation phase running indefinitely. Output-pipe shutdown is separately bounded so inherited handles cannot strand the dashboard forever; Windows cleanup falls back to descendant enumeration when the original cargo-mutants parent has already exited.
- Kept completion detection cheap during multi-hour campaigns: the watchdog probes at most once per second, checks only the final 2 KiB of `outcomes.json` for the `end_time` marker, and parses the complete JSON only after that marker appears.
- Preserve finalized on-disk mutation evidence even when post-campaign process cleanup reports an error; cleanup trouble is retained as diagnostic context rather than discarding a complete multi-hour campaign. This does not convert incomplete evidence into a pass: only a finalized, count-consistent outcomes file qualifies.
- Added direct stdin-lifecycle tests for `send_input`/`send_optional_input`, turning the two r48 timeout mutants into fast observable failures instead of allowing a no-op input helper to strand a child waiting for EOF.
- Hardened every source area represented by the r48 set of 35 missed mutants with exact tests or removal of semantically redundant logic, including MIR package failure propagation, inspected-workspace toolchain resolution, dynamic-evidence dispatch, date conversion, report family counts, source-line attribution, fuzz-target matching, false-sharing boundaries, FFI/docs/secret/state conjunctions, dead-reference visitors, cyclomatic `||`, test-workspace recursion, and lexical call termination. The Unix-only `signal_group` whole-function mutant is excluded only on Windows because that function is not compiled there and remains the Unix job's responsibility.
- The supplied r48 campaign itself completed all 2,433 mutants in about 8h02m (2,151 caught, 35 missed, 245 unviable, 2 timed out); the additional roughly 11.5 hours shown by the live dashboard occurred after cargo-mutants had finalized its evidence. No mutation thresholds, Clippy warning policy, or package semantic versions were weakened.

## 2026-08-30 - r48 non-interactive coverage tool provisioning

- Made fresh coverage set `CARGO_LLVM_COV_SETUP=yes`, allowing `cargo-llvm-cov` to install a missing `llvm-tools-preview` component for the inspected workspace toolchain without entering its interactive confirmation path; this prevents the live dashboard from appearing hung while cargo-llvm-cov waits on stdin.
- Added a regression that pins the isolated coverage environment, including its target/build directories and non-interactive setup policy.
- Applied the exact stable-rustfmt layout reported by the r47 Windows prerequisite gate in the workspace-toolchain parser regression.
- Kept strict Clippy `-D warnings` unchanged: it promotes warnings to hard errors rather than suppressing them. Package semantic versions remain unchanged.

## 2026-08-30 - r47 inspected-workspace Cargo toolchain isolation

- Fixed nested Cargo execution so coverage, mutation, fuzz, sanitizer, platform, release, reproducibility, performance, and other Cargo-backed QA work runs through the inspected workspace's rustup-selected toolchain instead of inheriting Universal Rust QA's own toolchain binary from the parent process PATH.
- Workspace toolchain resolution deliberately removes the parent `RUSTUP_TOOLCHAIN` while asking rustup for the inspected directory's active toolchain, so a repository `rust-toolchain.toml`/rustup directory override is honored; explicit Cargo `+toolchain` arguments remain authoritative and are routed through `rustup run`. Systems without rustup keep the direct-Cargo fallback.
- Added resolver regressions for rustup output, explicit `+toolchain` routing, and the exact `rustup run <workspace-toolchain> cargo ...` command/environment shape.
- Corrected the r46 SDK coverage-disable regression fixture to include the required top-level QA config fields; no production configuration requirements or strict thresholds were weakened. Package semantic versions remain unchanged.

## 2026-08-30 - r46 fresh coverage by default

- Changed standard engine/SDK/CLI run options so `[coverage] mode = "auto"` generates a fresh `cargo llvm-cov --json` result by default instead of merely consuming a pre-existing `llvm-cov.json`.
- Added global `--existing-coverage` and `--reuse-coverage` aliases for deliberately reusing the resolved coverage JSON without launching a new coverage command, plus persistent `[coverage] mode = "existing"`; missing reused evidence remains fail-closed/unavailable with an actionable diagnostic.
- Kept focused backend commands scoped: commands such as `mutants`, `fuzz`, `mir`, and `performance` do not unexpectedly start a fresh coverage campaign unless the caller chooses a standard/full/release run; the dedicated `coverage` command still forces fresh coverage.
- Added regression coverage for the fresh-default/existing-override policy and updated CLI/help/evidence documentation. Package semantic versions and strict coverage/CRAP thresholds remain unchanged.

## 2026-08-30 - r45 Windows prerequisite cleanup

- Repaired the remaining r44 test-module compile failures by restoring the explicit `std::fs`, `BTreeMap`, and coverage-finding imports required after the module splits.
- Rewrote the state detector regression fixtures with direct `WorkspaceSource` initialization so strict Clippy accepts them without suppressions.
- Made the shared rule-test workspace helper create empty workspace roots explicitly, eliminating the Windows cleanup failure in duplicate-analysis boundary tests that intentionally construct `WorkspaceSource` values by hand.
- Applied the exact rustfmt layout reported by the Windows r44 run; package semantic versions remain unchanged.

## 2026-08-30 - r44 r43 prerequisite and regression repair

- Repaired the r43 split-module compile regressions by keeping configuration default helpers test-visible without expanding the public API and routing nested reproducibility code to the crate-level artifact/process modules.
- Corrected dead-reference recursion suppression so genuinely qualified same-name calls remain live references while unqualified direct self-recursion stays excluded; the existing function-pointer and turbofish regressions remain intact.
- Tightened state round-trip evidence so the test function name itself cannot satisfy one of the required round-trip tokens; evidence now comes from the test body (or the existing property marker).
- Corrected mutation-hardening fixtures so hardware and constant-time tests exercise their documented enable/annotation gates, and duplicate-window boundary tests feed `SourceFile` evidence directly instead of being discarded as intentionally unparsable Rust.
- Applied the stable Rust formatting required by the Windows prerequisite gate; strict coverage, mutation, CRAP, and 400 logical-LOC thresholds remain unchanged.

## 2026-08-30 - r43 dead-reference, 400-LOC, and mutation-survivor hardening

- Replaced lexical dead-reference counting with a `syn::Visit` source-graph pass that recognizes bare and qualified function-pointer references, associated function references, method calls, and turbofish calls while still excluding direct self-recursion; added regressions for the r42 action-table and generic-call false positives.
- Split all eight r42 logical-LOC offenders below the configured 400-line target: `cargo-qa/main.rs`, `cargo-qa/paths.rs`, `qa-backends/performance.rs`, `qa-backends/release.rs`, `qa-backends/sanitizer.rs`, `qa-engine/engine.rs`, `qa-policy/config.rs`, and `qa-syntax/workspace.rs`.
- Applied survivor-directed production refactors and 65 additional exact regression tests across every module represented in the r42 set of 176 surviving mutants, prioritizing `process`, `mir`, `release`, `sanitizer`, `platform`, `state`, duplicate detection, metrics, and release engineering; no broad mutation exclusions were added.
- Kept the strict gates unchanged: coverage remains 90%, mutation remains 90%, CRAP remains 15, mutation timeouts remain fail-closed, and the 400 logical-LOC target remains visible in the normal structural metrics.

## 2026-08-29 - r42 mutation-timeout elimination

- Removed all five timeout shapes from the validated r41 mutation campaign without weakening the timeout gate: controlled child supervision now leaves the final `wait()` in the caller, so replacing the monitor helper cannot strand live children behind blocking stream joins; the obsolete whole-function `interrupt_child` mutation exclusion was removed as well.
- Reworked lexical call scanning so `calls`, `next_call_token`, and `next_identifier` pass cursor positions by value and the caller enforces monotonic progress; constant-return mutants can no longer create infinite loops.
- Added exact scanner-progress regressions for identifier boundaries, keyword skipping, and qualified calls, and shortened the controlled-process test children so a broken supervision mutant fails in seconds instead of approaching the mutation timeout.
- Preserved the passing r41 thresholds and evidence semantics: coverage remains 90%, mutation remains 90%, CRAP remains 15, and any genuine mutation timeout remains High/fail-closed.

## 2026-08-29 - r41 final strict-gate blocker repair

- Reduced the measured zero-coverage `coverage::run_coverage` CRAP hotspot by separating process preparation/execution from command classification; the strict CRAP 15 threshold is unchanged.
- Replaced the monolithic lexical call scanner with small covered helpers for identifier scanning, call-parenthesis recognition, and keyword filtering, reducing the measured `workspace::calls` complexity/CRAP without changing call-discovery semantics.
- Removed the two exact mutation-timeout triggers from the r40 campaign: release semver routing no longer relies on a negated boolean guard that can mutate into executing `cargo semver-checks`, and sanitizer pending/execution routing no longer relies on `!execute` that can mutate into launching the instrumented sanitizer workload.
- Preserved the validated r40 evidence contract: coverage remains 90%, mutation remains 90%, CRAP remains 15, surviving mutants above the aggregate mutation threshold remain actionable non-blocking evidence, and any actual mutation timeout remains High/fail-closed.

## 2026-08-28 - r40 formatter-only prerequisite repair

- Applied the exact stable Rust 1.98 `rustfmt` layouts reported by the r39 Windows prerequisite gate for the mutation-severity assertion and live coverage/CRAP regression fixture.
- No QA behavior, thresholds, coverage/mutation semantics, parser logic, or fail-fast policy changed from r39.

## 2026-08-28 - r39 coverage determinism and mutation-gate semantics repair

- Eliminated the Windows coverage-only self-test flake class by making every PID/counter-based QA test workspace remove any stale same-name directory before reuse; an interrupted or previously panicked process can no longer leak schemas, source files, reports, or fixtures into a later cargo-llvm-cov test process after PID reuse.
- Added an explicit engine regression proving that accepted coverage is applied to per-function coverage/CRAP and refreshed into the live summary as non-provisional before the Mutation phase begins.
- Added an internal expensive-phase fail-fast: when a forced coverage run is Failed/Unavailable, Mutation is explicitly skipped instead of launching a multi-hour cargo-mutants campaign that cannot make the already-invalid QA run pass.
- Corrected mutation gate semantics: the configured `minimum_kill_percent` remains the aggregate blocking threshold, so surviving mutants remain visible as `QA-MUT-002` Medium evidence instead of silently turning a 90% policy into a hidden 100% requirement; timed-out mutants remain High/fail-closed.
- Fixed Cargo-plugin bootstrap probes to invoke installed plugins through `cargo +stable <subcommand> --version` instead of directly executing plugin binaries with an unsupported `--version` shape, avoiding repeated no-op reinstall attempts on every Windows/Linux validation run.
- Kept coverage at 90%, mutation at 90%, CRAP at 15, fail-fast prerequisites, sanitizer gates, and byte-for-byte reproducibility unchanged.

## 2026-08-28 - r38 formatter-only repair

- Applied the exact stable Rust 1.98 `rustfmt` forms reported by the Windows prerequisite gate for the multiline type-source regression test and `qa-syntax` imports/unit-struct span branch.
- No parser semantics, QA thresholds, or self-hardening behavior changed from r37.

# Changelog

## 2026-08-28 - r37 exact type-source range repair

- Fixed `qa-syntax` multiline type-source extraction to use the actual `syn` delimiter spans instead of estimating a struct end from tokenized field-type line counts, which discard original source locations and truncated multiline structs.
- Applied exact source-range handling to named, tuple, and unit structs and replaced enum line-count heuristics with the enum closing-brace span.
- Strengthened the multiline type regression test to assert the complete struct and enum source slices, preserving exact line/source behavior for downstream rules and mutation testing.
- Preserved fail-fast prerequisite validation and all strict QA thresholds; no coverage, mutation, CRAP, or external-project policy was weakened.

## 2026-08-28 - r36 prerequisite test correctness repair

- Fixed the new engine threshold regression fixture so only the intended production function exceeds the configured cyclomatic limit; the helper intentionally counts all functions, so the unrelated test function now sits exactly at the boundary instead of CC 99.
- Rewrote the engine summary fixture with a single `RuleOutput` struct initializer to satisfy Clippy's `field_reassign_with_default` lint without suppressing warnings.
- Applied the exact stable-rustfmt wrapping reported by the r35 Windows run for the failed-coverage summary assertion.
- Preserved the fail-fast prerequisite gate and all strict QA thresholds; no production metric, mutation, coverage, or external-project behavior changed.

## 2026-08-28 - r35 prerequisite compile and formatting repair

- Fixed r34 mutation-hardening regression tests so they compile against the actual APIs: imported `engine::report::build_summary`, compared collected command environment keys through the correct `&OsStr` reference level, and asserted `ControlSnapshot::current_item` instead of a nonexistent `item` field.
- Applied the exact stable-rustfmt layouts reported by the Windows prerequisite run for process and security/error tests.
- Preserved fail-fast validation: the expensive self-hardening/mutation campaign remains skipped whenever formatting, check, Clippy, tests, doctests, or doctor fail.
- No QA threshold, mutation exclusion policy, coverage behavior, or external-project routing semantics were weakened or changed.

## 2026-08-28 - r34 coverage stability and mutation-threshold hardening

- Fixed the coverage-only generator-test failure by making the release-test temporary directory helper remove stale same-name state before each test; this prevents Windows PID reuse from making the first generated-output drift check inherit an old `generated.txt` and incorrectly pass.
- Applied the exact stable-rustfmt changes reported by the r33 Windows run and removed Windows-only unused-parameter warnings in the reproducibility flag adapter.
- Added mutation-sensitive exact tests for exception suppression, wildcard ordering, Gregorian date conversion, engine summary thresholds/health weighting, workspace parsing/module/test attribution, process environment restoration/diagnostics, mutation evidence failure paths, PE hardening output, FNV-1a/release artifact snapshots, and release evidence routing.
- Refactored mutation-prone cursor/stream loops in workspace discovery, state-terminal scanning, and process output draining so operator mutations cannot turn them into infinite loops; retained one narrowly scoped exclusion for replacing the entire active-child interruption routine with a no-op success because that mutation deterministically leaves the child alive and hangs the harness.
- Changed the Windows and Linux full validation runners to stop before the multi-hour self-hardening/mutation campaign when any prerequisite format/check/Clippy/test/doctest/doctor gate has failed, preventing expensive mutation runs on a source tree that is already known to be invalid.
- Kept coverage at 90%, mutation at 90%, CRAP at 15, and byte-for-byte reproducibility unchanged; no quality threshold was lowered and strict mutation runs still start from fresh evidence.

## 2026-08-27 - r33 mutation evidence recovery and survivor hardening

- Corrected the cargo-mutants output-path contract: `--output` is a parent directory and cargo-mutants creates `mutants.out/` beneath it. Local runs now target the project root so evidence remains `<project>/mutants.out/`; external runs target the isolated `mutations/` parent and read `<state>/mutations/mutants.out/outcomes.json`.
- Added a second fail-safe mutation evidence path that parses a completed cargo-mutants process summary and every printed `MISSED`/`TIMEOUT` item when machine-readable outcomes are unexpectedly unavailable, so a completed multi-hour campaign cannot collapse back to `N/A`/zero counts.
- Preserve semantic cargo-mutants exit results: campaigns with missed/time-out mutants are treated as available mutation evidence and then fail the unchanged 90% policy threshold instead of being misclassified as a broken backend.
- Structurally decomposed the two measured CRAP blockers (`coverage::collect` and `release::repro_mismatch_detail`) and added direct branch tests for the extracted helpers; the CRAP 15 limit is unchanged.
- Added mutation-sensitive assertions for external/local path routing, platform state homes, exact Windows reproducibility flags, coverage source/error fields, boundary sets, deterministic RNG vectors, rejected state transitions, SDK error/progress wrappers, sanitizer command mapping, performance assembly/bloat helpers, and release branch/repro helpers.
- Refactored host-specific path/repro helpers to unique names and exclude only code that is not compiled on the current host; active-host behavior remains fully mutation-tested.
- The first complete r32 self-hardening campaign exercised all 2,352 planned mutants (1,769 caught, 342 missed, 232 unviable, 9 timeouts), corresponding to an 83.44% strict mutation score. Fresh strict campaigns still start from a clean mutation-output directory; no prior mutant outcome is accepted as current verification.

## 2026-08-27 - r32 baseline compile repair

- Fixed the dashboard provisional-status regression test to reuse the existing fully populated report fixture instead of calling a nonexistent `SummaryMetrics::default()`, which had caused normal tests, cargo-llvm-cov, cargo-mutants baseline validation, and Windows ASan to fail before coverage or mutation evidence could be produced.
- Applied the exact stable-rustfmt layout reported for the provisional coverage-status fallback arm.
- Confirmed from the r31 evidence that release reproducibility remains available and byte-identical; no release or QA threshold was weakened.
- Kept fresh self-hardening mutation campaigns fail-closed: prior mutation output is still cleared before a requested run, so old mutant outcomes cannot be mistaken for current evidence.

## 2026-08-26 - r31 coverage baseline and live-status repair

- Fixed mutation command diagnostics so genuinely blank stdout/stderr fall back to the cargo-mutants exit status instead of being misclassified as a non-empty diagnostic wrapper; this repairs the unmutated baseline test that was simultaneously breaking coverage, mutation, and Windows ASan runs.
- Added direct whitespace-only diagnostics coverage so the fallback remains observable and mutation-sensitive.
- Made the live dashboard distinguish failed, unavailable, and disabled coverage evidence; a completed-but-failed coverage phase now says that coverage evidence failed and that CRAP/coverage remain unavailable instead of looking like coverage never ran.
- Applied the exact stable-rustfmt layout reported for the isolated cargo-llvm-cov environment array.
- Verified from the r30 Windows evidence that strict release reproducibility is now available and byte-identical; no reproducibility gate was weakened.
- Kept the strict 90% coverage and 90% mutation thresholds unchanged.

## 2026-08-26 - r30 backend isolation and Windows tool-path normalization

- Normalized Windows verbatim canonical project paths (`\\?\C:\...` / `\\?\UNC\...`) back to native tool paths before invoking Cargo and third-party QA tools, while retaining canonical path identity for project resolution.
- Isolated cargo-llvm-cov into a dedicated `llvm-cov-target` using its supported `CARGO_LLVM_COV_TARGET_DIR` and `CARGO_LLVM_COV_BUILD_DIR` controls so coverage profile files cannot be lost through shared-target cleanup.
- Prevented cargo-mutants from inheriting QA's external `CARGO_TARGET_DIR` override because cargo-mutants already builds mutations in scratch source trees; failed mutation commands now retain both stdout and stderr (or at least the exit status) instead of producing an empty blocker.
- Tightened Windows byte-for-byte reproducibility builds with serialized Cargo jobs, one codegen unit, disabled incremental compilation/debug info/PDB generation, stripped symbols, and `/Brepro` plus `/INCREMENTAL:NO`.
- Added regression coverage for native Windows path conversion, coverage-target reset/isolation, mutation command diagnostics, and deterministic reproducibility arguments.
- Kept all strict QA thresholds and external-project isolation semantics unchanged.

## 2026-08-26 - r29 live progress and Windows reproducibility repair

- Restored the live progress dashboard for terminal runs without undoing the non-interactive default: plain `cargo qa` and `--no-interactive` now show live TTY progress while running and still auto-exit; `--interactive` additionally keeps the post-run menu interactive. The Windows/Linux full runners now exercise that default directly instead of redundantly passing `--no-interactive`.
- Updated the remaining coverage evidence regression from the former one-decimal `91.2%` rendering to the intentional two-decimal `91.25%` rendering and removed the final rustfmt-only trailing blank line.
- Separated Windows reproducibility flags from hardening/path-disclosure flags. Same-path Windows repro builds no longer apply `--remap-path-prefix`, avoiding a known rustc Windows binary reproducibility problem; `/Brepro`, stable PDB naming, and `/INCREMENTAL:NO` remain enforced.
- Kept reproducibility byte-strict and added deterministic mismatch diagnostics with artifact name, run number, first differing byte offset, size, and FNV-1a fingerprints so any remaining toolchain nondeterminism is actionable rather than opaque.
- Kept the strict coverage, mutation, sanitizer, self-hardening, and release gates unchanged.

## 2026-08-26 - r28 Windows validation and self-hardening repair

- Applied the remaining stable Rust 1.98 rustfmt forms reported by the r27 Windows runner and removed the unused `reports_menu` re-export that Clippy rejected under `-D warnings`.
- Updated the live-dashboard regression to assert the intentional two-decimal coverage display, allowing ordinary tests, coverage, sanitizers, and mutation preflight to evaluate the current renderer instead of a stale `95.0%` expectation.
- Removed a production `.expect()` from external path parsing and replaced the tautological project-hash assertion with a fixed FNV-1a test vector while retaining path-sensitivity coverage.
- Split QA-engine finding/threshold logic into `engine/findings.rs`, reducing `engine.rs` below the framework's unchanged 600-line self-hardening source-sprawl limit without weakening the limit.
- Made reproducibility builds use one stable, fully cleaned target path across repeated builds so deterministic remap/linker flags are identical between runs; this avoids the reproducibility checker perturbing the binary by varying its own target-directory flag.
- Kept the strict 90% coverage and 90% mutation thresholds and the external-project routing semantics unchanged.

## 2026-08-26 - r27 validation repair

- Removed a stray token in `qa-report/src/render.rs` that prevented the report crate from parsing and cascaded into check, Clippy, test, doctest, doctor, and self-hardening failures.
- Applied the stable Rust 1.98 rustfmt forms reported by the Windows full-validation runner across the r26 external-project changes.
- No QA thresholds or external-project routing semantics were weakened or changed.

## 2026-08-26 - External-project UX, isolated state, and strict-gate follow-up

- Added first-class `--project-dir` / `--project`, `--output-dir`, and `--state-dir` routing while preserving plain `cargo qa` as the repository-local workflow.
- Made non-interactive execution the default; `--interactive` now explicitly opts in to the live dashboard, while `--no-interactive` remains accepted for compatibility.
- Added `UNIVERSAL_QA_STATE_HOME` plus platform state-home defaults for isolated external-project runs and stable per-project state hashing.
- Routed coverage, mutation, differential, fault, MIR, release/reproducibility artifacts, and child Cargo build output through the resolved run layout; external mode uses an external `CARGO_TARGET_DIR`.
- Added SDK and CLI regression coverage proving an external-project run does not create `qa-out`, `mutants.out`, or `target` in the inspected repository.
- Addressed the visible r25 mutation survivors with directly observable dashboard/report, fault-schedule, deterministic-rustflags, hardening, and boundary tests plus narrow exclusions only for host-inapplicable or intentionally non-terminating terminal mutations.
- Increased coverage display precision so a raw value just below 90% is not misleadingly rendered as `90.0%` while the unchanged strict threshold correctly blocks it.
- Kept the strict 90% coverage and 90% mutation-kill thresholds unchanged.

## 2026-08-25 - Windows r25 coverage and mutation hardening

- Applied the exact stable-rustfmt forms reported by the r24 Windows gate and fixed Clippy's `field_reassign_with_default` failure in the exception tests.
- Strengthened the real CLI exception workflow so a valid exception must be persisted before removal, killing the surviving required-details mutation instead of accepting an empty final state.
- Made sanitizer and MIR toolchain settings use non-default values in the integration workflow so deletion of those menu branches is observable.
- Added exact dashboard/report row renderers and tests for duplicate groups, occurrences, dead items, findings, evidence, files, function metrics, test coverage labels, and generated-report rows.
- Tightened live skip-control assertions so `SkipCurrent` and `SkipCategory` cannot be swapped without failing tests.
- Passed deterministic cargo-mutants exclusions explicitly for terminal-only I/O adapters, equivalent disjoint-bitflag OR/XOR mutations, and EOF/Back mutations that intentionally create non-terminating prompt loops; semantic core mutations remain under the unchanged 90% kill threshold.
- Kept the strict 90% coverage requirement and all existing CRAP, sanitizer, evidence, and fail-closed gates unchanged.

## 2026-08-24 - Windows r23 strict-gate repair

- Corrected cargo-mutants workspace testing to use the required boolean CLI form `--test-workspace=true` and made workspace mutation selection explicit with `--workspace`; the 90% mutation threshold remains unchanged.
- Removed redundant file-level `#![cfg(test)]` attributes from all source-backed unit-test modules; the existing parent `#[cfg(test)] mod tests;` gates remain authoritative and avoid Clippy `duplicated_attributes` failures.
- Applied the exact stable-rustfmt layout reported by the Windows diagnostic rerun, including blocker rendering and collision-resistant temporary-directory helpers.
- Added fail-closed mutation-backend tests for command classification, missing evidence, command-error preservation, empty-score handling, and modern outcome field fallbacks to increase executable coverage without weakening the 90% coverage requirement.
- Kept collision-resistant temporary test workspaces and the Phase 15 discovery diagnostic introduced in the previous hardening pass.

## 2026-08-24 - Windows self-hardening determinism and mutation campaign correction

- Applied the Windows `cargo fmt --check` formatting corrections reported by the strict self-hardening run.
- Made temporary test workspaces collision-resistant across parallel Windows test processes by combining the process ID with an atomic per-process sequence instead of relying on wall-clock resolution.
- Marked source-backed unit-test modules with `#![cfg(test)]` so mutation discovery does not treat test helpers as production mutation targets.
- Made blocker rendering deterministic and directly assertable, strengthened exception-menu behavioral tests, and added exact tests for exception filtering and required fields.
- Configured cargo-mutants to run the full workspace with all features so downstream integration contracts can kill lower-level mutants. Narrow exclusions cover only raw terminal adapters or synthetic prompt-loop mutations that cannot produce deterministic semantic evidence.
- Kept the strict coverage and mutation thresholds unchanged.

## 2026-08-24 - Live-dashboard coverage and mutation hardening

- Applied the exact Windows `cargo fmt --check` layout reported for the r21 live-progress change.
- Refactored the live dashboard into deterministic text renderers so pending, running, paused, skipping, and complete output can be asserted without terminal capture and presentation mutations no longer survive merely because stdout is difficult to intercept in unit tests.
- Added boundary assertions for progress-bar arithmetic, clamping, markers, elapsed time, progress notes, status transitions, and every supported progress-control key.
- Extracted and exhaustively tested terminal-mode decisions so `--interactive`, `--no-interactive`, piped stdin, and live-TTY selection distinguish every boolean combination.
- Added a real `reports` command integration route and strengthened progress-controller state, summary, completion, and clamping tests.
- Kept the strict 90% coverage, 90% mutation-kill, CRAP 15, sanitizer, and fail-closed skip requirements unchanged.

## 2026-08-23 - Live self-hardening progress and run controls

- Applied the exact stable-rustfmt layout reported by the r20 Windows gate before adding new behavior.
- Keep the existing QA results screen visible during terminal self-hardening runs and update health, coverage, mutation, findings, evidence, category progress, elapsed time, and the latest child-process status as evidence becomes available.
- Add a live progress bar with `P`/Space pause-resume, `S` skip-current-test/check, and `C` skip-current-category controls.
- Route backend child commands through a controlled process runner so pause/resume and skip operations apply to the active process tree instead of only changing the display.
- On Windows, suspend/resume the process tree through a bundled PowerShell process-control helper and use `taskkill /T /F` for skips; no unsafe Rust or new QA exception is introduced.
- Keep skips fail-closed: interrupted checks are recorded as unavailable/failed evidence and cannot turn an incomplete QA run green.
- Preserve non-terminal/CI behavior by falling back to the existing synchronous runner when stdin/stdout are not terminals.
- Keep the 90% coverage, 90% mutation-kill, CRAP 15, sanitizer, severity, and fail-closed strict-profile requirements unchanged.

## 2026-08-23 - CRAP hotspot decomposition and cargo-mutants 27.x diagnostics

- Structurally decomposed the twelve measured CRAP blockers reported by the Windows self-hardening run so the original hotspot functions no longer require coverage luck to remain below CRAP 15.
- Parse cargo-mutants 27.x `scenario.Mutant` outcomes, including source path, span line, and mutation name, while retaining compatibility with the older top-level `mutant` shape.
- Added focused mutation-sensitive tests for MIR, performance, release, sanitizer, mutation parsing, Windows ASan selection, and source sanitization.
- Kept all strict thresholds and fail-closed evidence requirements unchanged.

## 2026-08-23 - Windows compile, formatting, and dependency-evidence repair

- Applied the exact stable-rustfmt layout reported by the full Windows gate after the r18 hardening refactor.
- Fixed the dashboard metric test helper shadowing that prevented `cargo check`, Clippy, workspace tests, coverage, and Windows ASan from compiling the `cargo-qa` test target.
- Propagate release-settings secondary-action errors instead of deliberately discarding a `Result`, removing the `QA-ERR-001` self-analysis finding without weakening the rule.
- Removed the now-unused `qa-attr` dependency from `qa-engine`, addressing the dependency hygiene failure introduced by the refactor.
- Preserve both stdout and stderr for release/dependency backend failures so tools such as `cargo-machete` report the actual offending dependency instead of an opaque failure.
- Keep the 90% coverage, 90% mutation-kill, CRAP 15, sanitizer, severity, and fail-closed strict-profile requirements unchanged.

## 2026-08-23 - Measured self-hardening quality remediation

- Replaced string-based tautological-assertion detection with syntax-aware macro inspection so assertion text embedded in fixture literals is not reported as `QA-TEST-002`.
- Decomposed the interactive dashboard, settings dispatchers, CLI command routing, source discovery, engine orchestration, coverage, fuzz, and binary-hardening paths that dominated measured CRAP; under the framework's own cyclomatic metric no production function now has complexity intrinsically above the CRAP 15 ceiling.
- Added real-binary interactive dashboard workflows plus focused backend/engine/rule branch tests, with exhaustive postcondition assertions for every edited settings field, to raise executable coverage and mutation sensitivity instead of weakening the 90% coverage or 90% mutation requirements.
- Added bounded mutation-survivor/timeout details to terminal blocker output so any remaining mutation gap is attributable directly from the next self-hardening transcript.
- Kept coverage, mutation, CRAP, fuzz, sanitizer, severity, health, and fail-closed policies unchanged.

## 2026-08-23 - Exact Windows unit-test and tool-probe remediation

- Fixed unbounded-channel detection for turbofish calls such as `async_channel::unbounded::<T>()`, restoring `QA-RES-001` coverage without broadening policy exceptions.
- Normalized architecture-layer path separators before matching configured paths, so slash-form configuration works on Windows and Unix.
- Made tautological `assert_eq!` detection parse the top-level macro arguments, including nested production calls such as `assert_eq!(production(1), production(1))`.
- Reduced `qa-syntax` function-discovery argument count structurally instead of suppressing Clippy's `too_many_arguments` lint.
- Applied the exact rustfmt changes reported by the Windows gate to backend diagnostics.
- Preserve strict coverage, mutation, fuzz, sanitizer, severity, and health thresholds unchanged.

## 2026-08-23 - Failed-test diagnostic preservation

- Restored the last formatting-clean source-discovery and architecture baseline after the later speculative Windows fixes did not resolve the persistent `qa-rules --lib` failure.
- Kept exact unqualified self-edge filtering for dead-code analysis while using warning-clean map-entry idioms; qualified same-name call edges remain valid.
- Preserve both stdout and stderr, including the beginning and end of each stream, for failed coverage and sanitizer commands so Rust libtest failure names and panics are not hidden behind Cargo stderr.
- On Windows, repeat failed rustfmt and Clippy diagnostics and rerun `qa-rules` library tests directly at the end of the full runner so the final transcript contains the exact failing test and assertion.
- No coverage, mutation, fuzz, sanitizer, severity, health, or quality threshold was relaxed.

## 2026-08-23 - Strict gate regression cleanup

- Restored the function separation expected by rustfmt in the async/concurrency rule helpers.
- Refined dead-code self-edge filtering so only exact unqualified self calls are removed; qualified same-name calls remain valid source-graph edges.
- Added regression coverage for qualified same-name calls while retaining the recursive self-edge case.
- Preserved fail-closed sanitizer, coverage, mutation, and fuzz gates with the existing strict thresholds.

## 2026-08-23 - Dead-code self-edge and dynamic-evidence remediation

- Fixed source-graph dead-code analysis so a function declaration is not counted as an incoming call to itself.
- Treat otherwise-unreferenced recursive functions as dead unless another function actually references them.
- Added regression coverage for qualified calls and recursive self-edges.
- Preserved the existing Windows ASan runtime propagation and the strict 90% coverage / 90% mutation requirements.
- Kept coverage and sanitizer failures fail-closed; both can now progress past this shared `qa-rules --lib` baseline blocker.

## 2026-08-22 - Rule unit-test and literal-analysis remediation

- Fixed async detached-task detection so unrelated `let` bindings earlier on the same source line do not falsely classify a later `tokio::spawn` as supervised.
- Added comment masking that preserves string literals, allowing host-path and build-script policy checks to inspect literal values without triggering on comment-only examples.
- Made critical-layout `repr` recognition insensitive to token-stream whitespace and kept `OUT_DIR` detection effective after source sanitization.
- Reworked the structural-metrics control-flow fixture so its line-oriented LOC/cognitive expectations match the metric contract.
- Added focused regression tests for same-line spawn supervision, comment-only host paths, nested comments/string literals, stable repr handling, and comment-only `OUT_DIR` mentions.
- Preserved the strict 90% coverage and 90% mutation-kill requirements; no quality gate or severity threshold was weakened.

## 2026-08-22 - Windows ASan runtime and dynamic-evidence portability

- Discover the installed MSVC AddressSanitizer runtime DLL after Visual Studio component provisioning, export its directory through `QA_ASAN_RUNTIME_DIR`, and prepend it to `PATH` for the current Windows QA run.
- Make the sanitizer backend propagate that runtime directory into instrumented child processes and fail early with an actionable diagnostic when the required Windows ASan DLL is absent.
- Preserve both the beginning and end of cargo-llvm-cov stderr so a coverage test failure is no longer hidden behind compiler progress.
- Make artifact metadata tests accept Cargo's redirected absolute target directory, which is required by coverage and mutation runners.
- Normalize test-module separation introduced by the strict-profile test expansion without weakening formatting, coverage, mutation, or sanitizer gates.

## 2026-08-22 - Strict-profile test hardening

- Expanded the repository from the small baseline suite to 170+ focused executable tests across rules, backends, policy, syntax, reporting, SDK/runtime helpers, and the `cargo-qa` CLI.
- Added positive and negative branch assertions intended to kill condition, return-value, parser, status, persistence, and policy mutants rather than merely execute lines.
- Added module-local tests for private backend/rule helpers so coverage can reach implementation branches without recursively launching expensive QA tools.
- Added deterministic subprocess coverage for interactive CLI settings and exception workflows.
- Propagated `#[cfg(test)]` module context through source discovery so test helpers are not misclassified as production functions or production CRAP/safety findings.
- Moved large inline unit-test bodies into child `tests.rs` modules to keep production LOC/sprawl metrics representative of production code.
- Kept the strict 90% coverage and 90% mutation requirements unchanged.

## 2026-08-22 - Mutation and Windows ASan diagnostics

- Kept CRAP scoring focused on production functions so test helpers do not create production CRAP blockers.
- Enforced the configured workspace coverage threshold through the existing `QA-COV-001` rule instead of only displaying the threshold.
- Collapsed per-mutant High-finding spam into aggregate mutation blockers while retaining every surviving/timed-out mutant in `mutation.json`.
- Added caught/missed/timeout mutation counts to the terminal dashboard so mutation failures are immediately attributable.
- Preserved the configured mutation kill threshold and timeout strictness; no mutation gate was weakened.
- Preserved both the beginning and end of sanitizer stderr so Windows linker/runtime failures are visible instead of being hidden behind compiler progress.
- Windows QA tool setup now verifies the Visual Studio C++ AddressSanitizer component and provisions `Microsoft.VisualStudio.Component.VC.ASAN` when MSVC build tools are installed.

## 2026-08-22 - Windows compiler/self-hardening remediation

- Fixed the allocation runtime `dealloc` return-type error exposed by the first full Windows compile.
- Fixed the ambiguous cognitive-complexity depth type and the release-backend parse errors.
- Applied the Windows rustfmt diff across the workspace and removed the reported unused-variable warning.
- Added missing direct/dev dependencies required by the post-Phase-20 source (`serde` in `qa-backends`, `qa-attr` for `qa-rules` integration tests).
- Decomposed remaining functions exceeding the framework's default cyclomatic-complexity limit; no unexcepted function exceeds CC 12 under the framework's own metric.
- Removed self-analysis false positives from exception management while preserving explicit error handling.
- Repaired the stale `qa-report` all-target integration test so it constructs the Phase-20 report schema.
- Restored `std::error::Error::source()` chaining for SDK errors and narrowed secret-log heuristics to avoid configuration-name false positives.
- Made dashboard CC counts honor item-level `qa_attr::allow(cc = ...)` limits and excluded intentional golden fixtures from workspace health metrics.
- Added explicit coverage/mutation/fuzz backend evidence and noninteractive blocker details for actionable self-hardening failures.
- Removed the final unused private backend helper and cleaned warning-denied Clippy idioms in the CLI/source inventory.

## 2026-08-22 - Windows Cargo plugin bootstrap fix

- Fixed Windows self-hardening bootstrap aborting on a missing `cargo llvm-cov` subcommand before `cargo-llvm-cov` could be installed.
- Cargo plugins are now detected by their `cargo-<plugin>` executables on Windows and Linux, then installed with the current stable toolchain only when absent.
- Added a self-hardening regression check that rejects the unsafe missing-subcommand probe pattern.

## 0.1.0 - Phases 1-15

### Phases 1-7
- Foundation, policy/config import-export, interactive terminal dashboard and report browser.
- Structural metrics/sprawl/duplicate/dead-code/architecture analysis.
- Test validity and deterministic testkit helpers.
- `cargo-llvm-cov` evidence with per-function coverage and CRAP correlation.
- Deterministic `cargo-mutants` evidence and surviving-mutant reporting.
- Fuzz/property inventory and explicit fuzz-target build checks.
- Core panic/unsafe/math/parser/resource/allocation/environment rules.

### Phase 8 - State machines
- Critical state transition rejection/wildcard analysis.
- Critical-state round-trip and restart test ownership.
- Heuristic unreachable-state and terminal-state review.
- Async transition atomicity findings and state-machine testkit helpers.

### Phase 9 - Async/concurrency
- Cancellation contracts, blocking async calls, detached tasks, lock-across-await and panic-capable Drop checks.
- Unsafe Send/Sync rationale, `static mut`, and critical relaxed-atomic checks.
- Optional Loom/model-test backend with explicit evidence status.

### Phase 10 - Error/security
- Discarded Result, lost-context and Error-source-chain checks.
- Secret logging/formatting and Zeroize contracts.
- Source-level secret-dependent branch/index review plus optional repository-defined timing harness.

### Phase 11 - Native sanitizers
- Explicit ASan, LSan, TSan, MSan and optional RTSan execution using a configured nightly/target.
- Unsupported targets are N/A; missing toolchains are unavailable; incomplete MSan instrumentation remains unknown.

### Phase 12 - Differential testing
- Deterministically sorted corpus execution against independent reference/candidate commands.
- Exact, trimmed and canonical-JSON equivalence modes.
- Every divergence is persisted with input and both outcomes.

### Phase 13 - Fault injection
- `qa-fault-runtime` deterministic `(seed, kind, fail_at)` schedules for I/O, allocation, partial I/O, latency and clock faults.
- Explicit fault-test matrix and replayable failure JSONL evidence.

### Phase 14 - MIR analysis
- Per-package pinned-nightly MIR extraction.
- Correlation for panic edges, no-allocation contracts, hot-path drop cleanup, zeroization-survival signals and async state retention.

### Phase 15 - Platform/build/layout/FFI
- Default, no-default, all-features, optional each-feature, configured target and declared-MSRV checks.
- `build.rs` and proc-macro hermeticity rules.
- Critical repr/raw-byte/packed-layout review.
- FFI ABI type, panic boundary, raw-pointer and safety-documentation contracts.
