use crate::util::{has_attr, sanitize};
use qa_model::{Finding, Severity};
use qa_policy::QaConfig;
use qa_syntax::WorkspaceSource;
use std::{fs, path::Path};
use walkdir::WalkDir;
pub fn analyze(s: &WorkspaceSource, c: &QaConfig, o: &mut Vec<Finding>) {
    snapshots(&s.root, c, o);
    docs(s, c, o);
    dependencies(&s.root, c, o);
    api(s, c, o);
}
fn snapshots(root: &Path, config: &QaConfig, findings: &mut Vec<Finding>) {
    for entry in WalkDir::new(root).into_iter().filter_map(Result::ok) {
        let path = entry.path();
        if !entry.file_type().is_file() || excluded(path) {
            continue;
        }
        let name = path.file_name().and_then(|value| value.to_str()).unwrap_or("");
        check_pending_snapshot(path, name, config, findings);
        check_snapshot_automation(path, name, config, findings);
        check_snapshot_secrets(path, name, config, findings);
    }
}

fn check_pending_snapshot(path: &Path, name: &str, config: &QaConfig, findings: &mut Vec<Finding>) {
    let pending = [".snap.new", ".snap.pending"].iter().any(|suffix| name.ends_with(suffix));
    if config.snapshots.pending.eq_ignore_ascii_case("deny") && pending {
        findings.push(f(
            "QA-SNAP-003",
            Severity::High,
            "Unreviewed pending snapshot is present",
            path,
            None,
        ));
    }
}

fn check_snapshot_automation(
    path: &Path,
    name: &str,
    config: &QaConfig,
    findings: &mut Vec<Finding>,
) {
    if !config.snapshots.ci_updates.eq_ignore_ascii_case("deny") || !automation_file(name) {
        return;
    }
    let Ok(text) = fs::read_to_string(path) else { return };
    let auto_approve =
        ["cargo insta accept", "--accept", "INSTA_UPDATE=always", "INSTA_UPDATE\"=\"always"]
            .iter()
            .any(|token| text.contains(token));
    if auto_approve {
        findings.push(f(
            "QA-SNAP-001",
            Severity::High,
            "Snapshot auto-approval/update command appears in repository automation",
            path,
            None,
        ));
    }
}

fn automation_file(name: &str) -> bool {
    [".yml", ".yaml", ".toml", ".ps1", ".sh", ".cmd"].iter().any(|suffix| name.ends_with(suffix))
}

fn check_snapshot_secrets(path: &Path, name: &str, config: &QaConfig, findings: &mut Vec<Finding>) {
    if !name.ends_with(".snap") || !config.snapshots.secret_scan {
        return;
    }
    let Ok(text) = fs::read_to_string(path) else { return };
    let lower = text.to_ascii_lowercase();
    let secret =
        ["private_key", "seed phrase", "mnemonic", "secret_key", "api_token", "access_token"]
            .iter()
            .any(|token| lower.contains(token));
    if secret {
        findings.push(f(
            "QA-SNAP-005",
            Severity::Critical,
            "Snapshot appears to contain secret-bearing field names",
            path,
            None,
        ));
    }
}

fn docs(s: &WorkspaceSource, c: &QaConfig, o: &mut Vec<Finding>) {
    for x in &s.functions {
        if !x.is_public {
            continue;
        }
        let attrs = x.attributes.join(" ");
        let documented = attrs.contains("doc =");
        if !documented && !c.api.public_missing_docs.eq_ignore_ascii_case("allow") {
            o.push(Finding {
                rule_id: "QA-DOC-001".into(),
                severity: if c.api.public_missing_docs.eq_ignore_ascii_case("deny") {
                    Severity::High
                } else {
                    Severity::Low
                },
                message: format!("Public function `{}` has no rustdoc", x.qualified_name),
                path: Some(x.path.display().to_string()),
                line: Some(x.line),
                detail: None,
            });
        }
        if c.documentation.critical_requires_example
            && has_attr(&x.attributes, "critical")
            && !(attrs.contains("Examples") || attrs.contains("```"))
        {
            o.push(Finding {
                rule_id: "QA-DOC-002".into(),
                severity: Severity::Medium,
                message: format!(
                    "Critical public API `{}` lacks a recognized runnable example",
                    x.qualified_name
                ),
                path: Some(x.path.display().to_string()),
                line: Some(x.line),
                detail: None,
            });
        }
    }
}
fn dependencies(root: &Path, config: &QaConfig, findings: &mut Vec<Finding>) {
    for entry in WalkDir::new(root).max_depth(5).into_iter().filter_map(Result::ok) {
        let path = entry.path();
        if !cargo_manifest(entry.file_type().is_file(), path) || excluded(path) {
            continue;
        }
        analyze_manifest_dependencies(path, config, findings);
    }
}

fn cargo_manifest(is_file: bool, path: &Path) -> bool {
    is_file && path.file_name().and_then(|value| value.to_str()) == Some("Cargo.toml")
}

fn analyze_manifest_dependencies(path: &Path, config: &QaConfig, findings: &mut Vec<Finding>) {
    let Ok(text) = fs::read_to_string(path) else { return };
    let Ok(value) = toml::from_str::<toml::Value>(&text) else { return };
    for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
        if let Some(table) = value.get(section).and_then(toml::Value::as_table) {
            for (name, dependency) in table {
                check_dependency(path, name, dependency, config, findings);
            }
        }
    }
}

fn check_dependency(
    path: &Path,
    name: &str,
    dependency: &toml::Value,
    config: &QaConfig,
    findings: &mut Vec<Finding>,
) {
    if config.dependencies.deny_wildcards && dependency.as_str() == Some("*") {
        findings.push(f(
            "QA-DEP-004",
            Severity::High,
            &format!("Wildcard dependency version for `{name}`"),
            path,
            None,
        ));
    }
    let git = dependency.as_table().and_then(|table| table.get("git")).is_some();
    if config.dependencies.deny_git_dependencies && git {
        findings.push(f(
            "QA-DEP-003",
            Severity::High,
            &format!("Git dependency `{name}` is forbidden by policy"),
            path,
            None,
        ));
    }
}

fn api(source: &WorkspaceSource, config: &QaConfig, findings: &mut Vec<Finding>) {
    for function in &source.functions {
        check_unsafe_api(function, config, findings);
        check_must_use(function, config, findings);
        check_internal_type_leak(function, findings);
    }
}

fn check_unsafe_api(
    function: &qa_syntax::SourceFunction,
    config: &QaConfig,
    findings: &mut Vec<Finding>,
) {
    if !function.is_public || !function.is_unsafe || !config.api.unsafe_requires_safety_docs {
        return;
    }
    let attrs = function.attributes.join(" ");
    let documented = attrs.contains("Safety") || function.source.contains("SAFETY");
    if !documented {
        findings.push(Finding {
            rule_id: "QA-API-005".into(),
            severity: Severity::High,
            message: format!(
                "Public unsafe API `{}` lacks a # Safety contract",
                function.qualified_name
            ),
            path: Some(function.path.display().to_string()),
            line: Some(function.line),
            detail: None,
        });
    }
}

fn check_must_use(
    function: &qa_syntax::SourceFunction,
    config: &QaConfig,
    findings: &mut Vec<Finding>,
) {
    let result_api = function.is_public && function.source.contains("-> Result");
    let required = !config.api.must_use_results.eq_ignore_ascii_case("allow");
    if result_api && required && !has_attr(&function.attributes, "must_use") {
        findings.push(Finding {
            rule_id: "QA-API-006".into(),
            severity: Severity::Low,
            message: format!(
                "Public Result-returning API `{}` is not explicitly #[must_use]",
                function.qualified_name
            ),
            path: Some(function.path.display().to_string()),
            line: Some(function.line),
            detail: Some(
                "Rust's Result type is already must_use; this rule is informational API-contract hygiene."
                    .into(),
            ),
        });
    }
}

fn check_internal_type_leak(function: &qa_syntax::SourceFunction, findings: &mut Vec<Finding>) {
    if !function.is_public {
        return;
    }
    let code = sanitize(&function.source);
    if code.contains("pub ") && code.contains("::internal") {
        findings.push(Finding {
            rule_id: "QA-API-003".into(),
            severity: Severity::Medium,
            message: format!(
                "Public API `{}` appears to expose an internal module type",
                function.qualified_name
            ),
            path: Some(function.path.display().to_string()),
            line: Some(function.line),
            detail: None,
        });
    }
}

fn f(id: &str, severity: Severity, msg: &str, p: &Path, line: Option<usize>) -> Finding {
    Finding {
        rule_id: id.into(),
        severity,
        message: msg.into(),
        path: Some(p.display().to_string()),
        line,
        detail: None,
    }
}
fn excluded(p: &Path) -> bool {
    p.components()
        .any(|c| matches!(c.as_os_str().to_str(), Some("target" | "qa-out" | "vendor" | ".git")))
}

#[cfg(test)]
mod tests;
