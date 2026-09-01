use crate::util::{has_attr, policy_severity, sanitize, strip_comments_preserve_strings};
use qa_model::{Finding, Severity};
use qa_policy::QaConfig;
use qa_syntax::{SourceFunction, WorkspaceSource};

pub fn analyze(source: &WorkspaceSource, config: &QaConfig, findings: &mut Vec<Finding>) {
    for function in source.functions.iter().filter(|function| !function.is_test) {
        analyze_function(function, config, findings);
    }
    analyze_host_paths(source, config, findings);
}

fn analyze_function(function: &SourceFunction, config: &QaConfig, findings: &mut Vec<Finding>) {
    let code = sanitize(&function.source);
    check_panic_hygiene(function, config, &code, findings);
    check_critical_math(function, config, &code, findings);
    check_parser_bounds(function, &code, findings);
    check_unbounded_channels(function, config, &code, findings);
    check_explicit_leaks(function, config, &code, findings);
}

fn check_panic_hygiene(
    function: &SourceFunction,
    config: &QaConfig,
    code: &str,
    findings: &mut Vec<Finding>,
) {
    for (needle, id, policy) in [
        (".unwrap()", "QA-SAFE-001", &config.safety.unwrap),
        (".expect(", "QA-SAFE-002", &config.safety.expect),
        ("panic!(", "QA-SAFE-003", &config.safety.panic),
    ] {
        if !policy.eq_ignore_ascii_case("allow") && code.contains(needle) {
            findings.push(Finding {
                rule_id: id.into(),
                severity: policy_severity(policy),
                message: format!(
                    "Production function `{}` uses `{needle}`",
                    function.qualified_name
                ),
                path: Some(function.path.display().to_string()),
                line: Some(function.line),
                detail: None,
            });
        }
    }
}

fn check_critical_math(
    function: &SourceFunction,
    config: &QaConfig,
    code: &str,
    findings: &mut Vec<Finding>,
) {
    if !config.safety.critical_checked_arithmetic
        || !has_attr(&function.attributes, "critical_math")
    {
        return;
    }
    let arithmetic = [" + ", " - ", " * "].iter().any(|needle| code.contains(needle));
    let explicit = ["checked_", "saturating_", "wrapping_", "overflowing_"]
        .iter()
        .any(|needle| code.contains(needle));
    if arithmetic && !explicit {
        findings.push(Finding {
            rule_id: "QA-MATH-001".into(),
            severity: Severity::High,
            message: format!(
                "Critical math `{}` lacks explicit overflow semantics",
                function.qualified_name
            ),
            path: Some(function.path.display().to_string()),
            line: Some(function.line),
            detail: None,
        });
    }
}

fn check_parser_bounds(function: &SourceFunction, code: &str, findings: &mut Vec<Finding>) {
    if !has_attr(&function.attributes, "critical_parser") {
        return;
    }
    let unbounded_read =
        ["read_to_end(", "read_to_string("].iter().any(|needle| code.contains(needle));
    if unbounded_read && !bound(code) {
        findings.push(Finding {
            rule_id: "QA-PARSE-002".into(),
            severity: Severity::High,
            message: format!(
                "Critical parser `{}` performs unbounded read",
                function.qualified_name
            ),
            path: Some(function.path.display().to_string()),
            line: Some(function.line),
            detail: None,
        });
    }
}

fn check_unbounded_channels(
    function: &SourceFunction,
    config: &QaConfig,
    code: &str,
    findings: &mut Vec<Finding>,
) {
    if config.resources.unbounded_channels.eq_ignore_ascii_case("allow") {
        return;
    }
    let unbounded = [
        "unbounded_channel(",
        "unbounded_channel::<",
        "async_channel::unbounded(",
        "async_channel::unbounded::<",
    ]
    .iter()
    .any(|needle| code.contains(needle));
    if unbounded {
        findings.push(Finding {
            rule_id: "QA-RES-001".into(),
            severity: Severity::High,
            message: format!("`{}` creates unbounded channel", function.qualified_name),
            path: Some(function.path.display().to_string()),
            line: Some(function.line),
            detail: None,
        });
    }
}

fn check_explicit_leaks(
    function: &SourceFunction,
    config: &QaConfig,
    code: &str,
    findings: &mut Vec<Finding>,
) {
    if config.alloc.explicit_leaks.eq_ignore_ascii_case("allow") {
        return;
    }
    let leak = ["mem::forget(", "Box::leak("].iter().any(|needle| code.contains(needle));
    if leak {
        findings.push(Finding {
            rule_id: "QA-ALLOC-001".into(),
            severity: Severity::High,
            message: format!("`{}` uses explicit leak primitive", function.qualified_name),
            path: Some(function.path.display().to_string()),
            line: Some(function.line),
            detail: None,
        });
    }
}

fn analyze_host_paths(source: &WorkspaceSource, config: &QaConfig, findings: &mut Vec<Finding>) {
    if !config.environment.detect_absolute_host_paths {
        return;
    }
    for file in &source.files {
        let code = strip_comments_preserve_strings(&file.text);
        for (offset, line) in code.lines().enumerate() {
            if contains_host_path(line) {
                findings.push(Finding {
                    rule_id: "QA-ENV-002".into(),
                    severity: Severity::Medium,
                    message: "Source contains host-specific absolute path".into(),
                    path: Some(file.path.display().to_string()),
                    line: Some(offset + 1),
                    detail: Some(line.trim().into()),
                });
            }
        }
    }
}

fn contains_host_path(line: &str) -> bool {
    ["/home/", "/Users/", "C:\\Users\\", "/tmp/"].iter().any(|path| line.contains(path))
}

fn bound(source: &str) -> bool {
    source.contains("MAX_")
        || source.contains(".take(")
        || source.contains("Bounded")
        || source.to_ascii_lowercase().contains("limit")
}

#[cfg(test)]
mod tests;
