use std::path::Path;

pub(super) fn ensure_llvm_cov(workspace: &Path) -> Result<(), String> {
    match probe(workspace) {
        Ok(()) => return Ok(()),
        Err(error) if !missing_tooling(&error) => {
            return Err(format!("cargo-llvm-cov preflight failed: {error}"));
        }
        Err(_) => {}
    }

    install(workspace)?;
    probe(workspace).map_err(|error| {
        format!("cargo-llvm-cov is still unavailable after automatic installation: {error}")
    })
}

fn probe(workspace: &Path) -> Result<(), String> {
    let args = vec!["llvm-cov".into(), "--version".into()];
    let output = super::super::process::run(workspace, "cargo", &args, &[])
        .map_err(|error| error.to_string())?;
    if output.status.success() {
        return Ok(());
    }
    Err(super::super::process::diagnostics(&output.stdout, &output.stderr))
}

fn install(workspace: &Path) -> Result<(), String> {
    let stable_args =
        vec!["+stable".into(), "install".into(), "--locked".into(), "cargo-llvm-cov".into()];
    if command_succeeds(workspace, "cargo", &stable_args) {
        return Ok(());
    }

    let rustup_args = vec![
        "toolchain".into(),
        "install".into(),
        "stable".into(),
        "--profile".into(),
        "minimal".into(),
    ];
    if command_succeeds(workspace, "rustup", &rustup_args)
        && command_succeeds(workspace, "cargo", &stable_args)
    {
        return Ok(());
    }

    let fallback_args = vec!["install".into(), "--locked".into(), "cargo-llvm-cov".into()];
    let output = super::super::process::run(workspace, "cargo", &fallback_args, &[])
        .map_err(|error| format!("automatic cargo-llvm-cov installation failed: {error}"))?;
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "automatic cargo-llvm-cov installation failed: {}",
        super::super::process::diagnostics(&output.stdout, &output.stderr)
    ))
}

fn command_succeeds(workspace: &Path, program: &str, args: &[String]) -> bool {
    super::super::process::run(workspace, program, args, &[])
        .is_ok_and(|output| output.status.success())
}

fn missing_tooling(diagnostic: &str) -> bool {
    super::execute::classify_failure(diagnostic) == "tooling"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_missing_coverage_tooling_triggers_installation() {
        assert!(missing_tooling("error: no such command: `llvm-cov`"));
        assert!(missing_tooling("cargo-llvm-cov not found"));
        assert!(!missing_tooling(
            "error: failed to compile package because a symbol is unresolved"
        ));
    }
}
