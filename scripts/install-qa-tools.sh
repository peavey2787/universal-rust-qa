#!/usr/bin/env bash
set -Eeuo pipefail

# The workspace deliberately pins Rust 1.85 as its MSRV. Cargo QA utilities evolve
# independently and may require a newer compiler, so install them with the current
# stable toolchain rather than the workspace override.
rustup toolchain install stable --profile minimal
rustup component add --toolchain 1.85.0 rustfmt clippy llvm-tools-preview
rustup toolchain install nightly --profile minimal --component rust-src

# Discover Cargo plugins by their `cargo-<name>` executable first. Once present,
# probe through Cargo's real subcommand invocation contract rather than calling the
# plugin binary directly with an argument shape some plugins do not accept.
install_tool(){
  local executable="$1" crate="$2"
  if command -v "$executable" >/dev/null 2>&1; then
    local subcommand="${executable#cargo-}"
    if cargo +stable "$subcommand" --version >/dev/null 2>&1; then
      echo "Found $executable."
      return
    fi
    echo "WARN: $executable exists but 'cargo $subcommand --version' failed; reinstalling $crate." >&2
  fi
  echo "Installing $crate with current stable Rust..."
  if ! cargo +stable install --locked "$crate"; then
    echo "WARN: could not install $crate; the corresponding QA gate will report unavailable." >&2
    return 0
  fi
  hash -r
  if ! command -v "$executable" >/dev/null 2>&1; then
    echo "WARN: $crate installation completed but $executable is not discoverable on PATH." >&2
  fi
}

install_tool cargo-llvm-cov cargo-llvm-cov
install_tool cargo-mutants cargo-mutants
install_tool cargo-deny cargo-deny
install_tool cargo-hack cargo-hack
install_tool cargo-machete cargo-machete
install_tool cargo-semver-checks cargo-semver-checks
install_tool cargo-bloat cargo-bloat
install_tool cargo-llvm-lines cargo-llvm-lines
install_tool cargo-asm cargo-asm
install_tool cargo-insta cargo-insta
if [[ "$(uname -s)" != MINGW* && "$(uname -s)" != MSYS* ]]; then
  install_tool cargo-fuzz cargo-fuzz
fi
