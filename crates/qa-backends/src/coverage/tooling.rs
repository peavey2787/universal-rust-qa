use std::path::Path;

#[cfg(any(windows, test))]
use std::path::PathBuf;
#[cfg(windows)]
use std::sync::OnceLock;

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

#[cfg(any(windows, test))]
const WINDOWS_LLVM_COMPONENT: &str = "Microsoft.VisualStudio.Component.VC.Llvm.Clang";
#[cfg(windows)]
const WINDOWS_MSVC_COMPONENT: &str = "Microsoft.VisualStudio.Component.VC.Tools.x86.x64";

#[cfg(windows)]
static HOST_LIBCLANG_DIR: OnceLock<Option<String>> = OnceLock::new();

#[cfg(windows)]
pub(super) fn ensure_host_libclang_dir(workspace: &Path) -> Option<String> {
    HOST_LIBCLANG_DIR.get_or_init(|| discover_or_provision_host_libclang(workspace)).clone()
}

#[cfg(not(windows))]
pub(super) fn ensure_host_libclang_dir(_workspace: &Path) -> Option<String> {
    None
}

#[cfg(windows)]
fn discover_or_provision_host_libclang(workspace: &Path) -> Option<String> {
    if let Some(path) = discover_host_libclang(workspace) {
        return Some(path.display().to_string());
    }

    let (vswhere, setup) = visual_studio_installer_tools()?;
    let install = query_visual_studio_install(workspace, &vswhere, WINDOWS_MSVC_COMPONENT)?;
    eprintln!("coverage: provisioning Visual Studio host LLVM/Clang for native bindgen recovery");
    if !install_visual_studio_component(workspace, &setup, &install, WINDOWS_LLVM_COMPONENT) {
        return None;
    }

    let installed =
        query_visual_studio_install(workspace, &vswhere, WINDOWS_LLVM_COMPONENT).unwrap_or(install);
    find_visual_studio_libclang(&installed)
        .or_else(discover_standard_llvm_libclang)
        .map(|path| path.display().to_string())
}

#[cfg(windows)]
fn discover_host_libclang(workspace: &Path) -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("QA_HOST_LIBCLANG_PATH") {
        let path = PathBuf::from(path);
        if let Some(path) = valid_libclang_dir(&path) {
            return Some(path);
        }
    }
    if let Some(path) = discover_standard_llvm_libclang() {
        return Some(path);
    }
    let (vswhere, _) = visual_studio_installer_tools()?;
    let install = query_visual_studio_install(workspace, &vswhere, WINDOWS_LLVM_COMPONENT)?;
    find_visual_studio_libclang(&install)
}

#[cfg(windows)]
fn discover_standard_llvm_libclang() -> Option<PathBuf> {
    ["ProgramW6432", "ProgramFiles"]
        .into_iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from)
        .map(|root| root.join("LLVM").join("bin"))
        .find_map(|path| valid_libclang_dir(&path))
}

#[cfg(windows)]
fn visual_studio_installer_tools() -> Option<(PathBuf, PathBuf)> {
    let root = std::env::var_os("ProgramFiles(x86)").map(PathBuf::from)?;
    let installer = root.join("Microsoft Visual Studio").join("Installer");
    let vswhere = installer.join("vswhere.exe");
    let setup = installer.join("setup.exe");
    (vswhere.is_file() && setup.is_file()).then_some((vswhere, setup))
}

#[cfg(windows)]
fn query_visual_studio_install(
    workspace: &Path,
    vswhere: &Path,
    component: &str,
) -> Option<PathBuf> {
    let args = vec![
        "-latest".into(),
        "-products".into(),
        "*".into(),
        "-requires".into(),
        component.into(),
        "-property".into(),
        "installationPath".into(),
    ];
    let program = vswhere.display().to_string();
    let output = super::super::process::run(workspace, &program, &args, &[]).ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(PathBuf::from)
}

#[cfg(windows)]
fn install_visual_studio_component(
    workspace: &Path,
    setup: &Path,
    install: &Path,
    component: &str,
) -> bool {
    let args = visual_studio_modify_args(&install.display().to_string(), component);
    let program = setup.display().to_string();
    super::super::process::run(workspace, &program, &args, &[])
        .is_ok_and(|output| output.status.success())
}

#[cfg(any(windows, test))]
fn visual_studio_modify_args(install: &str, component: &str) -> Vec<String> {
    vec![
        "modify".into(),
        "--installPath".into(),
        install.into(),
        "--add".into(),
        component.into(),
        "--passive".into(),
        "--norestart".into(),
    ]
}

#[cfg(windows)]
fn find_visual_studio_libclang(install: &Path) -> Option<PathBuf> {
    visual_studio_llvm_dirs(install).into_iter().find_map(|path| valid_libclang_dir(&path))
}

#[cfg(any(windows, test))]
fn visual_studio_llvm_dirs(install: &Path) -> Vec<PathBuf> {
    let root = install.join("VC").join("Tools").join("Llvm");
    vec![root.join("x64").join("bin"), root.join("bin")]
}

#[cfg(windows)]
fn valid_libclang_dir(path: &Path) -> Option<PathBuf> {
    if path.is_file() {
        let name = path.file_name()?.to_string_lossy();
        if name.eq_ignore_ascii_case("libclang.dll") || name.eq_ignore_ascii_case("clang.dll") {
            return path.parent().map(Path::to_path_buf);
        }
        return None;
    }
    ["libclang.dll", "clang.dll"]
        .into_iter()
        .any(|name| path.join(name).is_file())
        .then(|| path.to_path_buf())
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

    #[test]
    fn windows_host_llvm_provisioning_uses_the_official_visual_studio_component() {
        let args = visual_studio_modify_args(
            r"C:\Program Files\Microsoft Visual Studio",
            WINDOWS_LLVM_COMPONENT,
        );
        assert_eq!(args[0], "modify");
        assert!(args.windows(2).any(|pair| {
            pair[0] == "--add" && pair[1] == "Microsoft.VisualStudio.Component.VC.Llvm.Clang"
        }));
        assert!(args.iter().any(|arg| arg == "--passive"));
        assert!(args.iter().any(|arg| arg == "--norestart"));
    }

    #[test]
    fn visual_studio_host_llvm_prefers_the_x64_bin_directory() {
        let dirs = visual_studio_llvm_dirs(Path::new(r"C:\VS"));
        assert_eq!(dirs.len(), 2);
        assert_eq!(
            dirs[0],
            Path::new(r"C:\VS").join("VC").join("Tools").join("Llvm").join("x64").join("bin")
        );
        assert_eq!(dirs[1], Path::new(r"C:\VS").join("VC").join("Tools").join("Llvm").join("bin"));
    }
}
