use qa_policy::QaConfig;
use qa_sdk::QaRunLayout;
use std::{
    env, fs,
    path::{Path, PathBuf},
};

const STATE_HOME_ENV: &str = "UNIVERSAL_QA_STATE_HOME";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PathOptions {
    pub project_dir: Option<PathBuf>,
    pub output_dir: Option<PathBuf>,
    pub state_dir: Option<PathBuf>,
}

impl PathOptions {
    fn external_requested(&self) -> bool {
        self.project_dir.is_some() || self.output_dir.is_some() || self.state_dir.is_some()
    }
}

pub fn take_path_options(args: &mut Vec<String>, cwd: &Path) -> Result<PathOptions, String> {
    let project_dir = take_path_option(args, &["--project-dir", "--project"], cwd)?;
    let output_dir = take_path_option(args, &["--output-dir"], cwd)?;
    let state_dir = take_path_option(args, &["--state-dir"], cwd)?;
    Ok(PathOptions { project_dir, output_dir, state_dir })
}

pub fn workspace(cwd: &Path, options: &PathOptions) -> Result<PathBuf, String> {
    let candidate = options.project_dir.as_deref().unwrap_or(cwd);
    let canonical = fs::canonicalize(candidate).map_err(|error| {
        format!("could not resolve project directory {}: {error}", candidate.display())
    })?;
    if !canonical.is_dir() {
        return Err(format!("project directory is not a directory: {}", canonical.display()));
    }
    let canonical = native_tool_path(canonical);
    Ok(single_cargo_wrapper(&canonical).unwrap_or(canonical))
}

fn single_cargo_wrapper(root: &Path) -> Option<PathBuf> {
    if root.join("Cargo.toml").is_file() {
        return None;
    }

    let mut level = vec![root.to_path_buf()];
    for _ in 0..3 {
        let mut next = Vec::new();
        let mut candidates = Vec::new();
        for directory in level {
            let entries = fs::read_dir(directory).ok()?;
            for entry in entries.filter_map(Result::ok) {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                if path.join("Cargo.toml").is_file() {
                    candidates.push(path);
                } else {
                    next.push(path);
                }
            }
        }
        match candidates.len() {
            0 => level = next,
            1 => return candidates.pop(),
            _ => return None,
        }
    }
    None
}

fn native_tool_path(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        windows_native_tool_path(path)
    }
    #[cfg(not(windows))]
    {
        passthrough_tool_path(path)
    }
}

#[cfg(windows)]
fn windows_native_tool_path(path: PathBuf) -> PathBuf {
    use std::{
        ffi::OsString,
        os::windows::ffi::{OsStrExt, OsStringExt},
    };

    const VERBATIM: &[u16] = &[b'\\' as u16, b'\\' as u16, b'?' as u16, b'\\' as u16];
    const UNC: &[u16] = &[b'U' as u16, b'N' as u16, b'C' as u16, b'\\' as u16];

    let wide = path.as_os_str().encode_wide().collect::<Vec<_>>();
    let Some(rest) = wide.strip_prefix(VERBATIM) else {
        return path;
    };
    let mut native = Vec::with_capacity(rest.len() + 2);
    if let Some(rest) = rest.strip_prefix(UNC) {
        native.extend_from_slice(&[b'\\' as u16, b'\\' as u16]);
        native.extend_from_slice(rest);
    } else {
        native.extend_from_slice(rest);
    }
    PathBuf::from(OsString::from_wide(&native))
}

#[cfg(not(windows))]
fn passthrough_tool_path(path: PathBuf) -> PathBuf {
    path
}

pub fn resolve_layout(
    workspace: &Path,
    config: &QaConfig,
    options: &PathOptions,
) -> Result<QaRunLayout, String> {
    if !options.external_requested() {
        return Ok(QaRunLayout::local(workspace, config));
    }
    let invocation_dir = env::current_dir().map_err(|error| {
        format!("could not resolve the current directory for {STATE_HOME_ENV}: {error}")
    })?;
    let env_home = state_home_from_env_value(env::var_os(STATE_HOME_ENV), &invocation_dir);
    resolve_layout_with_home(workspace, config, options, env_home)
}

fn state_home_from_env_value(value: Option<std::ffi::OsString>, cwd: &Path) -> Option<PathBuf> {
    let value = value.filter(|value| !value.is_empty())?;
    let path = PathBuf::from(value);
    Some(if path.is_absolute() { path } else { cwd.join(path) })
}

fn resolve_layout_with_home(
    workspace: &Path,
    config: &QaConfig,
    options: &PathOptions,
    env_home: Option<PathBuf>,
) -> Result<QaRunLayout, String> {
    if !options.external_requested() {
        return Ok(QaRunLayout::local(workspace, config));
    }

    let state_dir = match (&options.state_dir, &options.output_dir, env_home) {
        (Some(state), _, _) => state.clone(),
        (None, Some(output), _) => output.join("state"),
        (None, None, Some(home)) => project_state_dir(&home, workspace),
        (None, None, None) => project_state_dir(&default_state_home()?, workspace),
    };
    let reports_dir = options.output_dir.clone().unwrap_or_else(|| state_dir.join("reports"));

    Ok(QaRunLayout {
        artifact_root: state_dir.clone(),
        coverage_dir: state_dir.join("coverage"),
        mutation_dir: state_dir.join("mutations"),
        cargo_target_dir: Some(state_dir.join("build").join("target")),
        reports_dir,
        state_dir,
    })
}

fn take_path_option(
    args: &mut Vec<String>,
    names: &[&str],
    cwd: &Path,
) -> Result<Option<PathBuf>, String> {
    let mut found = None;
    while let Some((index, inline_match)) = next_path_option(args, names) {
        if found.is_some() {
            return Err(format!("{} may only be specified once", names[0]));
        }
        let value = match inline_match {
            Some(value) => {
                args.remove(index);
                value
            }
            None => {
                if index + 1 >= args.len() || args[index + 1].starts_with("--") {
                    return Err(format!("{} requires a directory", names[0]));
                }
                args.remove(index);
                args.remove(index)
            }
        };
        if value.trim().is_empty() {
            return Err(format!("{} requires a non-empty directory", names[0]));
        }
        found = Some(absolute_from(cwd, Path::new(&value)));
    }
    Ok(found)
}

fn next_path_option(args: &[String], names: &[&str]) -> Option<(usize, Option<String>)> {
    args.iter().enumerate().find_map(|(index, argument)| {
        if names.iter().any(|name| argument == name) {
            return Some((index, None));
        }
        names.iter().find_map(|name| {
            argument.strip_prefix(&format!("{name}=")).map(|value| (index, Some(value.to_string())))
        })
    })
}

fn absolute_from(cwd: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() { path.to_path_buf() } else { cwd.join(path) }
}

mod state;
use state::*;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_ID: AtomicU64 = AtomicU64::new(1);

    fn root(name: &str) -> PathBuf {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let path = env::temp_dir().join(format!("urqa-paths-{name}-{}-{id}", std::process::id()));
        if path.exists() {
            fs::remove_dir_all(&path).unwrap();
        }
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("Cargo.toml"), "[workspace]\n").unwrap();
        path
    }

    #[test]
    fn path_flags_are_removed_and_relative_values_anchor_to_invocation_directory() {
        let cwd = root("parse");
        let mut args = vec![
            "full".into(),
            "--project-dir".into(),
            "project".into(),
            "--output-dir=reports".into(),
            "--state-dir".into(),
            "state".into(),
        ];
        let options = take_path_options(&mut args, &cwd).unwrap();
        assert_eq!(args, vec!["full"]);
        assert_eq!(options.project_dir, Some(cwd.join("project")));
        assert_eq!(options.output_dir, Some(cwd.join("reports")));
        assert_eq!(options.state_dir, Some(cwd.join("state")));
        fs::remove_dir_all(cwd).unwrap();
    }

    #[test]
    fn custom_output_without_state_places_state_inside_output() {
        let workspace = root("output");
        let output = workspace.join("elsewhere");
        let options = PathOptions { output_dir: Some(output.clone()), ..Default::default() };
        let layout = resolve_layout(&workspace, &QaConfig::default(), &options).unwrap();
        assert_eq!(layout.reports_dir, output);
        assert_eq!(layout.state_dir, layout.reports_dir.join("state"));
        assert_eq!(layout.coverage_dir, layout.state_dir.join("coverage"));
        assert_eq!(layout.mutation_dir, layout.state_dir.join("mutations"));
        assert_eq!(layout.cargo_target_dir, Some(layout.state_dir.join("build").join("target")));
        fs::remove_dir_all(workspace).unwrap();
    }

    #[test]
    fn explicit_state_defaults_reports_to_state_reports() {
        let workspace = root("state");
        let state = workspace.join("state-home");
        let options = PathOptions { state_dir: Some(state.clone()), ..Default::default() };
        let layout = resolve_layout(&workspace, &QaConfig::default(), &options).unwrap();
        assert_eq!(layout.state_dir, state);
        assert_eq!(layout.reports_dir, layout.state_dir.join("reports"));
        assert_eq!(layout.artifact_root, layout.state_dir);
        fs::remove_dir_all(workspace).unwrap();
    }

    #[test]
    fn plain_local_mode_preserves_legacy_project_artifacts() {
        let workspace = root("local");
        let config = QaConfig { output_dir: "custom-qa-out".into(), ..Default::default() };
        let layout = QaRunLayout::local(&workspace, &config);
        assert_eq!(layout.reports_dir, workspace.join("custom-qa-out"));
        assert_eq!(layout.coverage_dir, workspace.join("custom-qa-out"));
        assert_eq!(layout.mutation_dir, workspace.join("mutants.out"));
        assert_eq!(layout.cargo_target_dir, None);
        fs::remove_dir_all(workspace).unwrap();
    }

    #[test]
    fn project_hash_matches_known_vector_and_is_path_sensitive() {
        assert_eq!(fnv1a_path(Path::new("alpha")), 0x8ac6_25bb_85ed_202b);
        assert_ne!(fnv1a_path(Path::new("alpha")), fnv1a_path(Path::new("beta")));
    }

    #[test]
    fn project_dir_only_uses_external_per_project_state_and_target() {
        let invocation = root("project-only-invocation");
        let project = root("project-only-target");
        let options = PathOptions { project_dir: Some(project.clone()), ..Default::default() };
        let resolved = workspace(&invocation, &options).unwrap();
        #[cfg(windows)]
        assert!(!resolved.to_string_lossy().starts_with(r"\\?\"));
        let layout = resolve_layout(&resolved, &QaConfig::default(), &options).unwrap();
        assert_ne!(layout.state_dir, resolved);
        assert!(layout.state_dir.ends_with(format!("{:016x}", fnv1a_path(&resolved))));
        assert_eq!(layout.reports_dir, layout.state_dir.join("reports"));
        assert_eq!(layout.coverage_dir, layout.state_dir.join("coverage"));
        assert_eq!(layout.mutation_dir, layout.state_dir.join("mutations"));
        assert_eq!(layout.cargo_target_dir, Some(layout.state_dir.join("build").join("target")));
        fs::remove_dir_all(invocation).unwrap();
        fs::remove_dir_all(project).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn verbatim_windows_paths_are_converted_for_external_tools() {
        assert_eq!(
            native_tool_path(PathBuf::from(r"\\?\C:\repo\crate")),
            PathBuf::from(r"C:\repo\crate")
        );
        assert_eq!(
            native_tool_path(PathBuf::from(r"\\?\UNC\server\share\repo")),
            PathBuf::from(r"\\server\share\repo")
        );
        assert_eq!(native_tool_path(PathBuf::from(r"C:\repo")), PathBuf::from(r"C:\repo"));
    }

    #[test]
    fn path_option_parser_rejects_missing_and_duplicate_values() {
        let cwd = root("parse-errors");
        let mut missing = vec!["--project-dir".into()];
        assert!(
            take_path_options(&mut missing, &cwd).unwrap_err().contains("requires a directory")
        );

        let mut duplicate = vec!["--output-dir=a".into(), "--output-dir".into(), "b".into()];
        assert!(
            take_path_options(&mut duplicate, &cwd).unwrap_err().contains("only be specified once")
        );

        let mut empty_inline = vec!["--state-dir=".into()];
        assert!(
            take_path_options(&mut empty_inline, &cwd).unwrap_err().contains("non-empty directory")
        );

        let mut option_as_value = vec!["--project-dir".into(), "--interactive".into()];
        assert!(
            take_path_options(&mut option_as_value, &cwd)
                .unwrap_err()
                .contains("requires a directory")
        );
        fs::remove_dir_all(cwd).unwrap();
    }

    #[test]
    fn workspace_resolution_rejects_files_and_missing_projects() {
        let cwd = root("workspace-errors");
        let file = cwd.join("not-a-directory");
        fs::write(&file, "x").unwrap();
        let file_options = PathOptions { project_dir: Some(file), ..Default::default() };
        assert!(workspace(&cwd, &file_options).unwrap_err().contains("not a directory"));

        let missing_options =
            PathOptions { project_dir: Some(cwd.join("missing-project")), ..Default::default() };
        assert!(workspace(&cwd, &missing_options).unwrap_err().contains("could not resolve"));
        fs::remove_dir_all(cwd).unwrap();
    }

    #[test]
    fn workspace_resolution_unwraps_one_immediate_cargo_project() {
        let outer = root("cargo-wrapper");
        fs::remove_file(outer.join("Cargo.toml")).unwrap();
        let inner = outer.join("rusty-kaspa-master");
        fs::create_dir_all(&inner).unwrap();
        fs::write(inner.join("Cargo.toml"), "[workspace]\n").unwrap();

        assert_eq!(workspace(&outer, &PathOptions::default()).unwrap(), inner);
        fs::remove_dir_all(outer).unwrap();
    }

    #[test]
    fn workspace_resolution_unwraps_multiple_archive_directories() {
        let outer = root("cargo-double-wrapper");
        fs::remove_file(outer.join("Cargo.toml")).unwrap();
        let wrapper = outer.join("download");
        let inner = wrapper.join("rusty-kaspa-master");
        fs::create_dir_all(&inner).unwrap();
        fs::write(inner.join("Cargo.toml"), "[workspace]\n").unwrap();

        assert_eq!(workspace(&outer, &PathOptions::default()).unwrap(), inner);
        fs::remove_dir_all(outer).unwrap();
    }

    #[test]
    fn workspace_resolution_does_not_guess_between_multiple_cargo_projects() {
        let outer = root("cargo-wrapper-ambiguous");
        fs::remove_file(outer.join("Cargo.toml")).unwrap();
        for name in ["first", "second"] {
            let child = outer.join(name);
            fs::create_dir_all(&child).unwrap();
            fs::write(child.join("Cargo.toml"), "[workspace]\n").unwrap();
        }

        assert_eq!(workspace(&outer, &PathOptions::default()).unwrap(), outer);
        fs::remove_dir_all(outer).unwrap();
    }

    #[test]
    fn explicit_output_and_state_are_independent_and_environment_home_is_used_for_external_project()
    {
        let workspace = root("layout-precedence");
        let output = workspace.join("reports-only");
        let state = workspace.join("state-only");
        let options = PathOptions {
            output_dir: Some(output.clone()),
            state_dir: Some(state.clone()),
            ..Default::default()
        };
        let layout = resolve_layout_with_home(
            &workspace,
            &QaConfig::default(),
            &options,
            Some(workspace.join("ignored-env-home")),
        )
        .unwrap();
        assert_eq!(layout.reports_dir, output);
        assert_eq!(layout.state_dir, state);

        let env_home = workspace.join("env-home");
        let env_options =
            PathOptions { project_dir: Some(workspace.clone()), ..Default::default() };
        let env_layout = resolve_layout_with_home(
            &workspace,
            &QaConfig::default(),
            &env_options,
            Some(env_home.clone()),
        )
        .unwrap();
        assert_eq!(env_layout.state_dir, project_state_dir(&env_home, &workspace));
        assert_eq!(env_layout.reports_dir, env_layout.state_dir.join("reports"));
        fs::remove_dir_all(workspace).unwrap();
    }

    #[test]
    fn plain_local_mode_ignores_external_state_home_override() {
        let workspace = root("local-env");
        let env_home = workspace.join("configured-global-state-home");
        let layout = resolve_layout_with_home(
            &workspace,
            &QaConfig::default(),
            &PathOptions::default(),
            Some(env_home),
        )
        .unwrap();
        assert_eq!(layout.state_dir, workspace);
        assert_eq!(layout.reports_dir, workspace.join("qa-out"));
        assert_eq!(layout.mutation_dir, workspace.join("mutants.out"));
        assert_eq!(layout.cargo_target_dir, None);
        fs::remove_dir_all(workspace).unwrap();
    }

    #[test]
    fn environment_state_home_value_rejects_empty_and_resolves_relative_values() {
        let cwd = root("env-value");
        assert_eq!(state_home_from_env_value(None, &cwd), None);
        assert_eq!(state_home_from_env_value(Some("".into()), &cwd), None);
        assert_eq!(
            state_home_from_env_value(Some("relative-state".into()), &cwd),
            Some(cwd.join("relative-state"))
        );
        assert_eq!(
            state_home_from_env_value(Some(cwd.clone().into_os_string()), &cwd),
            Some(cwd.clone())
        );
        fs::remove_dir_all(cwd).unwrap();
    }

    #[test]
    fn default_state_home_matches_the_current_platform_contract() {
        let state = default_state_home().unwrap();
        #[cfg(windows)]
        assert_eq!(
            state,
            PathBuf::from(env::var_os("LOCALAPPDATA").unwrap()).join("UniversalRustQA")
        );
        #[cfg(target_os = "macos")]
        assert_eq!(
            state,
            PathBuf::from(env::var_os("HOME").unwrap())
                .join("Library/Application Support/UniversalRustQA")
        );
        #[cfg(all(not(windows), not(target_os = "macos")))]
        {
            let expected = env::var_os("XDG_STATE_HOME")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
                .map(|path| path.join("universal-rust-qa"))
                .unwrap_or_else(|| {
                    PathBuf::from(env::var_os("HOME").unwrap())
                        .join(".local/state/universal-rust-qa")
                });
            assert_eq!(state, expected);
        }
    }

    #[test]
    fn next_path_option_distinguishes_exact_inline_and_unrelated_arguments() {
        let args = vec![
            "full".into(),
            "--output-dir=reports".into(),
            "--project-dir".into(),
            "project".into(),
        ];
        assert_eq!(next_path_option(&args, &["--output-dir"]), Some((1, Some("reports".into()))));
        assert_eq!(next_path_option(&args, &["--project-dir"]), Some((2, None)));
        assert_eq!(next_path_option(&args, &["--state-dir"]), None);
    }

    #[test]
    fn path_helpers_cover_absolute_relative_and_external_request_semantics() {
        let workspace = root("helpers");
        assert_eq!(absolute_from(&workspace, Path::new("child")), workspace.join("child"));
        assert_eq!(absolute_from(&workspace, &workspace), workspace);
        assert!(!PathOptions::default().external_requested());
        assert!(
            PathOptions { project_dir: Some(PathBuf::from("p")), ..Default::default() }
                .external_requested()
        );
        assert!(
            PathOptions { output_dir: Some(PathBuf::from("o")), ..Default::default() }
                .external_requested()
        );
        assert!(
            PathOptions { state_dir: Some(PathBuf::from("s")), ..Default::default() }
                .external_requested()
        );
        fs::remove_dir_all(workspace).unwrap();
    }
}
