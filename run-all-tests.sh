#!/usr/bin/env bash
set -Eeuo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT"
mkdir -p qa-out/self-hardening
LOG="qa-out/self-hardening/linux-$(date +%Y%m%d-%H%M%S).log"
exec > >(tee -a "$LOG") 2>&1

finish_pause(){
  if [[ -t 0 ]]; then read -r -p "Press Enter to close..." _; fi
}
prereq_failed(){
  local code=$?
  echo
  echo "PREREQUISITE FAILURE (exit $code). Full transcript: $LOG"
  finish_pause
  exit "$code"
}
trap prereq_failed ERR

printf '\nUniversal Rust QA — Linux full test + self-hardening\n====================================================\n'
./scripts/bootstrap.sh
# shellcheck disable=SC1090
[[ -f "$HOME/.cargo/env" ]] && source "$HOME/.cargo/env"
if [[ "${QA_SKIP_TOOL_INSTALL:-0}" != "1" ]]; then ./scripts/install-qa-tools.sh; fi
if [[ ! -f Cargo.lock ]]; then
  echo "Generating local Cargo.lock for --locked release gates..."
  cargo generate-lockfile
fi

# Collect the fast static/test gates together. The expensive self-hardening
# campaign runs only after every prerequisite gate is green.
trap - ERR
FAILURES=0
FAILED_STEPS=()
run_step(){
  local name="$1"; shift
  printf '\n==> %s\n' "$name"
  if "$@"; then
    printf 'PASS: %s\n' "$name"
  else
    local code=$?
    printf 'FAIL: %s (exit %s)\n' "$name" "$code"
    FAILURES=$((FAILURES+1))
    FAILED_STEPS+=("$name")
  fi
}

run_step "cargo fmt --check" cargo fmt --all -- --check
run_step "cargo check" cargo check --workspace --all-targets --all-features --locked
run_step "cargo clippy -D warnings" cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
run_step "cargo test" cargo test --workspace --all-targets --all-features --locked
run_step "cargo doctest" cargo test --workspace --doc --locked
run_step "cargo qa doctor" cargo run --locked -p cargo-qa -- qa doctor
if (( FAILURES == 0 )); then
  run_step "cargo qa self-hardening" cargo run --locked -p cargo-qa -- qa self-hardening
else
  printf '\nSKIP: cargo qa self-hardening because prerequisite gates failed.\n'
fi

printf '\n====================================================\n'
if (( FAILURES == 0 )); then
  printf 'PASS: all Linux tests and self-hardening completed.\n'
else
  printf 'FAIL: %d top-level step(s) failed:\n' "$FAILURES"
  printf '  - %s\n' "${FAILED_STEPS[@]}"
fi
printf 'Transcript: %s\nReports: %s\n' "$LOG" "$ROOT/qa-out"
finish_pause
(( FAILURES == 0 ))
