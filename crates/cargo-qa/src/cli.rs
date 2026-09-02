use qa_policy::QaConfig;
use std::{fs, path::Path};
pub fn help() {
    println!(
        r#"cargo qa [COMMAND] [PATH OPTIONS] [--interactive]

Path and state options:
  --project-dir <dir>   analyze another project without changing directory
  --project <dir>       alias for --project-dir
  --output-dir <dir>    write final reports directly to this directory
  --state-dir <dir>     write QA transient state to this directory

State behavior:
  cargo qa              local mode: qa-out/, mutants.out/, and normal Cargo target/ stay in project
  --project-dir <dir>   isolated mode: external per-project state and external CARGO_TARGET_DIR
  --output-dir <dir>    if --state-dir is omitted, transient state is placed under <dir>/state
  --state-dir <dir>     if --output-dir is omitted, reports are written under <dir>/reports
  UNIVERSAL_QA_STATE_HOME overrides the external state home when --state-dir is omitted

Coverage behavior:
  standard/full/release runs generate fresh cargo-llvm-cov JSON coverage by default
  --existing-coverage   reuse the resolved llvm-cov.json instead of generating coverage
  --reuse-coverage      alias for --existing-coverage
  [coverage] mode="existing" persistently reuses the resolved llvm-cov.json
  [coverage] mode="off" disables coverage even when fresh generation is requested

Interaction:
  non-interactive completion is the default; a TTY still shows live progress
  --interactive         keep the post-run dashboard/menu interactive after live progress
  --no-interactive      accepted for compatibility; explicitly auto-exit after the run

Commands:
  coverage              force a fresh cargo-llvm-cov collection
  mutants               run deterministic cargo-mutants campaign
  fuzz                   build configured fuzz targets
  concurrency           run configured Loom/model tests
  constant-time         run configured timing harness
  sanitizers            run ASan/LSan/TSan/MSan matrix
  differential          run deterministic differential corpus
  fault                 run deterministic fault schedules
  mir                   emit/analyze MIR with pinned nightly
  platform              feature/target/MSRV matrix
  hardware              embedded/MMIO/ISR/linker contracts
  performance           hot-path assembly/vectorization/bloat checks
  performance-baseline  explicitly approve current instruction baseline
  hardening             build and inspect release binary mitigations
  full                  run implemented runtime/compiler QA families
  release               full + hardening/docs/deps/API/generated/repro
  self-hardening        release + QA framework self-integrity gates

Interactive terminal controls while QA is running:
  P or Space            pause/resume active external check
  S                     skip current external test/check (fail-closed)
  C                     skip current external-check category (fail-closed)

Utility commands:
  --version | -V
  doctor | settings | exceptions | reports
  export-config <file> | import-config <file>"#
    )
}
pub fn export_config(w: &Path, d: &Path) -> Result<(), Box<dyn std::error::Error>> {
    QaConfig::load(w)?.save(d)?;
    Ok(())
}
pub fn import_config(w: &Path, s: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let t = fs::read_to_string(s)?;
    let _: QaConfig = toml::from_str(&t)?;
    fs::write(w.join("qa.toml"), t)?;
    Ok(())
}
