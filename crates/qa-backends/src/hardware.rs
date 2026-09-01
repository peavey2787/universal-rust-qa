use qa_model::{EvidenceRecord, EvidenceStatus};
use qa_policy::QaConfig;
use std::{fs, path::Path};

pub fn run(workspace: &Path, config: &QaConfig, execute: bool) -> Vec<EvidenceRecord> {
    if !config.hardware.enabled {
        return vec![record("suite", EvidenceStatus::Disabled, None, "hardware profile disabled")];
    }
    if !execute {
        return vec![record(
            "suite",
            EvidenceStatus::Unknown,
            None,
            "explicit hardware run required",
        )];
    }

    let mut output = Vec::new();
    if let Some(target) = &config.hardware.target {
        let args = vec!["check".into(), "--workspace".into(), "--target".into(), target.clone()];
        output.push(job(workspace, "target-build", "cargo", &args, Some(target)));
    } else {
        output.push(record(
            "target-build",
            EvidenceStatus::NotApplicable,
            None,
            "no hardware target configured",
        ));
    }

    if let Some(map) = &config.hardware.linker_map {
        let path = workspace.join(map);
        match fs::read_to_string(&path) {
            Ok(text) => {
                let recognized = text
                    .lines()
                    .filter(|line| {
                        line.contains("_stack_top")
                            || line.contains("_sbss")
                            || line.contains("_ebss")
                    })
                    .count();
                output.push(record(
                    "linker-map",
                    if recognized > 0 {
                        EvidenceStatus::Available
                    } else {
                        EvidenceStatus::Unknown
                    },
                    Some(&path),
                    &format!("recognized {recognized} conventional linker-symbol rows"),
                ));
            }
            Err(error) => output.push(record(
                "linker-map",
                EvidenceStatus::Unavailable,
                Some(&path),
                &error.to_string(),
            )),
        }
    } else {
        output.push(record(
            "linker-map",
            EvidenceStatus::NotApplicable,
            None,
            "no linker map configured",
        ));
    }
    output
}

fn job(
    workspace: &Path,
    name: &str,
    program: &str,
    args: &[String],
    source: Option<&str>,
) -> EvidenceRecord {
    match super::process::run(workspace, program, args, &[]) {
        Ok(output) => record(
            name,
            if output.status.success() {
                EvidenceStatus::Available
            } else {
                EvidenceStatus::Failed
            },
            source.map(Path::new),
            &String::from_utf8_lossy(&output.stderr).chars().take(1000).collect::<String>(),
        ),
        Err(error) => {
            record(name, EvidenceStatus::Unavailable, source.map(Path::new), &error.to_string())
        }
    }
}

fn record(name: &str, status: EvidenceStatus, path: Option<&Path>, detail: &str) -> EvidenceRecord {
    EvidenceRecord {
        family: "HW".into(),
        check: name.into(),
        status,
        source: path.map(|path| path.display().to_string()),
        detail: Some(detail.into()),
    }
}

#[cfg(test)]
mod tests;
