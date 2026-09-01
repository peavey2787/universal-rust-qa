# Error, secret, and constant-time hygiene

Phase 10 checks discarded important results, erased error context, missing `Error::source()` chains, logging/formatting of likely secret-bearing values, `Debug`/`Display` and zeroization contracts for `#[qa_attr::secret]` types, and conservative secret-dependent branch/index signals in critical crypto code. Constant-time findings are review signals rather than proofs.
