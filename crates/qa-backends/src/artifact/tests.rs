use super::*;

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn metadata_helpers_find_workspace_binary_and_target_directory() {
    let root = workspace();
    let names = binary_names(&root);
    assert!(names.iter().any(|name| name == "cargo-qa"));

    let target = default_target_dir(&root);
    assert!(target.is_absolute());

    let paths = binary_paths(&root, &target, false);
    assert!(paths.iter().any(|path| { path.file_stem().is_some_and(|name| name == "cargo-qa") }));
}

#[test]
fn deterministic_flags_remap_workspace_and_target() {
    let root = Path::new("example-workspace");
    let target = root.join("target");
    let flags = deterministic_rustflags(root, Some(&target));
    assert!(flags.contains(&format!("--remap-path-prefix={}=/target", target.display())));
    assert!(flags.contains(&format!("--remap-path-prefix={}=/workspace", root.display())));
    assert!(!flags.split('\u{1f}').any(str::is_empty));
    assert_eq!(flags, deterministic_rustflags(root, Some(&target)));
}

#[test]
fn deterministic_flags_ignore_empty_mapping_sources() {
    let flags = deterministic_rustflags(Path::new(""), None);
    assert!(!flags.contains("--remap-path-prefix==/workspace"));
    assert!(!flags.split('\u{1f}').any(str::is_empty));
}

#[test]
fn deterministic_flags_work_without_explicit_target() {
    let flags = deterministic_rustflags(Path::new("relative-workspace"), None);
    assert!(flags.contains("--remap-path-prefix=relative-workspace=/workspace"));
}

#[test]
fn reproducibility_flags_are_stable_for_the_current_platform() {
    let root = Path::new("example-workspace");
    let target = root.join("target");
    let flags = reproducibility_rustflags(root, Some(&target));
    assert_eq!(flags, reproducibility_rustflags(root, Some(&target)));
    assert!(!flags.is_empty());
    assert!(!flags.split('\u{1f}').any(str::is_empty));
    #[cfg(windows)]
    assert_eq!(
        flags,
        [
            "-Ccodegen-units=1",
            "-Cdebuginfo=0",
            "-Cstrip=symbols",
            "-Clink-arg=/Brepro",
            "-Clink-arg=/DEBUG:NONE",
            "-Clink-arg=/INCREMENTAL:NO",
        ]
        .join("\u{1f}")
    );
    #[cfg(not(windows))]
    {
        assert!(flags.contains("--remap-path-prefix"));
    }
}
