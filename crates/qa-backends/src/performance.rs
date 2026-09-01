use qa_model::{EvidenceRecord, EvidenceStatus, FunctionMetric};
use qa_policy::QaConfig;
use std::{collections::BTreeMap, path::Path};

pub fn run(
    workspace: &Path,
    config: &QaConfig,
    execute: bool,
    update: bool,
    functions: &[FunctionMetric],
) -> Vec<EvidenceRecord> {
    if !config.performance.enabled {
        return vec![record(
            "PERF",
            "suite",
            EvidenceStatus::Disabled,
            None,
            "performance profile disabled",
        )];
    }
    if !performance_requested(execute, update) {
        return vec![record(
            "PERF",
            "suite",
            EvidenceStatus::Unknown,
            None,
            "explicit performance run required",
        )];
    }

    let hot = hot_functions(functions);
    let mut output = hot_path_header(&hot);
    let mut counts = BTreeMap::new();
    for function in hot {
        inspect_hot_path(workspace, function, &mut counts, &mut output);
    }

    instruction_baseline(workspace, config, update, &counts, &mut output);
    add_tool_evidence(workspace, config, update, &mut output);
    output
}

fn performance_requested(execute: bool, update: bool) -> bool {
    execute || update
}

fn hot_functions(functions: &[FunctionMetric]) -> Vec<&FunctionMetric> {
    functions
        .iter()
        .filter(|function| {
            function.attributes.iter().any(|attribute| {
                attribute.contains("hot_path") || attribute.contains("vectorize_expected")
            })
        })
        .collect()
}

fn hot_path_header(hot: &[&FunctionMetric]) -> Vec<EvidenceRecord> {
    if hot.is_empty() {
        vec![record(
            "PERF",
            "hot-paths",
            EvidenceStatus::NotApplicable,
            None,
            "no hot_path/vectorize_expected functions discovered",
        )]
    } else {
        Vec::new()
    }
}

fn inspect_hot_path(
    workspace: &Path,
    function: &FunctionMetric,
    counts: &mut BTreeMap<String, usize>,
    output: &mut Vec<EvidenceRecord>,
) {
    let args = vec!["asm".into(), function.qualified_name.clone()];
    inspect_asm_result(
        function,
        super::process::run(workspace, "cargo", &args, &[]),
        counts,
        output,
    );
}

fn inspect_asm_result(
    function: &FunctionMetric,
    result: std::io::Result<std::process::Output>,
    counts: &mut BTreeMap<String, usize>,
    output: &mut Vec<EvidenceRecord>,
) {
    match result {
        Ok(result) => inspect_asm_output(function, result, counts, output),
        Err(error) => output.push(asm_record(
            function,
            EvidenceStatus::Unavailable,
            &format!("cargo-asm unavailable or failed: {error}"),
        )),
    }
}

fn inspect_asm_output(
    function: &FunctionMetric,
    result: std::process::Output,
    counts: &mut BTreeMap<String, usize>,
    output: &mut Vec<EvidenceRecord>,
) {
    if !result.status.success() {
        output.push(asm_record(function, EvidenceStatus::Failed, &stderr(&result.stderr, 800)));
        return;
    }
    let asm = String::from_utf8_lossy(&result.stdout);
    counts.insert(function.qualified_name.clone(), instruction_count(&asm));
    add_vectorization_evidence(function, &asm, output);
}

fn asm_record(function: &FunctionMetric, status: EvidenceStatus, detail: &str) -> EvidenceRecord {
    record(
        "PERF",
        &format!("asm:{}", function.qualified_name),
        status,
        Some(Path::new(&function.path)),
        detail,
    )
}

fn add_vectorization_evidence(
    function: &FunctionMetric,
    asm: &str,
    output: &mut Vec<EvidenceRecord>,
) {
    let expected =
        function.attributes.iter().any(|attribute| attribute.contains("vectorize_expected"));
    if !expected {
        return;
    }
    let simd = simd_evidence(asm);
    output.push(record(
        "PERF",
        &format!("vectorize:{}", function.qualified_name),
        if simd { EvidenceStatus::Available } else { EvidenceStatus::Failed },
        Some(Path::new(&function.path)),
        if simd {
            "SIMD/vector instruction evidence found"
        } else {
            "No recognized SIMD/vector instruction evidence found in cargo-asm output"
        },
    ));
}

fn add_tool_evidence(
    workspace: &Path,
    config: &QaConfig,
    update: bool,
    output: &mut Vec<EvidenceRecord>,
) {
    output.push(tool(
        workspace,
        "BLOAT",
        "cargo-bloat",
        "cargo",
        &["bloat", "--release", "--crates", "-n", "20"],
    ));
    binary_bloat(workspace, config, update, output);
    output.push(tool(
        workspace,
        "BLOAT",
        "cargo-llvm-lines",
        "cargo",
        &["llvm-lines", "--release"],
    ));
}

mod baseline;
use baseline::*;

fn instruction_count(asm: &str) -> usize {
    asm.lines()
        .filter(|line| {
            let text = line.trim();
            !text.is_empty()
                && !text.ends_with(':')
                && !text.starts_with('.')
                && !text.starts_with(';')
        })
        .count()
}

fn simd_evidence(asm: &str) -> bool {
    let lower = asm.to_ascii_lowercase();
    ["xmm", "ymm", "zmm", "vadd", "vmul", "vpx", "neon", "q0", "v0."]
        .iter()
        .any(|needle| lower.contains(needle))
}

fn tool(
    workspace: &Path,
    family: &str,
    name: &str,
    program: &str,
    args: &[&str],
) -> EvidenceRecord {
    let args = args.iter().map(|value| (*value).to_string()).collect::<Vec<_>>();
    match super::process::run(workspace, program, &args, &[]) {
        Ok(output) if output.status.success() => record(
            family,
            name,
            EvidenceStatus::Available,
            None,
            &String::from_utf8_lossy(&output.stdout).chars().take(1000).collect::<String>(),
        ),
        Ok(output) => {
            let detail = stderr(&output.stderr, 1000);
            record(
                family,
                name,
                if detail.contains("no such command") {
                    EvidenceStatus::Unavailable
                } else {
                    EvidenceStatus::Failed
                },
                None,
                &detail,
            )
        }
        Err(error) => record(family, name, EvidenceStatus::Unavailable, None, &error.to_string()),
    }
}

fn stderr(bytes: &[u8], limit: usize) -> String {
    String::from_utf8_lossy(bytes).chars().take(limit).collect()
}

fn record(
    family: &str,
    name: &str,
    status: EvidenceStatus,
    path: Option<&Path>,
    detail: &str,
) -> EvidenceRecord {
    EvidenceRecord {
        family: family.into(),
        check: name.into(),
        status,
        source: path.map(|path| path.display().to_string()),
        detail: Some(detail.into()),
    }
}

#[cfg(test)]
mod tests;
