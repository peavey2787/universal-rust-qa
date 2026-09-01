use qa_model::{Finding, QaReport};
use qa_policy::QaConfig;
use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub fn write_reports(
    workspace: &Path,
    config: &QaConfig,
    report: &QaReport,
) -> io::Result<PathBuf> {
    write_reports_to(&workspace.join(&config.output_dir), config, report)
}

pub fn write_reports_to(out: &Path, config: &QaConfig, report: &QaReport) -> io::Result<PathBuf> {
    let out = out.to_path_buf();
    fs::create_dir_all(&out)?;
    fs::write(out.join("summary.txt"), crate::summary_text(report, config))?;
    json(out.join("report.json"), report)?;
    json(
        out.join("metrics.json"),
        &serde_json::json!({"summary":report.summary,"files":report.files,"functions":report.functions,"types":report.types,"interfaces":report.interfaces}),
    )?;
    json(out.join("coverage.json"), &report.summary.coverage)?;
    json(
        out.join("mutation.json"),
        &serde_json::json!({"summary":report.summary.mutation,"items":report.mutations}),
    )?;
    json(
        out.join("fuzz.json"),
        &serde_json::json!({"summary":report.summary.fuzz,"targets":report.fuzz_targets}),
    )?;
    json(out.join("duplicates.json"), &report.duplicates)?;
    json(out.join("dead-code.json"), &report.dead_items)?;
    json(out.join("findings.json"), &report.findings)?;
    json(out.join("evidence.json"), &report.evidence)?;
    for (prefix, name) in [
        ("QA-TEST-", "tests.json"),
        ("QA-SAFE-", "safety.json"),
        ("QA-MATH-", "math.json"),
        ("QA-PARSE-", "parsers.json"),
        ("QA-RES-", "resources.json"),
        ("QA-ALLOC-", "allocation.json"),
        ("QA-ENV-", "environment.json"),
        ("QA-STATE-", "state.json"),
        ("QA-ASYNC-", "async.json"),
        ("QA-CONC-", "concurrency.json"),
        ("QA-ERR-", "errors.json"),
        ("QA-SECRET-", "secrets.json"),
        ("QA-CT-", "constant-time.json"),
        ("QA-BUILD-", "build.json"),
        ("QA-LAYOUT-", "layout.json"),
        ("QA-FFI-", "ffi.json"),
        ("QA-HW-", "hardware.json"),
        ("QA-PERF-", "performance-findings.json"),
        ("QA-HARDEN-", "hardening-findings.json"),
        ("QA-SNAP-", "snapshots.json"),
        ("QA-DOC-", "documentation.json"),
        ("QA-DEP-", "dependencies.json"),
        ("QA-API-", "api.json"),
        ("QA-GEN-", "generated.json"),
        ("QA-REPRO-", "reproducibility-findings.json"),
    ] {
        json(out.join(name), &family(&report.findings, prefix))?;
    }
    for (family, name) in [
        ("SAN", "sanitizers.json"),
        ("DIFF", "differential.json"),
        ("FAULT", "fault.json"),
        ("MIR", "mir.json"),
        ("CFG", "platform.json"),
        ("CONC", "concurrency-evidence.json"),
        ("CT", "constant-time-evidence.json"),
        ("HW", "hardware-evidence.json"),
        ("PERF", "performance.json"),
        ("BLOAT", "bloat.json"),
        ("HARDEN", "hardening.json"),
        ("SNAP", "snapshot-evidence.json"),
        ("DOC", "documentation-evidence.json"),
        ("DEP", "dependency-evidence.json"),
        ("API", "api-evidence.json"),
        ("GEN", "generated-evidence.json"),
        ("REPRO", "reproducibility.json"),
        ("SELF", "self-hardening.json"),
    ] {
        json(
            out.join(name),
            &report.evidence.iter().filter(|e| e.family == family).collect::<Vec<_>>(),
        )?;
    }
    config.save(&out.join("effective-config.toml")).map_err(|e| io::Error::other(e.to_string()))?;
    Ok(out)
}
fn family<'a>(findings: &'a [Finding], prefix: &str) -> Vec<&'a Finding> {
    findings.iter().filter(|f| f.rule_id.starts_with(prefix)).collect()
}
fn json(path: PathBuf, value: &impl serde::Serialize) -> io::Result<()> {
    fs::write(path, serde_json::to_vec_pretty(value).map_err(io::Error::other)?)
}

#[cfg(test)]
mod tests;
