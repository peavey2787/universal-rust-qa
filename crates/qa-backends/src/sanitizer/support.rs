use qa_policy::QaConfig;
use std::path::Path;

#[cfg(windows)]
use std::{env, path::PathBuf};

pub(super) fn sanitizer_envs(
    target: &str,
    sanitizer: &str,
    flag: &str,
) -> Result<Vec<(&'static str, String)>, String> {
    let mut envs = vec![("RUSTFLAGS", flag.to_string()), ("RUSTDOCFLAGS", flag.to_string())];
    append_runtime_env(&mut envs, target, sanitizer)?;
    Ok(envs)
}

#[cfg(windows)]
pub(super) fn append_runtime_env(
    envs: &mut Vec<(&'static str, String)>,
    target: &str,
    sanitizer: &str,
) -> Result<(), String> {
    if !needs_windows_asan_runtime(target, sanitizer) {
        return Ok(());
    }

    let runtime_dir = windows_asan_runtime_dir().ok_or_else(|| {
        concat!(
            "Windows ASan runtime clang_rt.asan_dynamic-x86_64.dll is unavailable. ",
            "Run scripts/install-qa-tools.ps1 (or the full Windows runner) so the ",
            "Visual Studio C++ AddressSanitizer runtime is provisioned and exported."
        )
        .to_string()
    })?;
    let current_path = env::var_os("PATH").unwrap_or_default();
    let mut paths = vec![runtime_dir];
    paths.extend(env::split_paths(&current_path));
    let joined = env::join_paths(paths)
        .map_err(|error| format!("could not construct Windows ASan PATH: {error}"))?;
    envs.push(("PATH", joined.to_string_lossy().into_owned()));
    Ok(())
}

#[cfg(windows)]
pub(super) fn needs_windows_asan_runtime(target: &str, sanitizer: &str) -> bool {
    sanitizer == "address" && target == "x86_64-pc-windows-msvc"
}

#[cfg(not(windows))]
pub(super) fn append_runtime_env(
    _envs: &mut Vec<(&'static str, String)>,
    _target: &str,
    _sanitizer: &str,
) -> Result<(), String> {
    Ok(())
}

#[cfg(windows)]
pub(super) fn windows_asan_runtime_dir() -> Option<PathBuf> {
    const DLL: &str = "clang_rt.asan_dynamic-x86_64.dll";

    if let Some(path) = env::var_os("QA_ASAN_RUNTIME_DIR").map(PathBuf::from) {
        if path.join(DLL).is_file() {
            return Some(path);
        }
    }

    let path = env::var_os("PATH")?;
    env::split_paths(&path).find(|directory| directory.join(DLL).is_file())
}

pub(super) fn sanitizer_args(config: &QaConfig, target: &str) -> Vec<String> {
    vec![
        format!("+{}", config.sanitizers.toolchain),
        "test".into(),
        "-Zbuild-std".into(),
        "--workspace".into(),
        "--target".into(),
        target.to_string(),
    ]
}

pub(super) fn sanitizer_flag(sanitizer: &str) -> String {
    if sanitizer == "memory" {
        "-Zsanitizer=memory -Zsanitizer-memory-track-origins".into()
    } else {
        format!("-Zsanitizer={sanitizer}")
    }
}

pub(super) fn supported(kind: &str, target: &str) -> bool {
    match kind {
        "address" => matches!(
            target,
            "aarch64-apple-darwin"
                | "aarch64-unknown-fuchsia"
                | "aarch64-unknown-linux-gnu"
                | "x86_64-apple-darwin"
                | "x86_64-unknown-fuchsia"
                | "x86_64-unknown-freebsd"
                | "x86_64-unknown-linux-gnu"
                | "x86_64-pc-windows-msvc"
        ),
        "leak" => matches!(
            target,
            "aarch64-unknown-linux-gnu" | "x86_64-apple-darwin" | "x86_64-unknown-linux-gnu"
        ),
        "memory" => matches!(
            target,
            "aarch64-unknown-linux-gnu" | "x86_64-unknown-freebsd" | "x86_64-unknown-linux-gnu"
        ),
        "thread" => matches!(
            target,
            "aarch64-apple-darwin"
                | "aarch64-unknown-linux-gnu"
                | "x86_64-apple-darwin"
                | "x86_64-unknown-freebsd"
                | "x86_64-unknown-linux-gnu"
        ),
        // RealtimeSanitizer target support is intentionally discovered through
        // execution because its target matrix is still evolving.
        "realtime" => true,
        _ => false,
    }
}

pub(super) fn discover_host(workspace: &Path, toolchain: &str) -> Option<String> {
    let args = vec![format!("+{toolchain}"), "-vV".into()];
    let output = super::super::process::run(workspace, "rustc", &args, &[]).ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| line.strip_prefix("host: ").map(str::to_string))
}
