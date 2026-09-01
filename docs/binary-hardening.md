# Binary Hardening — Phase 18

`QA-HARDEN-*` requires explicit release overflow checks and inspects final release artifacts. Linux uses `readelf` evidence for PIE, full RELRO, executable stack and RWX load segments; Windows uses `dumpbin` when available for ASLR/DEP; macOS uses `otool` for applicable Mach-O flags. Release builds use path remapping and the artifact is scanned for common developer-home path leakage.
