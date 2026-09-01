use qa_model::{EvidenceRecord, EvidenceStatus, RuleRegistry};
use qa_policy::QaConfig;
use std::{collections::BTreeSet, fs, path::Path};
use walkdir::WalkDir;

const FAMILIES: &[&str] = &[
    "METRIC", "SPRAWL", "DUP", "DEAD", "ARCH", "TEST", "COV", "MUT", "FUZZ", "SAFE", "MATH",
    "PARSE", "STATE", "SECRET", "CT", "CONC", "ASYNC", "ERR", "RES", "ALLOC", "ENV", "CFG",
    "BUILD", "LAYOUT", "FFI", "HW", "PERF", "HARDEN", "BLOAT", "DOC", "DEP", "API", "GEN", "REPRO",
    "SAN", "DIFF", "FAULT", "SNAP", "MIR",
];

pub fn run(
    workspace: &Path,
    config: &QaConfig,
    execute: bool,
    registry: &RuleRegistry,
) -> Vec<EvidenceRecord> {
    if !config.self_hardening.enabled {
        return vec![record("suite", EvidenceStatus::Disabled, None, "self-hardening disabled")];
    }
    if !execute {
        return vec![record(
            "suite",
            EvidenceStatus::Unknown,
            None,
            "explicit self-hardening/release run required",
        )];
    }

    let mut evidence = Vec::new();
    if config.self_hardening.require_rule_registry_integrity {
        check_rule_registry(workspace, registry, &mut evidence);
    }
    if config.self_hardening.require_report_schema {
        check_schemas(workspace, &mut evidence);
    }
    check_source_sprawl(workspace, config, &mut evidence);
    check_launchers(workspace, &mut evidence);
    check_tool_installer_probe_contract(workspace, &mut evidence);
    check_golden_mir_fixtures(workspace, &mut evidence);
    if config.self_hardening.require_clean_tree {
        check_git_clean(workspace, &mut evidence);
    }
    evidence
}

fn check_rule_registry(workspace: &Path, registry: &RuleRegistry, out: &mut Vec<EvidenceRecord>) {
    let mut ids = BTreeSet::new();
    let duplicates = registry.rules.iter().filter(|rule| !ids.insert(rule.id.clone())).count();
    let families = registry.rules.iter().map(|rule| rule.family.as_str()).collect::<BTreeSet<_>>();
    let missing =
        FAMILIES.iter().filter(|family| !families.contains(*family)).copied().collect::<Vec<_>>();
    let status = if duplicates == 0 && missing.is_empty() {
        EvidenceStatus::Available
    } else {
        EvidenceStatus::Failed
    };
    out.push(record(
        "rule-registry",
        status,
        None,
        &format!(
            "{} rules, {duplicates} duplicate IDs, missing families: {}",
            registry.rules.len(),
            missing.join(", ")
        ),
    ));

    let source_path = workspace.join("crates/qa-rules/src/registry.rs");
    let source_ids = fs::read_to_string(&source_path)
        .ok()
        .map(|text| extract_rule_ids(&text))
        .unwrap_or_default();
    let runtime_ids = registry.rules.iter().map(|rule| rule.id.clone()).collect::<BTreeSet<_>>();
    let same = source_ids == runtime_ids;
    out.push(record(
        "registry-differential",
        if same { EvidenceStatus::Available } else { EvidenceStatus::Failed },
        Some(&source_path),
        if same {
            "source registry IDs and runtime registry IDs agree exactly"
        } else {
            "source-level and runtime rule-registry projections diverge"
        },
    ));
}

fn check_schemas(workspace: &Path, out: &mut Vec<EvidenceRecord>) {
    for relative in [
        "schemas/qa-config.schema.json",
        "schemas/qa-report.schema.json",
        "schemas/rule-registry.schema.json",
    ] {
        let path = workspace.join(relative);
        let valid = fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .is_some();
        out.push(record(
            &format!("schema:{relative}"),
            if valid { EvidenceStatus::Available } else { EvidenceStatus::Failed },
            Some(&path),
            if valid { "valid JSON schema document" } else { "missing or invalid JSON" },
        ));
    }
}

fn check_source_sprawl(workspace: &Path, config: &QaConfig, out: &mut Vec<EvidenceRecord>) {
    let mut oversized = Vec::new();
    for entry in WalkDir::new(workspace).into_iter().filter_map(Result::ok) {
        let path = entry.path();
        if !entry.file_type().is_file()
            || path.extension().and_then(|value| value.to_str()) != Some("rs")
            || excluded(path)
        {
            continue;
        }
        if let Ok(text) = fs::read_to_string(path) {
            let lines = text.lines().count();
            if lines > config.self_hardening.max_source_file_loc {
                oversized.push(format!("{} ({lines})", path.display()));
            }
        }
    }
    let detail = if oversized.is_empty() {
        "all Rust source files satisfy the self-host maximum physical LOC".to_string()
    } else {
        format!("oversized: {}", oversized.join(", "))
    };
    out.push(record(
        "source-sprawl",
        if oversized.is_empty() { EvidenceStatus::Available } else { EvidenceStatus::Failed },
        None,
        &detail,
    ));
}

fn check_launchers(workspace: &Path, out: &mut Vec<EvidenceRecord>) {
    for relative in ["run-all-tests.sh", "run-all-tests.cmd"] {
        let path = workspace.join(relative);
        let present = path.is_file();
        out.push(record(
            &format!("launcher:{relative}"),
            if present { EvidenceStatus::Available } else { EvidenceStatus::Failed },
            Some(&path),
            if present { "top-level self-hardening launcher present" } else { "missing" },
        ));
    }
}

fn check_tool_installer_probe_contract(workspace: &Path, out: &mut Vec<EvidenceRecord>) {
    let powershell = workspace.join("scripts/install-qa-tools.ps1");
    let shell = workspace.join("scripts/install-qa-tools.sh");
    let ps_text = fs::read_to_string(&powershell).unwrap_or_default();
    let sh_text = fs::read_to_string(&shell).unwrap_or_default();

    let ps_ok = ps_text.contains("Get-Command $Executable -ErrorAction SilentlyContinue")
        && !ps_text.contains("& cargo $Subcommand --version");
    let sh_ok = sh_text.contains("command -v \"$executable\"")
        && !sh_text.contains("cargo \"$sub\" --version");
    let ok = ps_ok && sh_ok;
    out.push(record(
        "launcher-tool-probe",
        if ok {
            EvidenceStatus::Available
        } else {
            EvidenceStatus::Failed
        },
        None,
        if ok {
            "Cargo plugins are discovered by executable name without invoking missing subcommands"
        } else {
            "tool installers must discover cargo-<plugin> executables directly; probing a missing cargo subcommand can terminate PowerShell bootstrap"
        },
    ));
}

fn check_golden_mir_fixtures(workspace: &Path, out: &mut Vec<EvidenceRecord>) {
    for relative in
        ["fixtures/pass/mir/no_panic_no_alloc.rs", "fixtures/fail/mir/panic_and_alloc.rs"]
    {
        let path = workspace.join(relative);
        let present = path.is_file();
        out.push(record(
            &format!("golden:{relative}"),
            if present { EvidenceStatus::Available } else { EvidenceStatus::Failed },
            Some(&path),
            if present {
                "golden MIR regression fixture present"
            } else {
                "missing golden MIR fixture"
            },
        ));
    }
}

fn check_git_clean(workspace: &Path, out: &mut Vec<EvidenceRecord>) {
    let args = vec!["status".into(), "--porcelain".into()];
    match super::process::run(workspace, "git", &args, &[]) {
        Ok(output) => {
            let text = String::from_utf8_lossy(&output.stdout);
            let clean = text.trim().is_empty();
            let detail = if clean {
                "working tree clean".to_string()
            } else {
                format!("working tree changes: {}", text.trim().replace('\n', "; "))
            };
            out.push(record(
                "git-clean",
                if clean { EvidenceStatus::Available } else { EvidenceStatus::Failed },
                None,
                &detail,
            ));
        }
        Err(error) => {
            out.push(record("git-clean", EvidenceStatus::Unknown, None, &error.to_string()))
        }
    }
}

fn excluded(path: &Path) -> bool {
    path.components().any(|component| {
        matches!(component.as_os_str().to_str(), Some("target" | "qa-out" | "vendor" | ".git"))
    })
}

fn record(name: &str, status: EvidenceStatus, path: Option<&Path>, detail: &str) -> EvidenceRecord {
    EvidenceRecord {
        family: "SELF".into(),
        check: name.into(),
        status,
        source: path.map(|value| value.display().to_string()),
        detail: Some(detail.into()),
    }
}

fn extract_rule_ids(text: &str) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    for chunk in text.split('"') {
        if chunk.starts_with("QA-")
            && chunk
                .bytes()
                .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'-')
        {
            ids.insert(chunk.to_string());
        }
    }
    ids
}

#[cfg(test)]
mod tests;
