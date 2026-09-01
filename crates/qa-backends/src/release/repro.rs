use qa_model::{EvidenceRecord, EvidenceStatus};
use qa_policy::QaConfig;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

pub(super) fn repro(
    workspace: &Path,
    config: &QaConfig,
    artifact_root: &Path,
    execute: bool,
) -> Vec<EvidenceRecord> {
    if let Some(record) = repro_precondition(workspace, config, execute) {
        return vec![record];
    }
    repro_execute(workspace, config, artifact_root)
}

pub(super) fn repro_execute(
    workspace: &Path,
    config: &QaConfig,
    artifact_root: &Path,
) -> Vec<EvidenceRecord> {
    let root = artifact_root.join("repro");
    if let Err(error) = fs::create_dir_all(&root) {
        return vec![super::record(
            "REPRO",
            "output-directory",
            EvidenceStatus::Failed,
            Some(&root),
            &error.to_string(),
        )];
    }
    repro_runs(workspace, config, &root)
}

pub(super) fn repro_runs(workspace: &Path, config: &QaConfig, root: &Path) -> Vec<EvidenceRecord> {
    let mut snapshots = Vec::new();
    let target_dir = root.join("build");
    for _ in 0..config.reproducibility.runs.max(2) {
        match repro_build_snapshot(workspace, config, &target_dir) {
            Ok(snapshot) => snapshots.push(snapshot.0),
            Err(record) => return vec![record],
        }
    }
    vec![repro_comparison(root, &snapshots)]
}

pub(super) struct ReproSnapshot(BTreeMap<String, Vec<u8>>);

pub(super) fn repro_precondition(
    workspace: &Path,
    config: &QaConfig,
    execute: bool,
) -> Option<EvidenceRecord> {
    if !config.reproducibility.enabled {
        return Some(super::record(
            "REPRO",
            "suite",
            EvidenceStatus::Disabled,
            None,
            "reproducible build verification disabled",
        ));
    }
    if !execute {
        return Some(super::record(
            "REPRO",
            "suite",
            EvidenceStatus::Unknown,
            None,
            "explicit release run required",
        ));
    }
    if crate::artifact::binary_names(workspace).is_empty() {
        return Some(super::record(
            "REPRO",
            "artifacts",
            EvidenceStatus::NotApplicable,
            None,
            "no binary targets discovered",
        ));
    }
    None
}

pub(super) fn repro_build_snapshot(
    workspace: &Path,
    config: &QaConfig,
    target_dir: &Path,
) -> Result<ReproSnapshot, EvidenceRecord> {
    if target_dir.exists() {
        clean_repro_target(target_dir)?;
    }
    let args = repro_build_args(config);
    let rustflags = crate::artifact::reproducibility_rustflags(workspace, Some(target_dir));
    let env = [
        ("CARGO_TARGET_DIR", target_dir.display().to_string()),
        ("CARGO_BUILD_JOBS", "1".into()),
        ("CARGO_INCREMENTAL", "0".into()),
        ("SOURCE_DATE_EPOCH", "1".into()),
        ("CARGO_ENCODED_RUSTFLAGS", rustflags),
    ];
    repro_build_result(
        workspace,
        config,
        target_dir,
        crate::process::run(workspace, "cargo", &args, &env),
    )
    .map(ReproSnapshot)
}

pub(super) fn clean_repro_target(target_dir: &Path) -> Result<(), EvidenceRecord> {
    fs::remove_dir_all(target_dir).map_err(|error| {
        super::record(
            "REPRO",
            "clean",
            EvidenceStatus::Failed,
            Some(target_dir),
            &error.to_string(),
        )
    })
}

pub(super) fn repro_build_result(
    workspace: &Path,
    config: &QaConfig,
    target_dir: &Path,
    result: std::io::Result<std::process::Output>,
) -> Result<BTreeMap<String, Vec<u8>>, EvidenceRecord> {
    match result {
        Ok(output) => repro_build_output(workspace, config, target_dir, output),
        Err(error) => Err(super::record(
            "REPRO",
            "build",
            EvidenceStatus::Unavailable,
            None,
            &error.to_string(),
        )),
    }
}

pub(super) fn repro_build_output(
    workspace: &Path,
    config: &QaConfig,
    target_dir: &Path,
    output: std::process::Output,
) -> Result<BTreeMap<String, Vec<u8>>, EvidenceRecord> {
    if output.status.success() {
        return Ok(snapshot_binaries(workspace, config, target_dir));
    }
    Err(super::record(
        "REPRO",
        "build",
        EvidenceStatus::Failed,
        None,
        &String::from_utf8_lossy(&output.stderr).chars().take(1000).collect::<String>(),
    ))
}

pub(super) fn repro_build_args(config: &QaConfig) -> Vec<String> {
    let mut args = vec!["build".to_string(), "--workspace".into(), "--jobs=1".into()];
    if config.reproducibility.release {
        args.push("--release".into());
    }
    if config.reproducibility.locked {
        args.push("--locked".into());
    }
    args
}

pub(super) fn snapshot_binaries(
    workspace: &Path,
    config: &QaConfig,
    target_dir: &Path,
) -> BTreeMap<String, Vec<u8>> {
    let mut snapshot = BTreeMap::new();
    for path in crate::artifact::binary_paths(workspace, target_dir, config.reproducibility.release)
    {
        if let Ok(bytes) = fs::read(&path) {
            snapshot
                .insert(path.file_name().unwrap_or_default().to_string_lossy().into_owned(), bytes);
        }
    }
    snapshot
}

pub(super) fn repro_comparison(
    root: &Path,
    snapshots: &[BTreeMap<String, Vec<u8>>],
) -> EvidenceRecord {
    let Some(first) = snapshots.first() else {
        return super::record(
            "REPRO",
            "binary-artifacts",
            EvidenceStatus::Unknown,
            Some(root),
            "no reproducibility snapshots were produced",
        );
    };
    if snapshots.iter().skip(1).all(|snapshot| snapshot == first) {
        return super::record(
            "REPRO",
            "binary-artifacts",
            EvidenceStatus::Available,
            Some(root),
            "configured binary artifacts are byte-identical across clean repeated builds",
        );
    }
    let detail = repro_mismatch_detail(first, snapshots);
    super::record("REPRO", "binary-artifacts", EvidenceStatus::Failed, Some(root), &detail)
}

pub(super) fn repro_mismatch_detail(
    first: &BTreeMap<String, Vec<u8>>,
    snapshots: &[BTreeMap<String, Vec<u8>>],
) -> String {
    snapshots
        .iter()
        .enumerate()
        .skip(1)
        .find_map(|(run, snapshot)| snapshot_mismatch_detail(first, snapshot, run + 1))
        .unwrap_or_else(|| {
            "release binary artifact sets differ across clean repeated builds".into()
        })
}

pub(super) fn snapshot_mismatch_detail(
    first: &BTreeMap<String, Vec<u8>>,
    snapshot: &BTreeMap<String, Vec<u8>>,
    run: usize,
) -> Option<String> {
    let names = first.keys().chain(snapshot.keys()).collect::<BTreeSet<_>>();
    names.into_iter().find_map(|name| match (first.get(name), snapshot.get(name)) {
        (Some(a), Some(b)) if a != b => {
            let offset = first_diff(a, b).unwrap_or(a.len().min(b.len()));
            Some(format!(
                "release binary `{name}` differs in run {run}: first byte offset {offset}; run1 size={} fnv64={:016x}; run{run} size={} fnv64={:016x}",
                a.len(),
                fnv64(a),
                b.len(),
                fnv64(b)
            ))
        }
        (Some(_), None) => Some(format!("release binary `{name}` is missing in run {run}")),
        (None, Some(_)) => Some(format!("release binary `{name}` appears only in run {run}")),
        _ => None,
    })
}

pub(super) fn first_diff(a: &[u8], b: &[u8]) -> Option<usize> {
    a.iter().zip(b).position(|(left, right)| left != right)
}

pub(super) fn fnv64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}
