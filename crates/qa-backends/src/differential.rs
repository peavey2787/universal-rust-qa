use qa_model::{EvidenceRecord, EvidenceStatus};
use qa_policy::{DifferentialTarget, QaConfig};
use serde_json::Value;
use std::{
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
};

pub fn run(
    workspace: &Path,
    config: &QaConfig,
    artifact_root: &Path,
    execute: bool,
) -> Vec<EvidenceRecord> {
    if !config.differential.enabled {
        return vec![record(
            "suite",
            EvidenceStatus::Disabled,
            None,
            "differential testing disabled by policy",
        )];
    }
    let mut records = Vec::new();
    for target in &config.differential.target {
        records.extend(run_target(workspace, config, artifact_root, target, execute));
    }
    records
}

fn run_target(
    workspace: &Path,
    config: &QaConfig,
    artifact_root: &Path,
    target: &DifferentialTarget,
    execute: bool,
) -> Vec<EvidenceRecord> {
    if target.reference_command.trim() == target.candidate_command.trim() {
        return vec![record(
            &format!("{}:oracle", target.name),
            EvidenceStatus::Failed,
            None,
            "reference and candidate commands are identical; oracle is not independent",
        )];
    }

    let mut records = vec![record(
        &format!("{}:oracle", target.name),
        EvidenceStatus::Available,
        None,
        "reference and candidate entry commands differ",
    )];
    if !execute {
        records.push(record(
            &target.name,
            EvidenceStatus::Unknown,
            None,
            "configured; explicit differential run required",
        ));
        return records;
    }
    records.extend(execute_target(workspace, config, artifact_root, target));
    records
}

fn execute_target(
    workspace: &Path,
    config: &QaConfig,
    artifact_root: &Path,
    target: &DifferentialTarget,
) -> Vec<EvidenceRecord> {
    let corpus = workspace.join(&target.corpus);
    let mut cases = match corpus_files(&corpus) {
        Ok(cases) => cases,
        Err(error) => {
            return vec![record(&target.name, EvidenceStatus::Failed, Some(&corpus), &error)];
        }
    };
    cases.sort();
    let stats = execute_cases(workspace, artifact_root, target, &cases);
    vec![
        record(
            &target.name,
            if stats.divergences == 0 { EvidenceStatus::Available } else { EvidenceStatus::Failed },
            Some(&corpus),
            &format!(
                "{} deterministic cases, {} divergences, seed {}",
                stats.executed, stats.divergences, config.differential.seed
            ),
        ),
        record(
            &format!("{}:persistence", target.name),
            if stats.persisted == stats.divergences {
                EvidenceStatus::Available
            } else {
                EvidenceStatus::Failed
            },
            None,
            &format!("persisted {}/{} divergences", stats.persisted, stats.divergences),
        ),
    ]
}

#[derive(Debug, Default)]
struct DifferentialStats {
    executed: usize,
    divergences: usize,
    persisted: usize,
}

fn execute_cases(
    workspace: &Path,
    artifact_root: &Path,
    target: &DifferentialTarget,
    cases: &[PathBuf],
) -> DifferentialStats {
    let mut stats = DifferentialStats::default();
    for path in cases {
        let Ok(input) = fs::read(path) else {
            continue;
        };
        let reference = pipe(&target.reference_command, workspace, &input);
        let candidate = pipe(&target.candidate_command, workspace, &input);
        stats.executed += 1;
        if equivalent(target, &reference, &candidate) {
            continue;
        }
        stats.divergences += 1;
        if persist(artifact_root, target, path, &input, &reference, &candidate).is_ok() {
            stats.persisted += 1;
        }
    }
    stats
}

#[derive(Debug)]
struct Outcome {
    success: bool,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

fn pipe(command: &str, workspace: &Path, input: &[u8]) -> Option<Outcome> {
    let output = super::process::run_shell_with_input(workspace, command, input).ok()?;
    Some(Outcome { success: output.status.success(), stdout: output.stdout, stderr: output.stderr })
}

fn equivalent(
    target: &DifferentialTarget,
    reference: &Option<Outcome>,
    candidate: &Option<Outcome>,
) -> bool {
    let (Some(reference), Some(candidate)) = (reference, candidate) else {
        return false;
    };
    if reference.success != candidate.success {
        return false;
    }
    match target.equivalence.as_str() {
        "exact" => reference.stdout == candidate.stdout,
        "trimmed" => trim(&reference.stdout) == trim(&candidate.stdout),
        "canonical-json" | "canonical_json" => json(&reference.stdout) == json(&candidate.stdout),
        _ => false,
    }
}

fn trim(bytes: &[u8]) -> Vec<u8> {
    String::from_utf8_lossy(bytes).trim().as_bytes().to_vec()
}

fn json(bytes: &[u8]) -> Option<Value> {
    serde_json::from_slice(bytes).ok()
}

fn corpus_files(path: &Path) -> Result<Vec<PathBuf>, String> {
    let entries = fs::read_dir(path).map_err(|error| error.to_string())?;
    Ok(entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect())
}

fn persist(
    artifact_root: &Path,
    target: &DifferentialTarget,
    case: &Path,
    input: &[u8],
    reference: &Option<Outcome>,
    candidate: &Option<Outcome>,
) -> std::io::Result<()> {
    let output_dir = artifact_root.join("differential").join(&target.name);
    fs::create_dir_all(&output_dir)?;
    let id = fnv1a(input);
    let payload = serde_json::json!({
        "case": case.display().to_string(),
        "equivalence": target.equivalence.as_str(),
        "input_hex": hex(input),
        "reference": outcome_json(reference),
        "candidate": outcome_json(candidate),
    });
    fs::write(
        output_dir.join(format!("{id:016x}.json")),
        serde_json::to_vec_pretty(&payload).map_err(std::io::Error::other)?,
    )
}

fn outcome_json(value: &Option<Outcome>) -> Value {
    match value {
        Some(outcome) => serde_json::json!({
            "success": outcome.success,
            "stdout": String::from_utf8_lossy(&outcome.stdout),
            "stderr": String::from_utf8_lossy(&outcome.stderr),
        }),
        None => Value::Null,
    }
}

fn fnv1a(data: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in data {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn hex(data: &[u8]) -> String {
    let mut output = String::with_capacity(data.len() * 2);
    for byte in data {
        if write!(&mut output, "{byte:02x}").is_err() {
            return String::new();
        }
    }
    output
}

fn record(
    check: &str,
    status: EvidenceStatus,
    source: Option<&Path>,
    detail: &str,
) -> EvidenceRecord {
    EvidenceRecord {
        family: "DIFF".into(),
        check: check.into(),
        status,
        source: source.map(|path| path.display().to_string()),
        detail: Some(detail.into()),
    }
}

#[cfg(test)]
mod tests;
