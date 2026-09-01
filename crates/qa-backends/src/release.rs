use qa_model::{EvidenceRecord, EvidenceStatus};
use qa_policy::{GeneratorTarget, QaConfig};
use std::{fs, path::Path};
use walkdir::WalkDir;

pub fn run(
    workspace: &Path,
    config: &QaConfig,
    artifact_root: &Path,
    execute: bool,
) -> Vec<EvidenceRecord> {
    let mut evidence = Vec::new();
    evidence.extend(docs(workspace, config, execute));
    evidence.extend(snapshots(workspace, config, execute));
    evidence.extend(dependencies(workspace, config, execute));
    evidence.extend(api(workspace, config, execute));
    evidence.extend(generated(workspace, config, execute));
    evidence.extend(repro(workspace, config, artifact_root, execute));
    evidence
}

fn docs(workspace: &Path, config: &QaConfig, execute: bool) -> Vec<EvidenceRecord> {
    if !execute {
        return vec![record(
            "DOC",
            "doctests",
            EvidenceStatus::Unknown,
            None,
            "explicit release run required",
        )];
    }

    let mut evidence = Vec::new();
    if config.documentation.run_doctests {
        evidence.push(job(
            workspace,
            "DOC",
            "doctests",
            "cargo",
            &["test", "--workspace", "--doc"],
        ));
    }
    if config.documentation.check_examples {
        evidence.push(job(
            workspace,
            "DOC",
            "examples",
            "cargo",
            &["check", "--workspace", "--examples"],
        ));
    }
    evidence
}

fn snapshots(workspace: &Path, config: &QaConfig, execute: bool) -> Vec<EvidenceRecord> {
    let snapshots = WalkDir::new(workspace)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.file_type().is_file()
                && entry.path().extension().and_then(|ext| ext.to_str()) == Some("snap")
                && !excluded(entry.path())
        })
        .count();

    if snapshots == 0 {
        return vec![record(
            "SNAP",
            "suite",
            EvidenceStatus::NotApplicable,
            None,
            "no .snap files discovered",
        )];
    }
    if !execute {
        return vec![record(
            "SNAP",
            "suite",
            EvidenceStatus::Unknown,
            None,
            "explicit release run required",
        )];
    }
    if config.snapshots.unreferenced.eq_ignore_ascii_case("allow") {
        return vec![record(
            "SNAP",
            "unreferenced",
            EvidenceStatus::Disabled,
            None,
            "unreferenced snapshot rejection disabled",
        )];
    }

    vec![job(
        workspace,
        "SNAP",
        "unreferenced",
        "cargo",
        &["insta", "test", "--workspace", "--unreferenced=reject"],
    )]
}

fn dependencies(workspace: &Path, config: &QaConfig, execute: bool) -> Vec<EvidenceRecord> {
    if !execute {
        return vec![record(
            "DEP",
            "suite",
            EvidenceStatus::Unknown,
            None,
            "explicit release run required",
        )];
    }

    let mut evidence = Vec::new();
    if config.dependencies.run_cargo_deny {
        evidence.push(job(workspace, "DEP", "cargo-deny", "cargo", &["deny", "check"]));
    }
    if config.dependencies.run_unused {
        evidence.push(job(workspace, "DEP", "unused-dependencies", "cargo-machete", &[]));
    }
    evidence
}

fn api(workspace: &Path, config: &QaConfig, execute: bool) -> Vec<EvidenceRecord> {
    if config.api.run_semver_checks {
        return api_enabled(workspace, config, execute);
    }
    vec![record(
        "API",
        "semver",
        EvidenceStatus::Disabled,
        None,
        "semver checks disabled or no release baseline requested",
    )]
}

fn api_enabled(workspace: &Path, config: &QaConfig, execute: bool) -> Vec<EvidenceRecord> {
    if !execute {
        return vec![record(
            "API",
            "semver",
            EvidenceStatus::Unknown,
            None,
            "explicit release run required",
        )];
    }
    let args = api_args(config);
    match super::process::run(workspace, "cargo", &args, &[]) {
        Ok(output) => vec![api_output_record(output)],
        Err(error) => {
            vec![record("API", "semver", EvidenceStatus::Unavailable, None, &error.to_string())]
        }
    }
}

fn api_args(config: &QaConfig) -> Vec<String> {
    let mut args = vec!["semver-checks".to_string()];
    if let Some(baseline) = &config.api.baseline {
        args.extend(["--baseline-rev".into(), baseline.clone()]);
    }
    args
}

fn api_output_record(output: std::process::Output) -> EvidenceRecord {
    let status =
        if output.status.success() { EvidenceStatus::Available } else { EvidenceStatus::Failed };
    record(
        "API",
        "semver",
        status,
        None,
        &String::from_utf8_lossy(&output.stderr).chars().take(1000).collect::<String>(),
    )
}

fn generated(workspace: &Path, config: &QaConfig, execute: bool) -> Vec<EvidenceRecord> {
    if !config.generated.verify {
        return vec![record(
            "GEN",
            "suite",
            EvidenceStatus::Disabled,
            None,
            "generated-output verification disabled",
        )];
    }
    if config.generated.target.is_empty() {
        return vec![record(
            "GEN",
            "suite",
            EvidenceStatus::NotApplicable,
            None,
            "no generators configured",
        )];
    }
    if !execute {
        return vec![record(
            "GEN",
            "suite",
            EvidenceStatus::Unknown,
            None,
            "explicit release run required",
        )];
    }

    config.generated.target.iter().flat_map(|target| verify_generator(workspace, target)).collect()
}

fn verify_generator(workspace: &Path, target: &GeneratorTarget) -> Vec<EvidenceRecord> {
    let paths = target.outputs.iter().map(|path| workspace.join(path)).collect::<Vec<_>>();
    let original = paths.iter().map(|path| fs::read(path).ok()).collect::<Vec<_>>();

    let first = run_generator_once(workspace, target, &paths);
    if let Err(error) = restore(&paths, &original) {
        return vec![record(
            "GEN",
            &format!("{}:restore", target.name),
            EvidenceStatus::Failed,
            None,
            &error,
        )];
    }
    let second = run_generator_once(workspace, target, &paths);
    if let Err(error) = restore(&paths, &original) {
        return vec![record(
            "GEN",
            &format!("{}:restore", target.name),
            EvidenceStatus::Failed,
            None,
            &error,
        )];
    }

    let mut evidence = Vec::new();
    match (&first, &second) {
        (Ok(first_output), Ok(second_output)) => {
            let checked_in_matches = first_output == &original;
            let deterministic = first_output == second_output;

            evidence.push(record(
                "GEN",
                &format!("{}:drift", target.name),
                if checked_in_matches { EvidenceStatus::Available } else { EvidenceStatus::Failed },
                None,
                if checked_in_matches {
                    "generator reproduces checked-in outputs"
                } else {
                    "generated output differs from checked-in output"
                },
            ));
            evidence.push(record(
                "GEN",
                &format!("{}:determinism", target.name),
                if deterministic { EvidenceStatus::Available } else { EvidenceStatus::Failed },
                None,
                if deterministic {
                    "two isolated generator executions produced identical configured outputs"
                } else {
                    "generator outputs differ across identical executions"
                },
            ));
        }
        (Err(error), _) | (_, Err(error)) => {
            evidence.push(record("GEN", &target.name, EvidenceStatus::Failed, None, error))
        }
    }

    evidence
}

fn run_generator_once(
    workspace: &Path,
    target: &GeneratorTarget,
    paths: &[std::path::PathBuf],
) -> Result<Vec<Option<Vec<u8>>>, String> {
    match super::process::run_shell(workspace, &target.command, &[]) {
        Ok(output) if output.status.success() => {
            Ok(paths.iter().map(|path| fs::read(path).ok()).collect())
        }
        Ok(output) => Err(format!(
            "generator failed: {}",
            String::from_utf8_lossy(&output.stderr).chars().take(800).collect::<String>()
        )),
        Err(error) => Err(error.to_string()),
    }
}

fn restore(paths: &[std::path::PathBuf], original: &[Option<Vec<u8>>]) -> Result<(), String> {
    for (path, old) in paths.iter().zip(original) {
        match old {
            Some(bytes) => fs::write(path, bytes).map_err(|error| {
                format!("could not restore generated output {}: {error}", path.display())
            })?,
            None if path.exists() => fs::remove_file(path).map_err(|error| {
                format!("could not remove generated output {}: {error}", path.display())
            })?,
            None => {}
        }
    }
    Ok(())
}

mod repro;
use repro::*;

fn job(workspace: &Path, family: &str, name: &str, program: &str, args: &[&str]) -> EvidenceRecord {
    let args = args.iter().map(|arg| arg.to_string()).collect::<Vec<_>>();
    match super::process::run(workspace, program, &args, &[]) {
        Ok(output) => {
            let detail = super::process::diagnostics(&output.stdout, &output.stderr);
            let status = classify_job_status(output.status.success(), &detail);
            record(family, name, status, None, &detail)
        }
        Err(error) => record(family, name, EvidenceStatus::Unavailable, None, &error.to_string()),
    }
}

fn classify_job_status(success: bool, detail: &str) -> EvidenceStatus {
    if success {
        return EvidenceStatus::Available;
    }
    let lower = detail.to_ascii_lowercase();
    if ["no such command", "not recognized", "could not execute process"]
        .iter()
        .any(|needle| lower.contains(needle))
    {
        EvidenceStatus::Unavailable
    } else {
        EvidenceStatus::Failed
    }
}

fn record(
    family: &str,
    name: &str,
    status: EvidenceStatus,
    source: Option<&Path>,
    detail: &str,
) -> EvidenceRecord {
    EvidenceRecord {
        family: family.into(),
        check: name.into(),
        status,
        source: source.map(|path| path.display().to_string()),
        detail: Some(detail.into()),
    }
}

fn excluded(path: &Path) -> bool {
    path.components().any(|component| {
        matches!(component.as_os_str().to_str(), Some("target" | "qa-out" | "vendor" | ".git"))
    })
}

#[cfg(test)]
mod tests;
