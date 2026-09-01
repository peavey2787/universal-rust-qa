# Dynamic sanitizers

Phase 11 runs configured native sanitizer campaigns only when explicitly requested. ASan, LSan, TSan, MSan, and optional RTSan are executed with the configured nightly and target. Unsupported/missing toolchains are never reported as passing. MSan remains `UNKNOWN` after a successful run unless the repository configuration explicitly attests complete dependency/FFI instrumentation.
