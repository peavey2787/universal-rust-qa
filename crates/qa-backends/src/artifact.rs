use serde_json::Value;
use std::{
    collections::BTreeSet,
    env,
    path::{Path, PathBuf},
};

pub fn binary_names(workspace: &Path) -> Vec<String> {
    let args = vec!["metadata".into(), "--no-deps".into(), "--format-version".into(), "1".into()];
    let Ok(output) = super::process::run(workspace, "cargo", &args, &[]) else {
        return vec![];
    };
    let Ok(value) = serde_json::from_slice::<Value>(&output.stdout) else {
        return vec![];
    };

    let mut names = Vec::new();
    for package in value.get("packages").and_then(Value::as_array).into_iter().flatten() {
        for target in package.get("targets").and_then(Value::as_array).into_iter().flatten() {
            let is_binary = target
                .get("kind")
                .and_then(Value::as_array)
                .map(|kinds| kinds.iter().any(|kind| kind.as_str() == Some("bin")))
                .unwrap_or(false);
            if is_binary {
                if let Some(name) = target.get("name").and_then(Value::as_str) {
                    names.push(name.to_string());
                }
            }
        }
    }
    names.sort();
    names.dedup();
    names
}

pub fn binary_paths(workspace: &Path, target_dir: &Path, release: bool) -> Vec<PathBuf> {
    let profile = if release { "release" } else { "debug" };
    binary_names(workspace)
        .into_iter()
        .map(|name| {
            target_dir.join(profile).join(if cfg!(windows) { format!("{name}.exe") } else { name })
        })
        .collect()
}

pub fn default_target_dir(workspace: &Path) -> PathBuf {
    let args = vec!["metadata".into(), "--no-deps".into(), "--format-version".into(), "1".into()];
    super::process::run(workspace, "cargo", &args, &[])
        .ok()
        .and_then(|output| serde_json::from_slice::<Value>(&output.stdout).ok())
        .and_then(|value| value.get("target_directory").and_then(Value::as_str).map(PathBuf::from))
        .unwrap_or_else(|| workspace.join("target"))
}

pub fn deterministic_rustflags(workspace: &Path, target_dir: Option<&Path>) -> String {
    let mut mappings = Vec::new();
    if let Some(target_dir) = target_dir {
        mappings.push((target_dir.to_path_buf(), "/target"));
    }
    mappings.push((workspace.to_path_buf(), "/workspace"));
    for (key, destination) in [
        ("CARGO_HOME", "/cargo-home"),
        ("RUSTUP_HOME", "/rustup-home"),
        ("USERPROFILE", "/user"),
        ("HOME", "/user"),
    ] {
        if let Some(value) = env::var_os(key) {
            mappings.push((PathBuf::from(value), destination));
        }
    }

    let mut seen = BTreeSet::new();
    let mut flags = Vec::new();
    for (source, destination) in mappings {
        let source = source.display().to_string();
        if source.is_empty() || !seen.insert(source.clone()) {
            continue;
        }
        flags.push(format!("--remap-path-prefix={source}={destination}"));
    }
    #[cfg(windows)]
    {
        flags.push("-Clink-arg=/Brepro".into());
        flags.push("-Clink-arg=/PDBALTPATH:%_PDB%".into());
    }
    flags.join("\u{1f}")
}

pub fn reproducibility_rustflags(_workspace: &Path, _target_dir: Option<&Path>) -> String {
    #[cfg(windows)]
    {
        windows_reproducibility_rustflags()
    }
    #[cfg(not(windows))]
    {
        portable_reproducibility_rustflags(_workspace, _target_dir)
    }
}

#[cfg(windows)]
fn windows_reproducibility_rustflags() -> String {
    [
        "-Ccodegen-units=1",
        "-Cdebuginfo=0",
        "-Cstrip=symbols",
        "-Clink-arg=/Brepro",
        "-Clink-arg=/DEBUG:NONE",
        "-Clink-arg=/INCREMENTAL:NO",
    ]
    .join("\u{1f}")
}

#[cfg(not(windows))]
fn portable_reproducibility_rustflags(workspace: &Path, target_dir: Option<&Path>) -> String {
    deterministic_rustflags(workspace, target_dir)
}

#[cfg(test)]
mod tests;
