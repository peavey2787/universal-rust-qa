use qa_model::{EvidenceRecord, EvidenceStatus};
use qa_policy::QaConfig;
use serde_json::{Map, Value};
use std::{collections::BTreeMap, fs, path::Path};

pub(super) fn instruction_baseline(
    workspace: &Path,
    config: &QaConfig,
    update: bool,
    counts: &BTreeMap<String, usize>,
    output: &mut Vec<EvidenceRecord>,
) {
    if counts.is_empty() {
        return;
    }
    let path = workspace.join(&config.performance.baseline_path);
    if update {
        write_instruction_baseline(&path, counts, output);
        return;
    }
    compare_instruction_baseline(&path, config, counts, output);
}

pub(super) fn write_instruction_baseline(
    path: &Path,
    counts: &BTreeMap<String, usize>,
    output: &mut Vec<EvidenceRecord>,
) {
    if let Some(parent) = path.parent() {
        if let Err(error) = fs::create_dir_all(parent) {
            output.push(super::record(
                "PERF",
                "baseline",
                EvidenceStatus::Failed,
                Some(path),
                &error.to_string(),
            ));
            return;
        }
    }
    let map = counts
        .iter()
        .map(|(name, count)| (name.clone(), Value::from(*count as u64)))
        .collect::<Map<_, _>>();
    let bytes = match serde_json::to_vec_pretty(&Value::Object(map)) {
        Ok(bytes) => bytes,
        Err(error) => {
            output.push(super::record(
                "PERF",
                "baseline",
                EvidenceStatus::Failed,
                Some(path),
                &error.to_string(),
            ));
            return;
        }
    };
    match fs::write(path, bytes) {
        Ok(()) => output.push(super::record(
            "PERF",
            "baseline",
            EvidenceStatus::Available,
            Some(path),
            "performance instruction baseline updated explicitly",
        )),
        Err(error) => output.push(super::record(
            "PERF",
            "baseline",
            EvidenceStatus::Failed,
            Some(path),
            &error.to_string(),
        )),
    }
}

pub(super) fn compare_instruction_baseline(
    path: &Path,
    config: &QaConfig,
    counts: &BTreeMap<String, usize>,
    output: &mut Vec<EvidenceRecord>,
) {
    let Some(base) = read_json_map::<usize>(path) else {
        output.push(super::record(
            "PERF",
            "baseline",
            EvidenceStatus::Unknown,
            Some(path),
            "no performance baseline found; run `cargo qa performance-baseline` to approve one explicitly",
        ));
        return;
    };
    for (name, current) in counts {
        if let Some(old) = base.get(name) {
            output.push(instruction_drift_record(path, config, name, *old, *current));
        }
    }
}

pub(super) fn instruction_drift_record(
    path: &Path,
    config: &QaConfig,
    name: &str,
    old: usize,
    current: usize,
) -> EvidenceRecord {
    let delta = percent_delta(old as u64, current as u64);
    super::record(
        "PERF",
        &format!("instruction-drift:{name}"),
        if delta > config.performance.instruction_deny_percent {
            EvidenceStatus::Failed
        } else {
            EvidenceStatus::Available
        },
        Some(path),
        &format!(
            "baseline {old}, current {current}, delta {delta:+.1}% (warn {:.1}%, deny {:.1}%)",
            config.performance.instruction_warn_percent,
            config.performance.instruction_deny_percent
        ),
    )
}

pub(super) fn binary_bloat(
    workspace: &Path,
    config: &QaConfig,
    update: bool,
    output: &mut Vec<EvidenceRecord>,
) {
    let current = current_binary_sizes(workspace);
    if current.is_empty() {
        output.push(no_binary_size_record());
        return;
    }
    binary_bloat_current(workspace, config, update, &current, output);
}

pub(super) fn no_binary_size_record() -> EvidenceRecord {
    super::record(
        "BLOAT",
        "binary-size",
        EvidenceStatus::NotApplicable,
        None,
        "no release binaries available for size-baseline comparison",
    )
}

pub(super) fn binary_bloat_current(
    workspace: &Path,
    config: &QaConfig,
    update: bool,
    current: &BTreeMap<String, u64>,
    output: &mut Vec<EvidenceRecord>,
) {
    let path = workspace.join(&config.bloat.baseline_path);
    if update {
        write_binary_baseline(&path, current, output);
    } else {
        compare_binary_baseline(&path, config, current, output);
    }
}

pub(super) fn current_binary_sizes(workspace: &Path) -> BTreeMap<String, u64> {
    let target_dir = super::super::artifact::default_target_dir(workspace);
    let mut current = BTreeMap::new();
    for path in super::super::artifact::binary_paths(workspace, &target_dir, true) {
        if let Ok(metadata) = fs::metadata(&path) {
            current.insert(
                path.file_name().unwrap_or_default().to_string_lossy().into_owned(),
                metadata.len(),
            );
        }
    }
    current
}

pub(super) fn write_binary_baseline(
    path: &Path,
    current: &BTreeMap<String, u64>,
    output: &mut Vec<EvidenceRecord>,
) {
    if let Some(parent) = path.parent() {
        if let Err(error) = fs::create_dir_all(parent) {
            output.push(super::record(
                "BLOAT",
                "baseline",
                EvidenceStatus::Failed,
                Some(path),
                &error.to_string(),
            ));
            return;
        }
    }
    let result = serde_json::to_vec_pretty(current)
        .map_err(|error| error.to_string())
        .and_then(|bytes| fs::write(path, bytes).map_err(|error| error.to_string()));
    match result {
        Ok(()) => output.push(super::record(
            "BLOAT",
            "baseline",
            EvidenceStatus::Available,
            Some(path),
            "binary-size baseline updated explicitly",
        )),
        Err(error) => output.push(super::record(
            "BLOAT",
            "baseline",
            EvidenceStatus::Failed,
            Some(path),
            &error,
        )),
    }
}

pub(super) fn compare_binary_baseline(
    path: &Path,
    config: &QaConfig,
    current: &BTreeMap<String, u64>,
    output: &mut Vec<EvidenceRecord>,
) {
    let Some(base) = read_json_map::<u64>(path) else {
        output.push(super::record(
            "BLOAT",
            "baseline",
            EvidenceStatus::Unknown,
            Some(path),
            "no binary-size baseline found; run `cargo qa performance-baseline` to approve one explicitly",
        ));
        return;
    };
    for (name, size) in current {
        if let Some(old) = base.get(name) {
            output.push(binary_drift_record(path, config, name, *old, *size));
        }
    }
}

pub(super) fn binary_drift_record(
    path: &Path,
    config: &QaConfig,
    name: &str,
    old: u64,
    size: u64,
) -> EvidenceRecord {
    let delta = size.saturating_sub(old);
    let percent = percent_delta(old, size);
    let failed =
        percent > config.bloat.max_percent_growth && delta > config.bloat.max_absolute_growth_bytes;
    super::record(
        "BLOAT",
        &format!("binary-size:{name}"),
        if failed { EvidenceStatus::Failed } else { EvidenceStatus::Available },
        Some(path),
        &format!(
            "baseline {old} bytes, current {size} bytes, delta {percent:+.2}% / {delta} bytes; limits {:.2}% and {} bytes",
            config.bloat.max_percent_growth, config.bloat.max_absolute_growth_bytes
        ),
    )
}

pub(super) fn read_json_map<T>(path: &Path) -> Option<BTreeMap<String, T>>
where
    T: serde::de::DeserializeOwned,
{
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

pub(super) fn percent_delta(old: u64, current: u64) -> f64 {
    if old == 0 { 0.0 } else { (current as f64 - old as f64) / old as f64 * 100.0 }
}
