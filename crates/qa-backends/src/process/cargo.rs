use std::{io, path::Path, process::Command};

pub(super) fn workspace_command(workspace: &Path, args: &[String]) -> io::Result<Command> {
    if let Some((toolchain, cargo_args)) = explicit_toolchain(args) {
        return Ok(rustup_cargo(workspace, toolchain, cargo_args));
    }
    match active_toolchain(workspace)? {
        Some(toolchain) => Ok(rustup_cargo(workspace, &toolchain, args)),
        None => {
            let mut command = Command::new("cargo");
            command.current_dir(workspace).args(args);
            Ok(command)
        }
    }
}

fn explicit_toolchain(args: &[String]) -> Option<(&str, &[String])> {
    let first = args.first()?;
    let toolchain = first.strip_prefix('+').filter(|value| !value.is_empty())?;
    Some((toolchain, &args[1..]))
}

fn active_toolchain(workspace: &Path) -> io::Result<Option<String>> {
    let result = Command::new("rustup")
        .current_dir(workspace)
        .env_remove("RUSTUP_TOOLCHAIN")
        .args(["show", "active-toolchain"])
        .output();
    classify_active_toolchain_result(result)
}

fn classify_active_toolchain_result(
    result: io::Result<std::process::Output>,
) -> io::Result<Option<String>> {
    let output = match result {
        Ok(output) => output,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "rustup could not resolve the inspected workspace toolchain: {}",
            compact(&output.stderr)
        )));
    }
    let Some(toolchain) = active_toolchain_name(&output.stdout) else {
        return Err(io::Error::other(
            "rustup returned an empty active toolchain for the inspected workspace",
        ));
    };
    Ok(Some(toolchain))
}

fn rustup_cargo(workspace: &Path, toolchain: &str, args: &[String]) -> Command {
    let mut command = Command::new("rustup");
    command
        .current_dir(workspace)
        .env_remove("RUSTUP_TOOLCHAIN")
        .args(["run", toolchain, "cargo"])
        .args(args);
    command
}

fn active_toolchain_name(stdout: &[u8]) -> Option<String> {
    String::from_utf8_lossy(stdout).split_whitespace().next().map(str::to_string)
}

fn compact(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).trim().chars().take(800).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_toolchain_is_removed_before_real_cargo_arguments() {
        let args = vec!["+1.95.0".into(), "check".into(), "--workspace".into()];
        let (toolchain, cargo_args) = explicit_toolchain(&args).unwrap();
        assert_eq!(toolchain, "1.95.0");
        assert_eq!(
            cargo_args.iter().map(String::as_str).collect::<Vec<_>>(),
            vec!["check", "--workspace"]
        );
        assert!(explicit_toolchain(&["check".into()]).is_none());
        assert!(explicit_toolchain(&["+".into(), "check".into()]).is_none());
    }

    #[test]
    fn rustup_cargo_command_pins_the_resolved_workspace_toolchain() {
        let command = rustup_cargo(Path::new("workspace"), "1.95.0", &["check".into()]);
        assert_eq!(command.get_program(), "rustup");
        assert_eq!(
            command.get_args().map(|arg| arg.to_str().unwrap()).collect::<Vec<_>>(),
            vec!["run", "1.95.0", "cargo", "check"]
        );
        assert!(command.get_envs().any(|(key, value)| {
            key == std::ffi::OsStr::new("RUSTUP_TOOLCHAIN") && value.is_none()
        }));
    }

    #[test]
    fn rustup_active_toolchain_output_uses_only_the_canonical_name() {
        assert_eq!(
            active_toolchain_name(
                b"1.95.0-x86_64-pc-windows-msvc (overridden by 'rust-toolchain.toml')\n"
            ),
            Some("1.95.0-x86_64-pc-windows-msvc".into())
        );
        assert_eq!(
            active_toolchain_name(b"stable-x86_64-unknown-linux-gnu\n"),
            Some("stable-x86_64-unknown-linux-gnu".into())
        );
        assert_eq!(active_toolchain_name(b" \r\n"), None);
    }

    #[test]
    fn active_toolchain_and_error_classifier_do_not_silently_fall_back() {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"));
        if Command::new("rustup").arg("--version").output().is_ok() {
            assert!(active_toolchain(workspace).unwrap().is_some());
        } else {
            assert_eq!(active_toolchain(workspace).unwrap(), None);
        }

        let missing = classify_active_toolchain_result(Err(io::Error::new(
            io::ErrorKind::NotFound,
            "rustup missing",
        )))
        .unwrap();
        assert_eq!(missing, None);

        let denied = classify_active_toolchain_result(Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "blocked",
        )));
        assert_eq!(denied.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(compact(b"  useful diagnostic  \n"), "useful diagnostic");
        assert_eq!(compact(&[b'x'; 900]).len(), 800);
    }
}
