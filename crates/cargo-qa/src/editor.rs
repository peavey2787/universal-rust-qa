use qa_policy::ViewerConfig;
use std::{
    path::{Path, PathBuf},
    process::Command,
};

pub fn open(
    viewer: &ViewerConfig,
    path: &Path,
    line: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let resolved = resolved_path(path);
    let args = viewer_args(viewer, &resolved, line);
    let mut cmd = Command::new(&viewer.command);
    if args.is_empty() {
        cmd.arg(&resolved);
    } else {
        cmd.args(args);
    }
    cmd.spawn()?;
    Ok(())
}

fn resolved_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn viewer_args(viewer: &ViewerConfig, path: &Path, line: usize) -> Vec<String> {
    let path_text = path.to_string_lossy();
    viewer
        .args
        .iter()
        .map(|arg| arg.replace("{path}", &path_text).replace("{line}", &line.to_string()))
        .collect()
}

#[cfg(test)]
mod tests;
