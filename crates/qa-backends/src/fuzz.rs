use qa_model::EvidenceStatus;
use qa_policy::QaConfig;
use std::{collections::BTreeMap, path::Path};

pub struct FuzzBackend {
    pub targets: BTreeMap<String, EvidenceStatus>,
    pub errors: BTreeMap<String, String>,
}

pub fn check(workspace: &Path, _config: &QaConfig, names: &[String], run: bool) -> FuzzBackend {
    check_with(names, run, |name| {
        let args = vec!["fuzz".into(), "build".into(), name.to_string()];
        super::process::run(workspace, "cargo", &args, &[])
            .map(|result| result.status.success())
            .map_err(|error| error.to_string())
    })
}

fn check_with(
    names: &[String],
    run: bool,
    mut build: impl FnMut(&str) -> Result<bool, String>,
) -> FuzzBackend {
    let mut output = FuzzBackend { targets: BTreeMap::new(), errors: BTreeMap::new() };
    for name in names {
        if !run {
            output.targets.insert(name.clone(), EvidenceStatus::Unknown);
            continue;
        }
        match build(name) {
            Ok(true) => {
                output.targets.insert(name.clone(), EvidenceStatus::Available);
            }
            Ok(false) => {
                output.targets.insert(name.clone(), EvidenceStatus::Failed);
            }
            Err(error) => {
                output.targets.insert(name.clone(), EvidenceStatus::Unavailable);
                output.errors.insert(name.clone(), error);
            }
        }
    }
    output
}

#[cfg(test)]
mod tests;
