use std::path::Path;

pub(super) fn ensure_llvm_cov(workspace: &Path) -> Result<(), String> {
    let first_error = match probe(workspace) {
        Ok(()) => return Ok(()),
        Err(error) => error,
    };

    install(workspace).map_err(|install_error| {
        format!(
            "cargo-llvm-cov preflight failed: {first_error}; automatic installation failed: \
             {install_error}"
        )
    })?;
    probe(workspace).map_err(|error| {
        format!(
            "cargo-llvm-cov is still unavailable after automatic installation: {error}; \
             initial preflight: {first_error}"
        )
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
    let stable_args = stable_install_args();
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
    let compatible_args = compatible_install_args();
    let compatible = super::super::process::run(workspace, "cargo", &compatible_args, &[])
        .map_err(|error| format!("compatible cargo-llvm-cov installation failed: {error}"))?;
    if compatible.status.success() {
        return Ok(());
    }
    Err(format!(
        "automatic cargo-llvm-cov installation failed; latest: {}; compatible 0.6.21: {}",
        super::super::process::diagnostics(&output.stdout, &output.stderr),
        super::super::process::diagnostics(&compatible.stdout, &compatible.stderr)
    ))
}

fn stable_install_args() -> Vec<String> {
    vec!["+stable".into(), "install".into(), "--locked".into(), "cargo-llvm-cov".into()]
}

fn compatible_install_args() -> Vec<String> {
    vec![
        "install".into(),
        "--locked".into(),
        "--version".into(),
        "0.6.21".into(),
        "cargo-llvm-cov".into(),
    ]
}

fn command_succeeds(workspace: &Path, program: &str, args: &[String]) -> bool {
    super::super::process::run(workspace, program, args, &[])
        .is_ok_and(|output| output.status.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_install_command_matches_upstream_installation_contract() {
        assert_eq!(stable_install_args(), vec!["+stable", "install", "--locked", "cargo-llvm-cov"]);
    }

    #[test]
    fn pinned_fallback_supports_the_workspace_msrv_toolchain() {
        assert_eq!(
            compatible_install_args(),
            vec!["install", "--locked", "--version", "0.6.21", "cargo-llvm-cov"]
        );
    }
}
