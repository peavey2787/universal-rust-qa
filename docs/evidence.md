# Evidence

Missing evidence is represented explicitly as unavailable. The engine never converts an unavailable backend into a numeric zero or passing score. Standard runs generate fresh LLVM coverage by default when coverage mode is `auto`; `--existing-coverage`/`--reuse-coverage` is the per-run opt-in to reuse the resolved JSON evidence, and `[coverage] mode = "existing"` makes that reuse policy persistent. Health remains provisional whenever LLVM coverage/CRAP is absent or coverage generation fails.
