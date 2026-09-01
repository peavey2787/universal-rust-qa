use super::*;
use std::fs;

fn missing_command() -> String {
    "universal-rust-qa-command-that-does-not-exist".into()
}

#[test]
fn open_reports_spawn_failure_with_empty_args_and_missing_path() {
    let viewer = ViewerConfig { command: missing_command(), args: vec![] };
    let path = std::env::temp_dir().join("urqa-editor-missing-file");
    assert_eq!(resolved_path(&path), path);
    assert!(viewer_args(&viewer, &path, 7).is_empty());
    assert!(open(&viewer, &path, 7).is_err());
}

#[test]
fn viewer_arguments_expand_canonical_path_and_line_before_spawning() {
    let path = std::env::temp_dir().join(format!("urqa-editor-{}", std::process::id()));
    fs::write(&path, "fixture").unwrap();
    let viewer = ViewerConfig {
        command: missing_command(),
        args: vec!["--goto".into(), "{path}:{line}".into(), "line={line}".into()],
    };
    let canonical = path.canonicalize().unwrap();
    assert_eq!(resolved_path(&path), canonical);
    assert_eq!(
        viewer_args(&viewer, &canonical, 42),
        vec![
            "--goto".to_string(),
            format!("{}:42", canonical.to_string_lossy()),
            "line=42".to_string(),
        ]
    );
    assert!(open(&viewer, &path, 42).is_err());
    fs::remove_file(path).unwrap();
}
