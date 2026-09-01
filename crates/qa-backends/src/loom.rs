use qa_model::{EvidenceRecord, EvidenceStatus};
use qa_policy::QaConfig;
use std::path::Path;

pub fn run(workspace: &Path, config: &QaConfig, execute: bool) -> EvidenceRecord {
    if !config.concurrency.loom_enabled {
        return record(EvidenceStatus::Disabled, "Loom/model-test execution disabled by policy");
    }
    if !execute {
        return record(
            EvidenceStatus::Unknown,
            "configured; explicit concurrency/full run required",
        );
    }
    execute_loom(workspace, config)
}

fn execute_loom(workspace: &Path, config: &QaConfig) -> EvidenceRecord {
    let args = vec![
        "test".into(),
        "--workspace".into(),
        "--features".into(),
        config.concurrency.loom_feature.clone(),
    ];
    match super::process::run(workspace, "cargo", &args, &[]) {
        Ok(output) => loom_output(output),
        Err(error) => record(EvidenceStatus::Unavailable, &error.to_string()),
    }
}

fn loom_output(output: std::process::Output) -> EvidenceRecord {
    let status = loom_status(output.status.success());
    let detail = String::from_utf8_lossy(&output.stderr).chars().take(1000).collect::<String>();
    record(status, &detail)
}

fn loom_status(success: bool) -> EvidenceStatus {
    if success { EvidenceStatus::Available } else { EvidenceStatus::Failed }
}

fn record(status: EvidenceStatus, detail: &str) -> EvidenceRecord {
    EvidenceRecord {
        family: "CONC".into(),
        check: "loom/model tests".into(),
        status,
        source: None,
        detail: Some(detail.into()),
    }
}

#[cfg(test)]
mod tests;
