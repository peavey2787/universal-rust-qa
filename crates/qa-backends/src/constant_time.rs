use qa_model::{EvidenceRecord, EvidenceStatus};
use qa_policy::QaConfig;
use std::path::Path;

pub fn run(workspace: &Path, config: &QaConfig, execute: bool) -> EvidenceRecord {
    if !config.constant_time.enabled {
        return record(EvidenceStatus::Disabled, "constant-time checks disabled");
    }
    let Some(command) = config.constant_time.command.as_deref() else {
        return record(
            EvidenceStatus::NotApplicable,
            "no timing/constant-time command configured; static QA-CT rules still run",
        );
    };
    if !execute {
        return record(
            EvidenceStatus::Unknown,
            "timing harness configured; explicit constant-time/full run required",
        );
    }

    let output = super::process::run_shell(workspace, command, &[]);

    match output {
        Ok(output) => record(
            if output.status.success() {
                EvidenceStatus::Available
            } else {
                EvidenceStatus::Failed
            },
            &String::from_utf8_lossy(&output.stderr).chars().take(1000).collect::<String>(),
        ),
        Err(error) => record(EvidenceStatus::Unavailable, &error.to_string()),
    }
}

fn record(status: EvidenceStatus, detail: &str) -> EvidenceRecord {
    EvidenceRecord {
        family: "CT".into(),
        check: "timing harness".into(),
        status,
        source: None,
        detail: Some(detail.into()),
    }
}

#[cfg(test)]
mod tests;
