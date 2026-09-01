use std::{
    env,
    path::{Path, PathBuf},
};

pub(super) fn project_state_dir(base: &Path, workspace: &Path) -> PathBuf {
    base.join("projects").join(format!("{:016x}", fnv1a_path(workspace)))
}

pub(super) fn fnv1a_path(path: &Path) -> u64 {
    let normalized = path.to_string_lossy().replace('\\', "/");
    let normalized = if cfg!(windows) { normalized.to_ascii_lowercase() } else { normalized };
    let mut hash = 0xcbf29ce484222325u64;
    for byte in normalized.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

pub(super) fn default_state_home() -> Result<PathBuf, String> {
    #[cfg(windows)]
    {
        windows_default_state_home()
    }
    #[cfg(target_os = "macos")]
    {
        macos_default_state_home()
    }
    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        unix_default_state_home()
    }
}

#[cfg(windows)]
pub(super) fn windows_default_state_home() -> Result<PathBuf, String> {
    env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .map(|path| path.join("UniversalRustQA"))
        .ok_or_else(|| {
            "LOCALAPPDATA is unavailable; set UNIVERSAL_QA_STATE_HOME or --state-dir".into()
        })
}

#[cfg(target_os = "macos")]
pub(super) fn macos_default_state_home() -> Result<PathBuf, String> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .map(|path| path.join("Library/Application Support/UniversalRustQA"))
        .ok_or_else(|| "HOME is unavailable; set UNIVERSAL_QA_STATE_HOME or --state-dir".into())
}

#[cfg(all(not(windows), not(target_os = "macos")))]
pub(super) fn unix_default_state_home() -> Result<PathBuf, String> {
    if let Some(path) = env::var_os("XDG_STATE_HOME").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(path).join("universal-rust-qa"));
    }
    env::var_os("HOME")
        .map(PathBuf::from)
        .map(|path| path.join(".local/state/universal-rust-qa"))
        .ok_or_else(|| "HOME is unavailable; set UNIVERSAL_QA_STATE_HOME or --state-dir".into())
}
