use qa_model::{EvidenceRecord, EvidenceStatus};
use qa_policy::QaConfig;
use std::{fs, path::Path};

pub fn run(
    workspace: &Path,
    config: &QaConfig,
    artifact_root: &Path,
    execute: bool,
) -> Vec<EvidenceRecord> {
    if !config.fault.enabled {
        return vec![record(
            "suite",
            EvidenceStatus::Disabled,
            None,
            "fault injection disabled by policy",
        )];
    }
    if !execute {
        return vec![record(
            "suite",
            EvidenceStatus::Unknown,
            None,
            "configured; explicit fault run required",
        )];
    }
    let out_dir = artifact_root.join("fault");
    if let Err(error) = fs::create_dir_all(&out_dir) {
        return vec![record(
            "output-directory",
            EvidenceStatus::Failed,
            Some(&out_dir),
            &error.to_string(),
        )];
    }
    let mut records = Vec::new();
    let mut failures = Vec::new();
    for kind in &config.fault.kinds {
        records.push(run_kind(workspace, config, kind, &mut failures));
    }
    records.push(persist_failures(&out_dir, failures));
    records
}

fn run_kind(
    workspace: &Path,
    config: &QaConfig,
    kind: &str,
    failures: &mut Vec<serde_json::Value>,
) -> EvidenceRecord {
    run_kind_with(config, kind, failures, |fail_at| {
        let args = vec![
            "test".into(),
            "--workspace".into(),
            "--features".into(),
            config.fault.feature.clone(),
        ];
        let envs = [
            ("QA_FAULT_SEED", config.fault.seed.to_string()),
            ("QA_FAULT_KIND", kind.to_string()),
            ("QA_FAULT_AT", fail_at.to_string()),
        ];
        super::process::run(workspace, "cargo", &args, &envs)
            .map(|output| output.status.success())
            .map_err(|error| error.to_string())
    })
}

fn run_kind_with(
    config: &QaConfig,
    kind: &str,
    failures: &mut Vec<serde_json::Value>,
    mut schedule: impl FnMut(usize) -> Result<bool, String>,
) -> EvidenceRecord {
    let mut executed = 0usize;
    let mut failed = 0usize;
    for fail_at in 0..config.fault.max_fail_points {
        executed += 1;
        match schedule(fail_at) {
            Ok(true) => {}
            Ok(false) => {
                failed += 1;
                failures.push(failure_case(config, kind, fail_at, None));
            }
            Err(error) => {
                return record(
                    kind,
                    EvidenceStatus::Unavailable,
                    None,
                    &format!("seed={} fail_at={fail_at}: {error}", config.fault.seed),
                );
            }
        }
    }
    record(
        kind,
        if failed == 0 { EvidenceStatus::Available } else { EvidenceStatus::Failed },
        None,
        &format!(
            "{executed} deterministic fail points, {failed} failing schedules, seed={}",
            config.fault.seed
        ),
    )
}

fn failure_case(
    config: &QaConfig,
    kind: &str,
    fail_at: usize,
    detail: Option<&str>,
) -> serde_json::Value {
    serde_json::json!({
        "seed": config.fault.seed,
        "kind": kind,
        "fail_at": fail_at,
        "detail": detail,
    })
}

fn persist_failures(out_dir: &Path, failures: Vec<serde_json::Value>) -> EvidenceRecord {
    if failures.is_empty() {
        return record(
            "replay",
            EvidenceStatus::Available,
            None,
            "no failing schedules required persistence",
        );
    }
    let text =
        failures.into_iter().map(|value| value.to_string()).collect::<Vec<_>>().join("\n") + "\n";
    let path = out_dir.join("failures.jsonl");
    match fs::write(&path, text) {
        Ok(()) => record(
            "replay",
            EvidenceStatus::Available,
            Some(&path),
            "failing schedules persisted with seed/kind/fail_at for exact replay",
        ),
        Err(error) => record(
            "replay",
            EvidenceStatus::Failed,
            Some(&path),
            &format!("failed to persist replay schedules: {error}"),
        ),
    }
}

fn record(
    check: &str,
    status: EvidenceStatus,
    source: Option<&Path>,
    detail: &str,
) -> EvidenceRecord {
    EvidenceRecord {
        family: "FAULT".into(),
        check: check.into(),
        status,
        source: source.map(|path| path.display().to_string()),
        detail: Some(detail.into()),
    }
}

#[cfg(test)]
mod tests;
