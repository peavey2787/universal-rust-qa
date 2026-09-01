use qa_model::{EvidenceRecord, EvidenceStatus, Finding, Severity};
use qa_policy::QaConfig;
use std::{
    fs,
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

pub struct MirEvidence {
    pub records: Vec<EvidenceRecord>,
    pub findings: Vec<Finding>,
}

pub fn run(
    workspace: &Path,
    config: &QaConfig,
    artifact_root: &Path,
    execute: bool,
) -> MirEvidence {
    if config.mir.mode.eq_ignore_ascii_case("off") {
        return simple_evidence(EvidenceStatus::Disabled, "MIR analysis disabled");
    }
    if !execute {
        return simple_evidence(EvidenceStatus::Unknown, "explicit MIR run required");
    }
    let packages = package_manifests(workspace);
    if packages.is_empty() {
        return simple_evidence(
            EvidenceStatus::NotApplicable,
            "no Rust package targets discovered",
        );
    }
    execute_packages(workspace, config, artifact_root, packages)
}

fn simple_evidence(status: EvidenceStatus, detail: &str) -> MirEvidence {
    MirEvidence { records: vec![record("suite", status, None, detail)], findings: vec![] }
}

fn execute_packages(
    workspace: &Path,
    config: &QaConfig,
    artifact_root: &Path,
    packages: Vec<Package>,
) -> MirEvidence {
    let out_dir = artifact_root.join("mir");
    match fs::create_dir_all(&out_dir) {
        Ok(()) => execute_ready_packages(workspace, config, &out_dir, packages),
        Err(error) => output_directory_failure(&out_dir, &error),
    }
}

fn execute_ready_packages(
    workspace: &Path,
    config: &QaConfig,
    out_dir: &Path,
    packages: Vec<Package>,
) -> MirEvidence {
    let (aggregate, records, failed) = emit_packages(workspace, config, packages);
    finish_mir(workspace, config, out_dir, aggregate, records, failed)
}

fn emit_packages(
    workspace: &Path,
    config: &QaConfig,
    packages: Vec<Package>,
) -> (String, Vec<EvidenceRecord>, bool) {
    let mut aggregate = String::new();
    let mut records = Vec::new();
    let mut failed = false;
    for package in packages {
        let before = records.len();
        emit_package(workspace, config, &package, &mut aggregate, &mut records);
        failed |= package_failed(before, &records);
    }
    (aggregate, records, failed)
}

fn output_directory_failure(out_dir: &Path, error: &std::io::Error) -> MirEvidence {
    MirEvidence {
        records: vec![record(
            "output-directory",
            EvidenceStatus::Failed,
            Some(out_dir),
            &error.to_string(),
        )],
        findings: vec![],
    }
}

fn package_failed(before: usize, records: &[EvidenceRecord]) -> bool {
    records.len() == before
        || records.last().is_some_and(|record| record.status != EvidenceStatus::Available)
}

fn emit_package(
    workspace: &Path,
    config: &QaConfig,
    package: &Package,
    aggregate: &mut String,
    records: &mut Vec<EvidenceRecord>,
) {
    let args = rustc_args(config, package);
    match super::process::run(workspace, "cargo", &args, &[]) {
        Ok(output) => emit_package_output(package, output, aggregate, records),
        Err(error) => {
            emit_package_record(package, EvidenceStatus::Unavailable, &error.to_string(), records);
        }
    }
}

fn emit_package_output(
    package: &Package,
    output: std::process::Output,
    aggregate: &mut String,
    records: &mut Vec<EvidenceRecord>,
) {
    if !output.status.success() {
        emit_package_record(package, EvidenceStatus::Failed, &stderr(&output.stderr), records);
        return;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    aggregate.push_str(&format!("\n// ===== {} =====\n{}", package.manifest.display(), text));
    records.push(record(
        &package.name,
        EvidenceStatus::Available,
        Some(&package.manifest),
        "MIR emitted",
    ));
}

fn emit_package_record(
    package: &Package,
    status: EvidenceStatus,
    detail: &str,
    records: &mut Vec<EvidenceRecord>,
) {
    records.push(record(&package.name, status, Some(&package.manifest), detail));
}

fn finish_mir(
    workspace: &Path,
    config: &QaConfig,
    out_dir: &Path,
    aggregate: String,
    mut records: Vec<EvidenceRecord>,
    mut failed: bool,
) -> MirEvidence {
    let path = out_dir.join("workspace.mir");
    if !aggregate.is_empty() {
        if let Err(error) = fs::write(&path, &aggregate) {
            failed = true;
            records.push(record(
                "write-evidence",
                EvidenceStatus::Failed,
                Some(&path),
                &error.to_string(),
            ));
        }
    }
    let mut findings = Vec::new();
    if !aggregate.is_empty() {
        analyze_text(workspace, config, &aggregate, &mut findings);
    }
    records.push(record(
        "suite",
        if failed { EvidenceStatus::Failed } else { EvidenceStatus::Available },
        Some(&path),
        "pinned-toolchain MIR extraction and analysis completed",
    ));
    MirEvidence { records, findings }
}

#[derive(Debug)]
struct Package {
    manifest: PathBuf,
    name: String,
    lib: bool,
    bin: bool,
}

fn package_manifests(root: &Path) -> Vec<Package> {
    let mut output = Vec::new();
    for entry in WalkDir::new(root).max_depth(5).into_iter().filter_map(Result::ok) {
        if !entry.file_type().is_file()
            || entry.file_name().to_str() != Some("Cargo.toml")
            || entry.path().components().any(|component| {
                matches!(component.as_os_str().to_str(), Some("target" | "vendor" | ".git"))
            })
        {
            continue;
        }
        let Ok(text) = fs::read_to_string(entry.path()) else {
            continue;
        };
        let Ok(value) = toml::from_str::<toml::Value>(&text) else {
            continue;
        };
        let Some(package) = value.get("package") else {
            continue;
        };
        let Some(name) = package.get("name").and_then(toml::Value::as_str) else {
            continue;
        };
        let dir = entry.path().parent().unwrap_or(root);
        let lib = dir.join("src/lib.rs").exists() || value.get("lib").is_some();
        let bin = dir.join("src/main.rs").exists() || value.get("bin").is_some();
        if lib || bin {
            output.push(Package {
                manifest: entry.path().to_path_buf(),
                name: name.to_string(),
                lib,
                bin,
            });
        }
    }
    output.sort_by(|left, right| left.manifest.cmp(&right.manifest));
    output
}

fn rustc_args(config: &QaConfig, package: &Package) -> Vec<String> {
    let mut args = vec![
        format!("+{}", config.mir.toolchain),
        "rustc".into(),
        "--manifest-path".into(),
        package.manifest.display().to_string(),
    ];
    if package.lib {
        args.push("--lib".into());
    } else if package.bin {
        args.push("--bin".into());
        args.push(package.name.clone());
    }
    args.push("--".into());
    args.push("-Zunpretty=mir".into());
    args
}

fn analyze_text(workspace: &Path, config: &QaConfig, mir: &str, findings: &mut Vec<Finding>) {
    let source = qa_syntax::discover(workspace);
    for function in source.functions {
        let section = mir_section(mir, &function.name).unwrap_or("");
        if !section.is_empty() {
            analyze_function(config, section, &function, findings);
        }
    }
}

fn analyze_function(
    config: &QaConfig,
    section: &str,
    function: &qa_syntax::SourceFunction,
    findings: &mut Vec<Finding>,
) {
    let attrs = function.attributes.join(" ");
    check_panic_edges(config, section, function, &attrs, findings);
    check_allocations(config, section, function, &attrs, findings);
    check_drop_cleanup(config, section, function, &attrs, findings);
    check_zeroization(config, section, function, &attrs, findings);
    check_async_retention(config, function, &attrs, findings);
}

fn check_panic_edges(
    config: &QaConfig,
    section: &str,
    function: &qa_syntax::SourceFunction,
    attrs: &str,
    findings: &mut Vec<Finding>,
) {
    let protected = attrs.contains("no_panic") || attrs.contains("critical");
    let panic_edge = ["assert(", "begin_panic", "panic_fmt", "panic_bounds_check"]
        .iter()
        .any(|needle| section.contains(needle));
    if config.mir.check_panic_edges && protected && panic_edge {
        findings.push(finding(
            "QA-MIR-003",
            Severity::High,
            function,
            "MIR retains panic/assert edge in no-panic/critical function",
        ));
    }
}

fn check_allocations(
    config: &QaConfig,
    section: &str,
    function: &qa_syntax::SourceFunction,
    attrs: &str,
    findings: &mut Vec<Finding>,
) {
    if !config.mir.check_no_alloc || !attrs.contains("no_alloc") {
        return;
    }
    let alloc = ["exchange_malloc", "__rust_alloc", "RawVec", "with_capacity", "reserve"]
        .iter()
        .any(|needle| section.contains(needle));
    if alloc {
        findings.push(finding(
            "QA-MIR-004",
            Severity::High,
            function,
            "MIR contains a recognized allocation path in #[qa_attr::no_alloc] function",
        ));
    }
}

fn check_drop_cleanup(
    config: &QaConfig,
    section: &str,
    function: &qa_syntax::SourceFunction,
    attrs: &str,
    findings: &mut Vec<Finding>,
) {
    if !config.mir.check_drop_cleanup || !attrs.contains("hot_path") {
        return;
    }
    let drops = section.matches("drop(").count();
    if drops > 3 {
        let mut item = finding(
            "QA-MIR-001",
            Severity::Medium,
            function,
            "Hot-path MIR contains substantial implicit drop/cleanup activity",
        );
        item.detail = Some(format!("recognized drop terminators/calls: {drops}"));
        findings.push(item);
    }
}

fn check_zeroization(
    config: &QaConfig,
    section: &str,
    function: &qa_syntax::SourceFunction,
    attrs: &str,
    findings: &mut Vec<Finding>,
) {
    let secret = attrs.contains("secret") || attrs.contains("critical_crypto");
    let requested = function.source.contains("zeroize");
    let present = section.to_ascii_lowercase().contains("zeroize");
    if config.mir.check_zeroization && secret && requested && !present {
        findings.push(finding(
            "QA-MIR-002",
            Severity::Medium,
            function,
            "Source requests zeroization but recognizable zeroize call is absent from emitted MIR",
        ));
    }
}

fn check_async_retention(
    config: &QaConfig,
    function: &qa_syntax::SourceFunction,
    attrs: &str,
    findings: &mut Vec<Finding>,
) {
    if !config.mir.check_async_retention || !attrs.contains("critical_async") {
        return;
    }
    let awaits = function.source.contains(".await");
    let retained =
        ["secret", "Vec<", "String"].iter().any(|needle| function.source.contains(needle));
    if awaits && retained {
        let mut item = finding(
            "QA-MIR-005",
            Severity::Medium,
            function,
            "Critical async function may retain sensitive/large state across suspension",
        );
        item.detail = Some(
            "MIR review required to confirm which locals are captured in the coroutine state."
                .into(),
        );
        findings.push(item);
    }
}

fn finding(
    id: &str,
    severity: Severity,
    function: &qa_syntax::SourceFunction,
    message: &str,
) -> Finding {
    Finding {
        rule_id: id.into(),
        severity,
        message: format!("{message}: `{}`", function.qualified_name),
        path: Some(function.path.display().to_string()),
        line: Some(function.line),
        detail: Some(
            "MIR rules are compiler-version-coupled evidence and use a pinned nightly toolchain."
                .into(),
        ),
    }
}

fn mir_section<'a>(mir: &'a str, name: &str) -> Option<&'a str> {
    let needle = format!("fn {name}(");
    let start = mir.find(&needle)?;
    let rest = &mir[start..];
    let end =
        rest[needle.len()..].find("\nfn ").map(|index| index + needle.len()).unwrap_or(rest.len());
    Some(&rest[..end])
}

fn stderr(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).chars().take(1000).collect()
}

fn record(
    check: &str,
    status: EvidenceStatus,
    source: Option<&Path>,
    detail: &str,
) -> EvidenceRecord {
    EvidenceRecord {
        family: "MIR".into(),
        check: check.into(),
        status,
        source: source.map(|path| path.display().to_string()),
        detail: Some(detail.into()),
    }
}

#[cfg(test)]
mod tests;
