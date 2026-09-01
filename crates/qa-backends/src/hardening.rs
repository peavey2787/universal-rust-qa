use qa_model::{EvidenceRecord, EvidenceStatus};
use qa_policy::QaConfig;
use std::{
    collections::BTreeSet,
    env, fs,
    path::{Path, PathBuf},
};

pub fn run(workspace: &Path, config: &QaConfig, execute: bool) -> Vec<EvidenceRecord> {
    run_with(
        workspace,
        config,
        execute,
        || release_build(workspace),
        || {
            let target_dir = super::artifact::default_target_dir(workspace);
            super::artifact::binary_paths(workspace, &target_dir, true)
        },
    )
}

#[derive(Debug)]
enum BuildOutcome {
    Success,
    Failed(String),
    Unavailable(String),
}

fn release_build(workspace: &Path) -> BuildOutcome {
    let args = vec!["build".into(), "--workspace".into(), "--release".into(), "--locked".into()];
    let target_dir = super::artifact::default_target_dir(workspace);
    let rustflags = super::artifact::deterministic_rustflags(workspace, Some(&target_dir));
    let envs = [
        ("CARGO_INCREMENTAL", "0".into()),
        ("SOURCE_DATE_EPOCH", "1".into()),
        ("CARGO_ENCODED_RUSTFLAGS", rustflags),
    ];
    build_outcome(super::process::run(workspace, "cargo", &args, &envs))
}

fn build_outcome(result: std::io::Result<std::process::Output>) -> BuildOutcome {
    match result {
        Ok(output) if output.status.success() => BuildOutcome::Success,
        Ok(output) => BuildOutcome::Failed(stderr(&output.stderr)),
        Err(error) => BuildOutcome::Unavailable(error.to_string()),
    }
}

fn run_with(
    workspace: &Path,
    config: &QaConfig,
    execute: bool,
    build: impl FnOnce() -> BuildOutcome,
    binaries: impl FnOnce() -> Vec<PathBuf>,
) -> Vec<EvidenceRecord> {
    if !config.hardening.enabled {
        return vec![record("suite", EvidenceStatus::Disabled, None, "binary hardening disabled")];
    }
    if !execute {
        return vec![record(
            "suite",
            EvidenceStatus::Unknown,
            None,
            "explicit hardening/release run required",
        )];
    }
    match build() {
        BuildOutcome::Success => inspect_artifacts(workspace, config, binaries()),
        BuildOutcome::Failed(detail) => {
            vec![record("release-build", EvidenceStatus::Failed, None, &detail)]
        }
        BuildOutcome::Unavailable(detail) => {
            vec![record("release-build", EvidenceStatus::Unavailable, None, &detail)]
        }
    }
}

fn inspect_artifacts(
    workspace: &Path,
    config: &QaConfig,
    binaries: Vec<PathBuf>,
) -> Vec<EvidenceRecord> {
    let binaries = binaries.into_iter().filter(|binary| binary.exists()).collect::<Vec<_>>();
    let mut output = vec![record(
        "release-build",
        EvidenceStatus::Available,
        None,
        "release workspace built with locked dependency graph",
    )];
    if binaries.is_empty() {
        output.push(record(
            "artifacts",
            EvidenceStatus::Unknown,
            None,
            "no release binary artifacts discovered from cargo metadata",
        ));
        return output;
    }
    for binary in binaries {
        output.push(path_disclosure(&binary, workspace, config));
        output.extend(platform_records(&binary, config));
    }
    output
}

fn platform_records(binary: &Path, config: &QaConfig) -> Vec<EvidenceRecord> {
    #[cfg(target_os = "linux")]
    {
        elf(binary, config)
    }
    #[cfg(target_os = "windows")]
    {
        pe(binary, config)
    }
    #[cfg(target_os = "macos")]
    {
        macho(binary, config)
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        let _ = (binary, config);
        Vec::new()
    }
}

fn path_disclosure(path: &Path, workspace: &Path, config: &QaConfig) -> EvidenceRecord {
    if !config.hardening.deny_host_paths {
        return record("path-disclosure", EvidenceStatus::Disabled, Some(path), "disabled");
    }
    match fs::read(path) {
        Ok(bytes) => {
            let text = String::from_utf8_lossy(&bytes);
            let bad = host_path_markers(workspace)
                .into_iter()
                .any(|marker| contains_path_marker(&text, &marker));
            record(
                "path-disclosure",
                if bad { EvidenceStatus::Failed } else { EvidenceStatus::Available },
                Some(path),
                if bad {
                    "workspace or current-user host path detected in production artifact"
                } else {
                    "no workspace or current-user host path detected"
                },
            )
        }
        Err(error) => {
            record("path-disclosure", EvidenceStatus::Unavailable, Some(path), &error.to_string())
        }
    }
}

fn host_path_markers(workspace: &Path) -> Vec<String> {
    unique_nonempty_markers(
        std::iter::once(workspace.display().to_string()).chain(
            ["CARGO_HOME", "RUSTUP_HOME", "USERPROFILE", "HOME"]
                .into_iter()
                .filter_map(env::var_os)
                .map(|value| PathBuf::from(value).display().to_string()),
        ),
    )
}

fn unique_nonempty_markers(values: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    values
        .into_iter()
        .filter(|value| !value.is_empty())
        .filter(|value| seen.insert(value.clone()))
        .collect()
}

fn contains_path_marker(text: &str, marker: &str) -> bool {
    if marker.is_empty() {
        return false;
    }
    text.contains(marker)
        || text.contains(&marker.replace('\\', "/"))
        || text.contains(&marker.replace('/', "\\"))
}

#[cfg(target_os = "linux")]
fn elf(path: &Path, config: &QaConfig) -> Vec<EvidenceRecord> {
    let args = vec!["-W".into(), "-h".into(), "-l".into(), "-d".into(), path.display().to_string()];
    match super::process::run(path.parent().unwrap_or(Path::new(".")), "readelf", &args, &[]) {
        Ok(output) if output.status.success() => {
            let text = String::from_utf8_lossy(&output.stdout);
            elf_records(path, config, &text)
        }
        Ok(output) => {
            vec![record("ELF", EvidenceStatus::Failed, Some(path), &stderr(&output.stderr))]
        }
        Err(error) => {
            vec![record("ELF", EvidenceStatus::Unavailable, Some(path), &error.to_string())]
        }
    }
}

#[cfg(target_os = "linux")]
fn elf_records(path: &Path, config: &QaConfig, text: &str) -> Vec<EvidenceRecord> {
    let pie = elf_pie(text);
    let relro = elf_full_relro(text);
    let exec_stack = elf_has_executable_stack(text);
    let rwx = elf_has_rwx_segment(text);
    vec![
        mitigation_record(
            "PIE",
            !config.hardening.require_pie || pie,
            path,
            pie,
            "PIE/DYN artifact",
            "PIE not recognized",
        ),
        mitigation_record(
            "full-relro",
            !config.hardening.require_full_relro || relro,
            path,
            relro,
            "GNU_RELRO + immediate binding found",
            "full RELRO not recognized",
        ),
        mitigation_record(
            "executable-stack",
            !config.hardening.deny_executable_stack || !exec_stack,
            path,
            exec_stack,
            "GNU_STACK is executable",
            "non-executable stack evidence",
        ),
        mitigation_record(
            "rwx-segments",
            !config.hardening.deny_rwx_segments || !rwx,
            path,
            rwx,
            "RWX LOAD segment detected",
            "no RWX LOAD segment recognized",
        ),
    ]
}

#[cfg(target_os = "linux")]
fn mitigation_record(
    name: &str,
    policy_passed: bool,
    path: &Path,
    detected: bool,
    detected_detail: &str,
    absent_detail: &str,
) -> EvidenceRecord {
    record(
        name,
        if policy_passed { EvidenceStatus::Available } else { EvidenceStatus::Failed },
        Some(path),
        if detected { detected_detail } else { absent_detail },
    )
}

#[cfg(target_os = "linux")]
fn elf_pie(text: &str) -> bool {
    text.contains("Type:                              DYN") || text.contains("Type: DYN")
}

#[cfg(target_os = "linux")]
fn elf_full_relro(text: &str) -> bool {
    let immediate_binding =
        text.contains("BIND_NOW") || (text.contains("FLAGS") && text.contains("NOW"));
    text.contains("GNU_RELRO") && immediate_binding
}

#[cfg(target_os = "linux")]
fn elf_has_executable_stack(text: &str) -> bool {
    text.lines().any(|line| line.contains("GNU_STACK") && line.contains("RWE"))
}

#[cfg(target_os = "linux")]
fn elf_has_rwx_segment(text: &str) -> bool {
    text.lines().any(|line| line.contains("LOAD") && line.contains("RWE"))
}

#[cfg(target_os = "windows")]
fn pe(path: &Path, _config: &QaConfig) -> Vec<EvidenceRecord> {
    let args = vec!["/headers".into(), path.display().to_string()];
    match super::process::run(path.parent().unwrap_or(Path::new(".")), "dumpbin", &args, &[]) {
        Ok(output) => pe_output(path, output),
        Err(_) => vec![pe_unknown(path)],
    }
}

#[cfg(target_os = "windows")]
fn pe_output(path: &Path, output: std::process::Output) -> Vec<EvidenceRecord> {
    if !output.status.success() {
        return vec![pe_unknown(path)];
    }
    let text = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
    vec![
        pe_mitigation(path, "ASLR", text.contains("dynamic base"), "PE DYNAMIC_BASE"),
        pe_mitigation(path, "DEP", text.contains("nx compatible"), "PE NX_COMPAT"),
    ]
}

#[cfg(target_os = "windows")]
fn pe_mitigation(path: &Path, name: &str, detected: bool, detail: &str) -> EvidenceRecord {
    let status = if detected { EvidenceStatus::Available } else { EvidenceStatus::Failed };
    record(name, status, Some(path), detail)
}

#[cfg(target_os = "windows")]
fn pe_unknown(path: &Path) -> EvidenceRecord {
    record(
        "PE",
        EvidenceStatus::Unknown,
        Some(path),
        "dumpbin unavailable; PE mitigation inspection could not complete",
    )
}

#[cfg(target_os = "macos")]
fn macho(path: &Path, _config: &QaConfig) -> Vec<EvidenceRecord> {
    let args = vec!["-hv".into(), path.display().to_string()];
    match super::process::run(path.parent().unwrap_or(Path::new(".")), "otool", &args, &[]) {
        Ok(output) if output.status.success() => {
            let text = String::from_utf8_lossy(&output.stdout);
            vec![record(
                "PIE",
                if text.contains("PIE") {
                    EvidenceStatus::Available
                } else {
                    EvidenceStatus::Failed
                },
                Some(path),
                "Mach-O PIE flag inspection",
            )]
        }
        _ => vec![record("Mach-O", EvidenceStatus::Unknown, Some(path), "otool unavailable")],
    }
}

fn stderr(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).chars().take(1000).collect()
}

fn record(name: &str, status: EvidenceStatus, path: Option<&Path>, detail: &str) -> EvidenceRecord {
    EvidenceRecord {
        family: "HARDEN".into(),
        check: name.into(),
        status,
        source: path.map(|path| path.display().to_string()),
        detail: Some(detail.into()),
    }
}

#[cfg(test)]
mod tests;
