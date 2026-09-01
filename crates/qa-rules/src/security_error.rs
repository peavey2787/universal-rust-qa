use crate::util::{has_attr, policy_severity, sanitize};
use qa_model::{Finding, Severity};
use qa_policy::QaConfig;
use qa_syntax::{SourceFunction, SourceInterface, SourceType, WorkspaceSource};

pub fn analyze(source: &WorkspaceSource, config: &QaConfig, findings: &mut Vec<Finding>) {
    let secret_names = secret_type_names(source);
    for function in source.functions.iter().filter(|function| !function.is_test) {
        analyze_function(function, config, &secret_names, findings);
    }
    analyze_secret_types(source, config, findings);
    analyze_error_sources(source, config, findings);
}

fn secret_type_names(source: &WorkspaceSource) -> Vec<String> {
    source
        .types
        .iter()
        .filter(|ty| has_attr(&ty.attributes, "secret"))
        .map(|ty| ty.name.rsplit("::").next().unwrap_or(&ty.name).to_string())
        .collect()
}

fn analyze_function(
    function: &SourceFunction,
    config: &QaConfig,
    secret_names: &[String],
    findings: &mut Vec<Finding>,
) {
    let code = sanitize(&function.source);
    check_discarded_result(function, config, &code, findings);
    check_lost_context(function, config, &code, findings);
    check_secret_logging(function, config, secret_names, &code, findings);
    check_constant_time(function, config, &code, findings);
}

fn check_discarded_result(
    function: &SourceFunction,
    config: &QaConfig,
    code: &str,
    findings: &mut Vec<Finding>,
) {
    if config.errors.discarded_results.eq_ignore_ascii_case("allow") {
        return;
    }
    if discarded_important_result(code) {
        emit(
            findings,
            function,
            "QA-ERR-001",
            policy_severity(&config.errors.discarded_results),
            "Potentially important Result is deliberately discarded",
        );
    }
}

fn check_lost_context(
    function: &SourceFunction,
    config: &QaConfig,
    code: &str,
    findings: &mut Vec<Finding>,
) {
    if config.errors.lost_context.eq_ignore_ascii_case("allow") {
        return;
    }
    if ["map_err(|_|", "map_err(| _ |"].iter().any(|needle| code.contains(needle)) {
        emit(
            findings,
            function,
            "QA-ERR-004",
            policy_severity(&config.errors.lost_context),
            "Error mapping discards the original error/context",
        );
    }
}

fn check_secret_logging(
    function: &SourceFunction,
    config: &QaConfig,
    secret_names: &[String],
    code: &str,
    findings: &mut Vec<Finding>,
) {
    if config.errors.secret_logging.eq_ignore_ascii_case("allow") || !logging_sink(code) {
        return;
    }
    let named_secret = secret_names.iter().any(|name| code.contains(name));
    if named_secret || strong_secret_identifier(code) {
        emit(
            findings,
            function,
            "QA-ERR-002",
            Severity::Critical,
            "Potential secret-bearing value reaches a formatting/logging sink",
        );
    } else if ambiguous_secret_identifier(code) {
        emit(
            findings,
            function,
            "QA-ERR-002",
            Severity::Medium,
            "Potentially sensitive seed/token identifier reaches a formatting/logging sink; annotate the value as secret for strict enforcement",
        );
    }
}

fn check_constant_time(
    function: &SourceFunction,
    config: &QaConfig,
    code: &str,
    findings: &mut Vec<Finding>,
) {
    if !constant_time_candidate(function, config) {
        return;
    }
    for (offset, line) in code.lines().enumerate() {
        check_secret_branch(function, config, line, offset, findings);
        check_secret_index(function, config, line, offset, findings);
    }
}

fn constant_time_candidate(function: &SourceFunction, config: &QaConfig) -> bool {
    config.constant_time.enabled
        && ["critical_crypto", "secret"].iter().any(|name| has_attr(&function.attributes, name))
}

fn check_secret_branch(
    function: &SourceFunction,
    config: &QaConfig,
    line: &str,
    offset: usize,
    findings: &mut Vec<Finding>,
) {
    let lower = line.to_ascii_lowercase();
    let branches = ["if ", "match "].iter().any(|needle| lower.contains(needle));
    if branches && secret_identifier(&lower) {
        findings.push(Finding {
            rule_id: "QA-CT-001".into(),
            severity: policy_severity(&config.constant_time.secret_branch),
            message: format!(
                "Secret-dependent control flow requires constant-time review: `{}`",
                function.qualified_name
            ),
            path: Some(function.path.display().to_string()),
            line: Some(function.line + offset),
            detail: Some(
                "Static taint heuristic; use constant-time runtime/codegen evidence for a stronger conclusion."
                    .into(),
            ),
        });
    }
}

fn check_secret_index(
    function: &SourceFunction,
    config: &QaConfig,
    line: &str,
    offset: usize,
    findings: &mut Vec<Finding>,
) {
    let lower = line.to_ascii_lowercase();
    let indexing = lower.find('[').zip(lower.find(']')).is_some();
    if indexing && secret_identifier(&lower) {
        findings.push(Finding {
            rule_id: "QA-CT-002".into(),
            severity: policy_severity(&config.constant_time.secret_index),
            message: format!(
                "Secret-dependent indexing requires constant-time review: `{}`",
                function.qualified_name
            ),
            path: Some(function.path.display().to_string()),
            line: Some(function.line + offset),
            detail: Some("Static taint heuristic.".into()),
        });
    }
}

fn analyze_secret_types(source: &WorkspaceSource, config: &QaConfig, findings: &mut Vec<Finding>) {
    for ty in source.types.iter().filter(|ty| has_attr(&ty.attributes, "secret")) {
        check_secret_formatting(ty, config, findings);
        check_zeroize_contract(ty, config, findings);
    }
}

fn check_secret_formatting(ty: &SourceType, config: &QaConfig, findings: &mut Vec<Finding>) {
    if !config.secrets.deny_debug_display {
        return;
    }
    let attrs = ty.attributes.join(" ");
    if ["Debug", "Display"].iter().any(|needle| attrs.contains(needle)) {
        findings.push(Finding {
            rule_id: "QA-ERR-002".into(),
            severity: Severity::Critical,
            message: format!("Secret type `{}` exposes Debug/Display formatting", ty.name),
            path: Some(ty.path.display().to_string()),
            line: Some(ty.line),
            detail: None,
        });
    }
}

fn check_zeroize_contract(ty: &SourceType, config: &QaConfig, findings: &mut Vec<Finding>) {
    if !config.secrets.require_zeroize {
        return;
    }
    let attrs = ty.attributes.join(" ");
    if ["Zeroize", "ZeroizeOnDrop"].iter().any(|needle| attrs.contains(needle)) {
        return;
    }
    findings.push(Finding {
        rule_id: "QA-SECRET-002".into(),
        severity: Severity::High,
        message: format!(
            "Secret type `{}` lacks a recognized Zeroize/ZeroizeOnDrop contract",
            ty.name
        ),
        path: Some(ty.path.display().to_string()),
        line: Some(ty.line),
        detail: None,
    });
}

fn analyze_error_sources(source: &WorkspaceSource, config: &QaConfig, findings: &mut Vec<Finding>) {
    if config.errors.broken_sources.eq_ignore_ascii_case("allow") {
        return;
    }
    for interface in &source.interfaces {
        if custom_error_without_source(interface) {
            findings.push(Finding {
                rule_id: "QA-ERR-003".into(),
                severity: policy_severity(&config.errors.broken_sources),
                message: format!(
                    "Custom Error implementation `{}` does not expose a recognized source() chain",
                    interface.name
                ),
                path: Some(interface.path.display().to_string()),
                line: Some(interface.line),
                detail: Some(
                    "If the error has no causal source, add a scoped QA exception or use a derive that exposes sources explicitly."
                        .into(),
                ),
            });
        }
    }
}

fn custom_error_without_source(interface: &SourceInterface) -> bool {
    let error_impl = ["Error for", "impl std::error::Error", "impl Error for"]
        .iter()
        .any(|needle| interface.name.contains(needle) || interface.source.contains(needle));
    error_impl && !interface.source.contains("source(")
}

fn discarded_important_result(code: &str) -> bool {
    let discarded = ["let _ =", ".ok();", "drop("].iter().any(|needle| code.contains(needle));
    let important = [
        "write", "flush", "sync", "send", "persist", "save", "commit", "verify", "remove",
        "rename", "connect", "shutdown",
    ]
    .iter()
    .any(|needle| code.contains(needle));
    discarded && important
}

fn logging_sink(code: &str) -> bool {
    [
        "println!(",
        "eprintln!(",
        "dbg!(",
        "tracing::",
        "log::",
        "debug!(",
        "info!(",
        "warn!(",
        "error!(",
    ]
    .iter()
    .any(|needle| code.contains(needle))
}

fn secret_identifier(code: &str) -> bool {
    strong_secret_identifier(code) || ambiguous_secret_identifier(code)
}

fn strong_secret_identifier(code: &str) -> bool {
    let identifiers = identifiers(code);
    [
        "secret",
        "private_key",
        "privatekey",
        "mnemonic",
        "password",
        "credential",
        "api_key",
        "apikey",
    ]
    .iter()
    .any(|needle| identifiers.iter().any(|identifier| identifier == needle))
}

fn ambiguous_secret_identifier(code: &str) -> bool {
    let identifiers = identifiers(code);
    ["seed", "token"].iter().any(|needle| identifiers.iter().any(|identifier| identifier == needle))
}

fn identifiers(code: &str) -> Vec<String> {
    code.split(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        .filter(|identifier| !identifier.is_empty())
        .map(|identifier| identifier.to_ascii_lowercase())
        .collect()
}

fn emit(
    findings: &mut Vec<Finding>,
    function: &SourceFunction,
    id: &str,
    severity: Severity,
    message: &str,
) {
    findings.push(Finding {
        rule_id: id.into(),
        severity,
        message: format!("{message}: `{}`", function.qualified_name),
        path: Some(function.path.display().to_string()),
        line: Some(function.line),
        detail: None,
    });
}

#[cfg(test)]
mod tests;
