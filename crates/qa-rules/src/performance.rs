use crate::util::{has_attr, sanitize};
use qa_model::{Finding, Severity};
use qa_policy::QaConfig;
use qa_syntax::{SourceFunction, SourceType, WorkspaceSource};

pub fn analyze(source: &WorkspaceSource, config: &QaConfig, findings: &mut Vec<Finding>) {
    if !config.performance.enabled {
        return;
    }
    for ty in source.types.iter().filter(|ty| ty.kind == "struct") {
        check_false_sharing(ty, config, findings);
    }
    for function in &source.functions {
        check_function(function, findings);
    }
}

fn check_false_sharing(ty: &SourceType, config: &QaConfig, findings: &mut Vec<Finding>) {
    let shared = ty
        .field_types
        .iter()
        .filter(|field| ["Atomic", "Mutex", "RwLock"].iter().any(|token| field.contains(token)))
        .count();
    let padded = ty.source.contains("CachePadded")
        || ty.attributes.iter().any(|attribute| attribute.contains("align"));
    if shared < 2 || padded {
        return;
    }
    let severity = if config.performance.false_sharing.eq_ignore_ascii_case("deny") {
        Severity::High
    } else {
        Severity::Medium
    };
    findings.push(Finding {
        rule_id: "QA-PERF-001".into(),
        severity,
        message: format!(
            "Shared struct `{}` has {shared} adjacent synchronization fields without a recognized cache-padding contract",
            ty.name
        ),
        path: Some(ty.path.display().to_string()),
        line: Some(ty.line),
        detail: Some(
            "Potential false sharing is heuristic; annotate/pad only when fields are independently hot across threads."
                .into(),
        ),
    });
}

fn check_function(function: &SourceFunction, findings: &mut Vec<Finding>) {
    let code = sanitize(&function.source);
    check_vectorization_contract(function, &code, findings);
    check_hot_path_output(function, &code, findings);
}

fn check_vectorization_contract(
    function: &SourceFunction,
    code: &str,
    findings: &mut Vec<Finding>,
) {
    if !has_attr(&function.attributes, "vectorize_expected") {
        return;
    }
    let loop_like = ["for ", "while ", "iter"].iter().any(|token| code.contains(token));
    if !loop_like {
        findings.push(Finding {
            rule_id: "QA-PERF-002".into(),
            severity: Severity::Medium,
            message: format!(
                "Vectorization contract on `{}` has no recognized loop/iterator body",
                function.qualified_name
            ),
            path: Some(function.path.display().to_string()),
            line: Some(function.line),
            detail: Some(
                "Final vectorization is verified by the performance backend, not inferred solely from source syntax."
                    .into(),
            ),
        });
    }
}

fn check_hot_path_output(function: &SourceFunction, code: &str, findings: &mut Vec<Finding>) {
    if !has_attr(&function.attributes, "hot_path") {
        return;
    }
    let output =
        ["println !", "println!", "dbg !", "dbg!"].iter().any(|token| code.contains(token));
    if output {
        findings.push(Finding {
            rule_id: "QA-PERF-005".into(),
            severity: Severity::Medium,
            message: format!(
                "Hot path `{}` contains diagnostic formatting/output",
                function.qualified_name
            ),
            path: Some(function.path.display().to_string()),
            line: Some(function.line),
            detail: None,
        });
    }
}

#[cfg(test)]
mod tests;
