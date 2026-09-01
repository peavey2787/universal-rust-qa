#!/usr/bin/env bash
set -Eeuo pipefail
if command -v cargo >/dev/null 2>&1; then exit 0; fi
if ! command -v curl >/dev/null 2>&1; then echo "ERROR: cargo is missing and curl is unavailable. Install Rust from rustup.rs." >&2; exit 1; fi
echo "Rust/Cargo not found. Installing the minimal stable rustup toolchain..."
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
# shellcheck disable=SC1090
source "$HOME/.cargo/env"
