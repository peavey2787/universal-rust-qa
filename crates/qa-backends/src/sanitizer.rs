use qa_model::{EvidenceRecord, EvidenceStatus};
use qa_policy::QaConfig;
use std::path::Path;

pub fn run(workspace: &Path, config: &QaConfig, execute: bool) -> Vec<EvidenceRecord> {
    if config.sanitizers.mode.eq_ignore_ascii_case("off") {
        return vec![record(
            "matrix",
            EvidenceStatus::Disabled,
            None,
            "sanitizers disabled by policy",
        )];
    }
    if execute {
        let Some(target) = sanitizer_target(workspace, config) else {
            return vec![record(
                "matrix",
                EvidenceStatus::Unavailable,
                None,
                "unable to determine rustc host target",
            )];
        };
        let mut records = config
            .sanitizers
            .kinds
            .iter()
            .map(|kind| run_kind(workspace, config, &target, kind))
            .collect::<Vec<_>>();
        records.push(matrix_record(&target, &records));
        records
    } else {
        pending_records(config)
    }
}

fn pending_records(config: &QaConfig) -> Vec<EvidenceRecord> {
    config
        .sanitizers
        .kinds
        .iter()
        .map(|kind| record(kind, EvidenceStatus::Unknown, None, "explicit sanitizer run required"))
        .collect()
}

fn sanitizer_target(workspace: &Path, config: &QaConfig) -> Option<String> {
    config
        .sanitizers
        .target
        .clone()
        .or_else(|| discover_host(workspace, &config.sanitizers.toolchain))
}

fn run_kind(workspace: &Path, config: &QaConfig, target: &str, kind: &str) -> EvidenceRecord {
    let Some(sanitizer) = sanitizer_name(kind) else {
        return record(kind, EvidenceStatus::NotApplicable, Some(target), "unknown sanitizer kind");
    };
    if !supported(sanitizer, target) {
        return record(
            kind,
            EvidenceStatus::NotApplicable,
            Some(target),
            "sanitizer is not supported by this Rust target contract",
        );
    }
    execute_sanitizer(workspace, config, target, kind, sanitizer)
}

fn sanitizer_name(kind: &str) -> Option<&'static str> {
    match kind {
        "address" => Some("address"),
        "leak" => Some("leak"),
        "thread" => Some("thread"),
        "memory" => Some("memory"),
        "realtime" => Some("realtime"),
        _ => None,
    }
}

fn execute_sanitizer(
    workspace: &Path,
    config: &QaConfig,
    target: &str,
    kind: &str,
    sanitizer: &str,
) -> EvidenceRecord {
    let args = sanitizer_args(config, target);
    let flag = sanitizer_flag(sanitizer);
    let envs = match sanitizer_envs(target, sanitizer, &flag) {
        Ok(envs) => envs,
        Err(detail) => {
            return record(kind, EvidenceStatus::Unavailable, Some(target), &detail);
        }
    };
    execute_sanitizer_command(workspace, config, target, kind, sanitizer, &args, &envs)
}

fn execute_sanitizer_command(
    workspace: &Path,
    config: &QaConfig,
    target: &str,
    kind: &str,
    sanitizer: &str,
    args: &[String],
    envs: &[(&'static str, String)],
) -> EvidenceRecord {
    match super::process::run(workspace, "cargo", args, envs) {
        Ok(output) => sanitizer_output_record(config, target, kind, sanitizer, output),
        Err(error) => record(kind, EvidenceStatus::Unavailable, Some(target), &error.to_string()),
    }
}

fn sanitizer_output_record(
    config: &QaConfig,
    target: &str,
    kind: &str,
    sanitizer: &str,
    output: std::process::Output,
) -> EvidenceRecord {
    if output.status.success() {
        successful_record(config, target, kind, sanitizer)
    } else {
        failed_output_record(target, kind, &output.stdout, &output.stderr)
    }
}

mod support;
use support::*;

fn successful_record(
    config: &QaConfig,
    target: &str,
    kind: &str,
    sanitizer: &str,
) -> EvidenceRecord {
    if sanitizer == "memory" && !config.sanitizers.msan_complete_instrumentation {
        return record(
            kind,
            EvidenceStatus::Unknown,
            Some(target),
            "MSan execution completed, but policy does not attest complete dependency/FFI instrumentation",
        );
    }
    record(
        kind,
        EvidenceStatus::Available,
        Some(target),
        "sanitizer test workload completed successfully with instrumented std",
    )
}

fn failed_output_record(
    target: &str,
    kind: &str,
    stdout_bytes: &[u8],
    stderr_bytes: &[u8],
) -> EvidenceRecord {
    let detail = super::process::diagnostics(stdout_bytes, stderr_bytes);
    let lower = String::from_utf8_lossy(stderr_bytes).to_ascii_lowercase();
    let unsupported = ["not supported", "unsupported sanitizer", "unsupported target"]
        .iter()
        .any(|needle| lower.contains(needle));
    record(
        kind,
        if unsupported { EvidenceStatus::NotApplicable } else { EvidenceStatus::Failed },
        Some(target),
        &detail,
    )
}

fn matrix_record(target: &str, records: &[EvidenceRecord]) -> EvidenceRecord {
    let hard_fail = records
        .iter()
        .any(|item| matches!(item.status, EvidenceStatus::Failed | EvidenceStatus::Unavailable));
    let complete = records.iter().all(|item| {
        matches!(item.status, EvidenceStatus::Available | EvidenceStatus::NotApplicable)
    });
    let status = if hard_fail {
        EvidenceStatus::Failed
    } else if complete {
        EvidenceStatus::Available
    } else {
        EvidenceStatus::Unknown
    };
    record("matrix", status, Some(target), "configured sanitizer matrix evaluated")
}

fn record(
    check: &str,
    status: EvidenceStatus,
    target: Option<&str>,
    detail: &str,
) -> EvidenceRecord {
    EvidenceRecord {
        family: "SAN".into(),
        check: check.into(),
        status,
        source: target.map(str::to_string),
        detail: Some(detail.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_matrix_is_conservative() {
        assert!(supported("address", "x86_64-pc-windows-msvc"));
        assert!(!supported("thread", "x86_64-pc-windows-msvc"));
        assert!(supported("leak", "x86_64-unknown-linux-gnu"));
        assert!(supported("thread", "x86_64-unknown-linux-gnu"));
        assert!(supported("memory", "x86_64-unknown-linux-gnu"));
        assert!(!supported("memory", "x86_64-apple-darwin"));
        assert!(supported("realtime", "future-target"));
        assert!(!supported("unknown", "x86_64-unknown-linux-gnu"));
    }

    #[test]
    fn sanitizer_name_flags_and_args_are_exact() {
        assert_eq!(sanitizer_name("address"), Some("address"));
        assert_eq!(sanitizer_name("leak"), Some("leak"));
        assert_eq!(sanitizer_name("thread"), Some("thread"));
        assert_eq!(sanitizer_name("memory"), Some("memory"));
        assert_eq!(sanitizer_name("realtime"), Some("realtime"));
        assert_eq!(sanitizer_name("other"), None);
        assert_eq!(sanitizer_flag("address"), "-Zsanitizer=address");
        assert_eq!(sanitizer_flag("memory"), "-Zsanitizer=memory -Zsanitizer-memory-track-origins");
        let mut config = QaConfig::default();
        config.sanitizers.toolchain = "nightly-test".into();
        let args = sanitizer_args(&config, "x86_64-unknown-linux-gnu");
        assert_eq!(
            args,
            vec![
                "+nightly-test",
                "test",
                "-Zbuild-std",
                "--workspace",
                "--target",
                "x86_64-unknown-linux-gnu",
            ]
        );
    }

    #[test]
    fn run_states_and_pending_records_do_not_execute_tools() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut config = QaConfig::default();
        config.sanitizers.mode = "off".into();
        assert_eq!(run(root, &config, true)[0].status, EvidenceStatus::Disabled);

        config.sanitizers.mode = "explicit".into();
        config.sanitizers.kinds = vec!["address".into(), "thread".into()];
        let pending = run(root, &config, false);
        assert_eq!(pending.len(), 2);
        assert!(pending.iter().all(|record| record.status == EvidenceStatus::Unknown));

        config.sanitizers.target = Some("unknown-target".into());
        let records = run(root, &config, true);
        assert_eq!(records.len(), 3);
        assert_eq!(records[0].status, EvidenceStatus::NotApplicable);
        assert_eq!(records[1].status, EvidenceStatus::NotApplicable);
        assert_eq!(records[2].status, EvidenceStatus::Available);
    }

    #[test]
    fn successful_and_failed_records_encode_policy_semantics() {
        let mut config = QaConfig::default();
        assert_eq!(
            successful_record(&config, "x86_64-unknown-linux-gnu", "address", "address").status,
            EvidenceStatus::Available
        );
        assert_eq!(
            successful_record(&config, "x86_64-unknown-linux-gnu", "memory", "memory").status,
            EvidenceStatus::Unknown
        );
        config.sanitizers.msan_complete_instrumentation = true;
        assert_eq!(
            successful_record(&config, "x86_64-unknown-linux-gnu", "memory", "memory").status,
            EvidenceStatus::Available
        );

        let unsupported = failed_output_record(
            "x86_64-unknown-linux-gnu",
            "address",
            b"",
            b"error: unsupported sanitizer for target",
        );
        assert_eq!(unsupported.status, EvidenceStatus::NotApplicable);
        let failed = failed_output_record(
            "x86_64-unknown-linux-gnu",
            "address",
            b"test failure",
            b"linker exploded",
        );
        assert_eq!(failed.status, EvidenceStatus::Failed);
    }

    #[test]
    fn matrix_status_tracks_hard_fail_complete_and_unknown() {
        let available = record("address", EvidenceStatus::Available, None, "ok");
        let na = record("thread", EvidenceStatus::NotApplicable, None, "n/a");
        let unknown = record("memory", EvidenceStatus::Unknown, None, "unknown");
        let failed = record("leak", EvidenceStatus::Failed, None, "failed");
        assert_eq!(
            matrix_record("target", &[available.clone(), na]).status,
            EvidenceStatus::Available
        );
        assert_eq!(
            matrix_record("target", &[available.clone(), unknown]).status,
            EvidenceStatus::Unknown
        );
        assert_eq!(matrix_record("target", &[available, failed]).status, EvidenceStatus::Failed);
    }

    #[test]
    fn sanitizer_diagnostics_preserve_test_stdout_and_failure_tail() {
        let detail =
            crate::process::diagnostics(b"failures:\n    qa_rules::case", b"error: test failed");
        assert!(detail.contains("qa_rules::case"));
        assert!(detail.contains("error: test failed"));
    }

    #[test]
    fn sanitizer_environment_and_command_helpers_are_observable_without_instrumented_builds() {
        let envs =
            sanitizer_envs("x86_64-unknown-linux-gnu", "thread", "-Zsanitizer=thread").unwrap();
        assert_eq!(
            envs,
            vec![
                ("RUSTFLAGS", "-Zsanitizer=thread".into()),
                ("RUSTDOCFLAGS", "-Zsanitizer=thread".into()),
            ]
        );

        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"));
        let config = QaConfig::default();
        let args = vec!["--version".into()];
        let record = execute_sanitizer_command(
            workspace,
            &config,
            "x86_64-unknown-linux-gnu",
            "address",
            "address",
            &args,
            &[],
        );
        assert_eq!(record.status, EvidenceStatus::Available);
        assert_eq!(record.check, "address");
        assert_eq!(record.source.as_deref(), Some("x86_64-unknown-linux-gnu"));
    }

    #[test]
    fn sanitizer_output_record_maps_success_and_failure_processes_exactly() {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"));
        let config = QaConfig::default();
        let success =
            super::super::process::run(workspace, "rustc", &["--version".into()], &[]).unwrap();
        let success = sanitizer_output_record(
            &config,
            "x86_64-unknown-linux-gnu",
            "address",
            "address",
            success,
        );
        assert_eq!(success.status, EvidenceStatus::Available);

        let failed = super::super::process::run(
            workspace,
            "rustc",
            &["--definitely-not-a-real-rustc-option".into()],
            &[],
        )
        .unwrap();
        let failed = sanitizer_output_record(
            &config,
            "x86_64-unknown-linux-gnu",
            "address",
            "address",
            failed,
        );
        assert_eq!(failed.status, EvidenceStatus::Failed);
        assert!(failed.detail.as_deref().is_some_and(|detail| detail.contains("stderr:")));
    }

    #[cfg(windows)]
    #[test]
    fn windows_asan_runtime_is_required_only_for_supported_address_target() {
        assert!(needs_windows_asan_runtime("x86_64-pc-windows-msvc", "address"));
        assert!(!needs_windows_asan_runtime("x86_64-pc-windows-msvc", "thread"));
        assert!(!needs_windows_asan_runtime("x86_64-unknown-linux-gnu", "address"));
    }

    #[test]
    fn sanitizer_target_prefers_exact_configured_target_and_host_discovery_is_nonempty() {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut config = QaConfig::default();
        config.sanitizers.target = Some("configured-target".into());
        assert_eq!(sanitizer_target(workspace, &config).as_deref(), Some("configured-target"));

        config.sanitizers.target = None;
        let host = discover_host(workspace, &config.sanitizers.toolchain).unwrap();
        assert!(!host.is_empty());
        assert!(host.contains('-'));
        assert_eq!(sanitizer_target(workspace, &config).as_deref(), Some(host.as_str()));
    }

    #[cfg(windows)]
    #[test]
    fn provisioned_windows_asan_runtime_directory_is_real_and_nonempty() {
        let directory = windows_asan_runtime_dir().expect("Windows runner provisions ASan runtime");
        assert!(!directory.as_os_str().is_empty());
        assert!(directory.join("clang_rt.asan_dynamic-x86_64.dll").is_file());
    }

    #[cfg(windows)]
    #[test]
    fn windows_asan_runtime_is_appended_to_sanitizer_environment() {
        let runtime = windows_asan_runtime_dir().expect("Windows runner provisions ASan runtime");
        let mut envs = Vec::new();
        append_runtime_env(&mut envs, "x86_64-pc-windows-msvc", "address").unwrap();
        let path = envs
            .iter()
            .find_map(|(name, value)| (*name == "PATH").then_some(value))
            .expect("address sanitizer must extend PATH");
        assert!(std::env::split_paths(path).any(|entry| entry == runtime));

        let mut unrelated = Vec::new();
        append_runtime_env(&mut unrelated, "x86_64-pc-windows-msvc", "thread").unwrap();
        assert!(unrelated.is_empty());
    }
}
