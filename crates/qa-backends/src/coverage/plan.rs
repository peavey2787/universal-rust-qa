use super::model::CoveragePackage;
use qa_policy::CoverageConfig;
use serde_json::Value;
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};
use walkdir::{DirEntry, WalkDir};

pub(super) fn workspace_packages(
    workspace: &Path,
    config: &CoverageConfig,
) -> Result<(usize, Vec<CoveragePackage>, Vec<String>), String> {
    let args = vec!["metadata".into(), "--no-deps".into(), "--format-version".into(), "1".into()];
    let output = super::super::process::run(workspace, "cargo", &args, &[])
        .map_err(|error| format!("cargo metadata unavailable: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "cargo metadata failed: {}",
            super::super::process::diagnostics(&output.stdout, &output.stderr)
        ));
    }
    let value: Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("cargo metadata JSON was malformed: {error}"))?;
    packages_from_metadata(workspace, config, &value)
}

pub(super) fn packages_from_metadata(
    workspace: &Path,
    config: &CoverageConfig,
    value: &Value,
) -> Result<(usize, Vec<CoveragePackage>, Vec<String>), String> {
    let members = string_set(value, "workspace_members");
    let default_members = string_set(value, "workspace_default_members");
    let packages = value
        .get("packages")
        .and_then(Value::as_array)
        .ok_or_else(|| "cargo metadata omitted packages".to_string())?;

    let mut raw = Vec::<(String, PathBuf, bool, bool)>::new();
    for package in packages {
        let Some(id) = package.get("id").and_then(Value::as_str) else {
            continue;
        };
        if !members.is_empty() && !members.contains(id) {
            continue;
        }
        let Some(name) = package.get("name").and_then(Value::as_str) else {
            continue;
        };
        let Some(manifest) = package.get("manifest_path").and_then(Value::as_str) else {
            continue;
        };
        let root = Path::new(manifest).parent().unwrap_or(workspace).to_path_buf();
        let testable = package
            .get("targets")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .flat_map(|target| target.get("kind").and_then(Value::as_array).into_iter().flatten())
            .filter_map(Value::as_str)
            .any(|kind| kind != "custom-build");
        raw.push((name.to_string(), root, testable, default_members.contains(id)));
    }
    raw.sort_by(|left, right| left.0.cmp(&right.0));
    let workspace_count = raw.len();

    if !config.include_packages.is_empty() {
        let found = raw.iter().map(|item| item.0.as_str()).collect::<BTreeSet<_>>();
        let missing = config
            .include_packages
            .iter()
            .filter(|name| !found.contains(name.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(format!(
                "coverage include_packages not found in workspace: {}",
                missing.join(", ")
            ));
        }
    }

    let roots = raw.iter().map(|item| item.1.clone()).collect::<Vec<_>>();
    let mut selected = Vec::new();
    let mut not_applicable = Vec::new();
    for (name, root, testable, default_member) in raw {
        if !selected_by_policy(&name, config) {
            continue;
        }
        if !testable {
            not_applicable.push(name);
            continue;
        }
        let source_loc = package_source_loc(&root, &roots)?;
        selected.push(CoveragePackage {
            name,
            root: normalize_path(&root),
            source_loc,
            default_member,
        });
    }
    Ok((workspace_count, selected, not_applicable))
}

fn string_set<'a>(value: &'a Value, field: &str) -> BTreeSet<&'a str> {
    value
        .get(field)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect()
}

fn normalize_path(path: &Path) -> String {
    super::parse::normalize(&path.to_string_lossy())
}

fn selected_by_policy(name: &str, config: &CoverageConfig) -> bool {
    let included = config.include_packages.is_empty()
        || config.include_packages.iter().any(|candidate| candidate == name);
    let excluded = config.exclude_packages.iter().any(|candidate| candidate == name);
    included && !excluded
}

fn package_source_loc(root: &Path, member_roots: &[PathBuf]) -> Result<usize, String> {
    let mut total = 0usize;
    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| include_entry(entry, root, member_roots))
    {
        let entry = entry.map_err(|error| {
            format!("failed to enumerate coverage source scope under {}: {error}", root.display())
        })?;
        if !entry.file_type().is_file()
            || entry.path().extension().is_none_or(|extension| extension != "rs")
        {
            continue;
        }
        let text = fs::read_to_string(entry.path()).map_err(|error| {
            format!(
                "failed to read coverage source {} while calculating eligible LOC: {error}",
                entry.path().display()
            )
        })?;
        total += logical_source_loc(&text);
    }
    Ok(total)
}

fn include_entry(entry: &DirEntry, root: &Path, member_roots: &[PathBuf]) -> bool {
    if entry.path() == root {
        return true;
    }
    if entry.file_type().is_dir() {
        if matches!(entry.file_name().to_str(), Some("target" | ".git" | "node_modules")) {
            return false;
        }
        if member_roots.iter().any(|candidate| candidate != root && candidate == entry.path()) {
            return false;
        }
    }
    true
}

fn logical_source_loc(text: &str) -> usize {
    text.lines()
        .filter(|line| {
            let line = line.trim();
            !line.is_empty() && !line.starts_with("//") && !matches!(line, "{" | "}" | "};")
        })
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_selection_honors_include_exclude_and_custom_build_only_members() {
        let root = std::env::temp_dir().join(format!("urqa-coverage-plan-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        for name in ["a", "b", "build"] {
            fs::create_dir_all(root.join(name).join("src")).unwrap();
        }
        fs::write(root.join("a/src/lib.rs"), "pub fn a() {}\n").unwrap();
        fs::write(root.join("b/src/lib.rs"), "pub fn b() {}\n").unwrap();
        let value = serde_json::json!({
            "workspace_members": ["a 0.1.0", "b 0.1.0", "build 0.1.0"],
            "workspace_default_members": ["a 0.1.0"],
            "packages": [
                {"id":"a 0.1.0","name":"a","manifest_path":root.join("a/Cargo.toml"),"targets":[{"kind":["lib"]}]},
                {"id":"b 0.1.0","name":"b","manifest_path":root.join("b/Cargo.toml"),"targets":[{"kind":["lib"]}]},
                {"id":"build 0.1.0","name":"build","manifest_path":root.join("build/Cargo.toml"),"targets":[{"kind":["custom-build"]}]}
            ]
        });
        let config = CoverageConfig {
            include_packages: vec!["a".into(), "build".into()],
            exclude_packages: vec![],
            ..CoverageConfig::default()
        };
        let (workspace_count, packages, not_applicable) =
            packages_from_metadata(&root, &config, &value).unwrap();
        assert_eq!(workspace_count, 3);
        assert_eq!(packages.iter().map(|package| package.name.as_str()).collect::<Vec<_>>(), ["a"]);
        assert!(packages[0].default_member);
        assert_eq!(packages[0].source_loc, 1);
        assert_eq!(not_applicable, vec!["build"]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_included_package_is_rejected_instead_of_silently_shrinking_scope() {
        let value = serde_json::json!({
            "workspace_members": ["a 0.1.0"],
            "workspace_default_members": ["a 0.1.0"],
            "packages": [{"id":"a 0.1.0","name":"a","manifest_path":"/tmp/ws/a/Cargo.toml","targets":[{"kind":["lib"]}]}]
        });
        let config = CoverageConfig {
            include_packages: vec!["missing".into()],
            ..CoverageConfig::default()
        };
        let error = packages_from_metadata(Path::new("/tmp/ws"), &config, &value).unwrap_err();
        assert!(error.contains("missing"));
    }

    #[test]
    fn logical_source_loc_uses_the_same_nonblank_comment_and_brace_contract_as_qa_loc() {
        assert_eq!(logical_source_loc("// x\n\n{\nlet x = 1;\n}\n};\n"), 1);
    }
}
