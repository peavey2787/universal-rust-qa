# Release Engineering — Phase 19

Phase 19 covers snapshot hygiene, doctests/examples, dependency/license/advisory checks, API/SemVer checks, generated-output drift/determinism, and isolated reproducible builds.

Configured generators are executed twice; original checked-in outputs are restored after each run. Reproducibility verification performs clean repeated builds through one stable target path with `SOURCE_DATE_EPOCH` and incremental compilation disabled, then byte-compares the configured binary artifacts. On Windows/MSVC the linker is forced into reproducible, non-incremental mode with a stable PDB reference and the repro build deliberately avoids `--remap-path-prefix`; hardening/path-disclosure builds retain remapping independently.
