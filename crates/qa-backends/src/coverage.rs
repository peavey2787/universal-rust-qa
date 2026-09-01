use qa_model::EvidenceStatus;
use qa_policy::QaConfig;
use serde_json::Value;
use std::{collections::BTreeMap, fs, path::Path};

#[derive(Debug, Clone, Default)]
pub struct CoverageEvidence {
    pub status: EvidenceStatus,
    pub percent: Option<f64>,
    pub source: Option<String>,
    pub error: Option<String>,
    pub files: BTreeMap<String, BTreeMap<usize, u64>>,
}

pub fn collect(
    workspace: &Path,
    config: &QaConfig,
    output: &Path,
    force: bool,
) -> CoverageEvidence {
    collect_with(config, output, force, || run_coverage(workspace, config, output))
}

fn run_coverage(workspace: &Path, config: &QaConfig, output: &Path) -> CoverageCommand {
    run_coverage_process(workspace, config, output)
        .map(coverage_command)
        .unwrap_or_else(CoverageCommand::Unavailable)
}

fn run_coverage_process(
    workspace: &Path,
    config: &QaConfig,
    output: &Path,
) -> Result<std::io::Result<std::process::Output>, String> {
    let path = output.join("llvm-cov.json");
    let target = prepare_coverage_target(output)?.display().to_string();
    let args = coverage_args(config, &path);
    let env = coverage_env(&target);
    Ok(super::process::with_cargo_target_dir(None, || {
        super::process::run(workspace, "cargo", &args, &env)
    }))
}

fn coverage_env(target: &str) -> [(&'static str, String); 3] {
    [
        ("CARGO_LLVM_COV_TARGET_DIR", target.into()),
        ("CARGO_LLVM_COV_BUILD_DIR", target.into()),
        ("CARGO_LLVM_COV_SETUP", "yes".into()),
    ]
}

fn coverage_args(config: &QaConfig, path: &Path) -> Vec<String> {
    let mut args = vec![
        "llvm-cov".into(),
        "--json".into(),
        "--output-path".into(),
        path.display().to_string(),
    ];
    if config.coverage.all_features {
        args.push("--all-features".into());
    }
    args
}

fn prepare_coverage_target(output: &Path) -> Result<std::path::PathBuf, String> {
    fs::create_dir_all(output).map_err(|error| {
        format!("failed to create coverage output {}: {error}", output.display())
    })?;
    let evidence = output.join("llvm-cov.json");
    if evidence.exists() {
        fs::remove_file(&evidence).map_err(|error| {
            format!("failed to reset coverage evidence {}: {error}", evidence.display())
        })?;
    }
    let target = output.join("llvm-cov-target");
    if target.exists() {
        fs::remove_dir_all(&target).map_err(|error| {
            format!("failed to reset coverage target {}: {error}", target.display())
        })?;
    }
    Ok(target)
}

#[derive(Debug)]
enum CoverageCommand {
    Success,
    Failed(String),
    Unavailable(String),
}

fn coverage_command(result: std::io::Result<std::process::Output>) -> CoverageCommand {
    match result {
        Ok(output) if output.status.success() => CoverageCommand::Success,
        Ok(output) => {
            CoverageCommand::Failed(super::process::diagnostics(&output.stdout, &output.stderr))
        }
        Err(error) => CoverageCommand::Unavailable(error.to_string()),
    }
}

fn collect_with(
    config: &QaConfig,
    output: &Path,
    force: bool,
    command: impl FnOnce() -> CoverageCommand,
) -> CoverageEvidence {
    if config.coverage.mode == "off" {
        return CoverageEvidence { status: EvidenceStatus::Disabled, ..Default::default() };
    }
    let generate = force && config.coverage.mode != "existing";
    if generate {
        match command() {
            CoverageCommand::Success => {}
            CoverageCommand::Failed(error) => {
                return CoverageEvidence {
                    status: EvidenceStatus::Failed,
                    error: Some(error),
                    ..Default::default()
                };
            }
            CoverageCommand::Unavailable(error) => {
                return CoverageEvidence {
                    status: EvidenceStatus::Unavailable,
                    error: Some(error),
                    ..Default::default()
                };
            }
        }
    }
    let path = output.join("llvm-cov.json");
    if !path.exists() {
        return CoverageEvidence {
            status: EvidenceStatus::Unavailable,
            error: Some(
                "existing cargo-llvm-cov JSON evidence not found; rerun without the coverage reuse flag or set [coverage] mode = \"auto\" to generate fresh coverage"
                    .into(),
            ),
            ..Default::default()
        };
    }
    parse(&path)
}

fn parse(path: &Path) -> CoverageEvidence {
    let text = match fs::read_to_string(path) {
        Ok(value) => value,
        Err(error) => {
            return CoverageEvidence {
                status: EvidenceStatus::Failed,
                error: Some(error.to_string()),
                ..Default::default()
            };
        }
    };
    let value: Value = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(error) => {
            return CoverageEvidence {
                status: EvidenceStatus::Failed,
                error: Some(error.to_string()),
                ..Default::default()
            };
        }
    };

    let percent = value.pointer("/data/0/totals/lines/percent").and_then(Value::as_f64);
    let mut files = BTreeMap::new();
    for file in value.pointer("/data/0/files").and_then(Value::as_array).into_iter().flatten() {
        let Some(name) = file.get("filename").and_then(Value::as_str) else {
            continue;
        };
        let mut lines = BTreeMap::<usize, u64>::new();
        if let Some(segments) = file.get("segments").and_then(Value::as_array) {
            for segment in segments {
                let Some(parts) = segment.as_array() else {
                    continue;
                };
                let Some(line) = parts.first().and_then(Value::as_u64) else {
                    continue;
                };
                let count = parts.get(2).and_then(Value::as_u64).unwrap_or(0);
                let line = line as usize;
                lines.entry(line).and_modify(|value| *value = (*value).max(count)).or_insert(count);
            }
        }
        files.insert(normalize(name), lines);
    }

    CoverageEvidence {
        status: EvidenceStatus::Available,
        percent,
        source: Some(path.display().to_string()),
        files,
        ..Default::default()
    }
}

pub fn function_percent(
    evidence: &CoverageEvidence,
    path: &str,
    start: usize,
    end: usize,
) -> Option<f64> {
    if evidence.status != EvidenceStatus::Available {
        return None;
    }
    let key = normalize(path);
    let lines = evidence.files.get(&key).or_else(|| {
        evidence
            .files
            .iter()
            .find(|(candidate, _)| candidate.ends_with(&key) || key.ends_with(candidate.as_str()))
            .map(|(_, lines)| lines)
    })?;
    let relevant = lines.range(start..=end).collect::<Vec<_>>();
    if relevant.is_empty() {
        return None;
    }
    let covered = relevant.iter().filter(|(_, count)| **count > 0).count();
    Some(100.0 * covered as f64 / relevant.len() as f64)
}

fn normalize(path: &str) -> String {
    path.replace('\\', "/")
}

#[cfg(test)]
mod tests;
