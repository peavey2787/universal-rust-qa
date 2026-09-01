use qa_model::{EvidenceRecord, EvidenceStatus};
use qa_policy::QaConfig;
use std::{
    fs,
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

pub fn run(workspace: &Path, config: &QaConfig, execute: bool) -> Vec<EvidenceRecord> {
    let mut jobs: Vec<(String, Vec<String>)> = Vec::new();
    if config.platform.check_default {
        jobs.push(("default".into(), vec!["check".into(), "--workspace".into()]));
    }
    if config.platform.check_no_default {
        jobs.push((
            "no-default".into(),
            vec!["check".into(), "--workspace".into(), "--no-default-features".into()],
        ));
    }
    if config.platform.check_all_features {
        jobs.push((
            "all-features".into(),
            vec!["check".into(), "--workspace".into(), "--all-features".into()],
        ));
    }
    if config.platform.check_each_feature {
        jobs.push((
            "each-feature".into(),
            vec!["hack".into(), "check".into(), "--workspace".into(), "--each-feature".into()],
        ));
    }
    for target in &config.platform.targets {
        jobs.push((
            format!("target:{target}"),
            vec!["check".into(), "--workspace".into(), "--target".into(), target.clone()],
        ));
    }

    let mut records = jobs
        .into_iter()
        .map(|(name, args)| run_job(workspace, execute, &name, "cargo", &args))
        .collect::<Vec<_>>();
    if config.platform.check_msrv {
        records.extend(msrv_jobs(workspace, execute));
    }
    records
}

fn msrv_jobs(workspace: &Path, execute: bool) -> Vec<EvidenceRecord> {
    let mut output = Vec::new();
    for manifest in manifests(workspace) {
        let Ok(text) = fs::read_to_string(&manifest) else {
            continue;
        };
        let Ok(value) = toml::from_str::<toml::Value>(&text) else {
            continue;
        };
        let Some(package) = value.get("package") else {
            continue;
        };
        let name = package.get("name").and_then(toml::Value::as_str).unwrap_or("package");
        let Some(version) = package.get("rust-version").and_then(toml::Value::as_str) else {
            output.push(record(
                &format!("msrv:{name}"),
                EvidenceStatus::Unknown,
                Some(&manifest),
                "package.rust-version is not declared",
            ));
            continue;
        };
        let args = vec![
            format!("+{version}"),
            "check".into(),
            "--manifest-path".into(),
            manifest.display().to_string(),
        ];
        output.push(run_job_with_source(
            workspace,
            execute,
            &format!("msrv:{name}"),
            "cargo",
            &args,
            Some(&manifest),
        ));
    }
    output
}

fn manifests(root: &Path) -> Vec<PathBuf> {
    let mut output = WalkDir::new(root)
        .max_depth(5)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.file_type().is_file()
                && entry.file_name() == "Cargo.toml"
                && !entry.path().components().any(|component| {
                    matches!(component.as_os_str().to_str(), Some("target" | "vendor" | ".git"))
                })
        })
        .map(|entry| entry.path().to_path_buf())
        .collect::<Vec<_>>();
    output.sort();
    output
}

fn run_job(
    workspace: &Path,
    execute: bool,
    name: &str,
    program: &str,
    args: &[String],
) -> EvidenceRecord {
    run_job_with_source(workspace, execute, name, program, args, None)
}

fn run_job_with_source(
    workspace: &Path,
    execute: bool,
    name: &str,
    program: &str,
    args: &[String],
    source: Option<&Path>,
) -> EvidenceRecord {
    if !execute {
        return record(name, EvidenceStatus::Unknown, source, "explicit platform run required");
    }
    match super::process::run(workspace, program, args, &[]) {
        Ok(output) => record(
            name,
            if output.status.success() {
                EvidenceStatus::Available
            } else {
                EvidenceStatus::Failed
            },
            source,
            &String::from_utf8_lossy(&output.stderr).chars().take(1000).collect::<String>(),
        ),
        Err(error) => record(name, EvidenceStatus::Unavailable, source, &error.to_string()),
    }
}

fn record(
    check: &str,
    status: EvidenceStatus,
    source: Option<&Path>,
    detail: &str,
) -> EvidenceRecord {
    EvidenceRecord {
        family: "CFG".into(),
        check: check.into(),
        status,
        source: source.map(|path| path.display().to_string()),
        detail: Some(detail.into()),
    }
}

#[cfg(test)]
mod tests;
