use crate::util::{has_attr, sanitize};
use qa_model::{Finding, Severity};
use qa_policy::QaConfig;
use qa_syntax::{SourceFunction, SourceType, WorkspaceSource};

pub fn analyze(source: &WorkspaceSource, config: &QaConfig, findings: &mut Vec<Finding>) {
    if !config.state.enabled {
        return;
    }
    analyze_transition_functions(source, config, findings);
    analyze_state_types(source, config, findings);
}

fn analyze_transition_functions(
    source: &WorkspaceSource,
    config: &QaConfig,
    findings: &mut Vec<Finding>,
) {
    for function in source.functions.iter().filter(|function| is_state_function(function)) {
        let code = sanitize(&function.source);
        check_transition_match(function, config, &code, findings);
        check_async_atomicity(function, &code, findings);
    }
}

fn check_transition_match(
    function: &SourceFunction,
    config: &QaConfig,
    code: &str,
    findings: &mut Vec<Finding>,
) {
    if !code.contains("match ") {
        return;
    }
    let wildcard = code.lines().any(|line| line.contains("_ =>"));
    let explicit_reject = ["Err(", "InvalidTransition", "InvalidState", "reject", "Rejected"]
        .iter()
        .any(|token| code.contains(token));
    if config.state.require_explicit_invalid_transition {
        check_invalid_transition(function, wildcard, explicit_reject, findings);
    }
    check_panicking_wildcard(function, code, wildcard, findings);
}

fn check_invalid_transition(
    function: &SourceFunction,
    wildcard: bool,
    explicit_reject: bool,
    findings: &mut Vec<Finding>,
) {
    if wildcard && !explicit_reject {
        emit(
            findings,
            function,
            "QA-STATE-001",
            Severity::High,
            "Wildcard state transition does not explicitly reject invalid input",
        );
    }
    if !explicit_reject {
        emit(
            findings,
            function,
            "QA-STATE-004",
            Severity::Medium,
            "No recognized explicit invalid-transition rejection path",
        );
    }
}

fn check_panicking_wildcard(
    function: &SourceFunction,
    code: &str,
    wildcard: bool,
    findings: &mut Vec<Finding>,
) {
    let panics = ["panic!(", "unreachable!(", "todo!(", "unimplemented!("]
        .iter()
        .any(|token| code.contains(token));
    if wildcard && panics {
        emit(
            findings,
            function,
            "QA-STATE-001",
            Severity::Critical,
            "Wildcard transition terminates through panic/unreachable behavior",
        );
    }
}

fn check_async_atomicity(function: &SourceFunction, code: &str, findings: &mut Vec<Finding>) {
    if function.is_async && code.contains(".await") && mutates_state_before_await(code) {
        emit(
            findings,
            function,
            "QA-STATE-007",
            Severity::High,
            "State mutation appears to cross an async cancellation boundary",
        );
    }
}

fn analyze_state_types(source: &WorkspaceSource, config: &QaConfig, findings: &mut Vec<Finding>) {
    for ty in source.types.iter().filter(|ty| is_state_type(ty)) {
        check_roundtrip_contract(source, ty, config, findings);
        check_restart_contract(source, ty, config, findings);
        check_variant_reachability(source, ty, findings);
        check_terminal_states(source, ty, config, findings);
    }
}

fn is_state_type(ty: &SourceType) -> bool {
    ["critical_state", "state_machine"].iter().any(|name| has_attr(&ty.attributes, name))
}

fn check_roundtrip_contract(
    source: &WorkspaceSource,
    ty: &SourceType,
    config: &QaConfig,
    findings: &mut Vec<Finding>,
) {
    if !config.state.require_roundtrip_contract {
        return;
    }
    let leaf = ty.name.rsplit("::").next().unwrap_or(&ty.name);
    if !has_roundtrip_test(source, leaf) {
        findings.push(Finding {
            rule_id: "QA-STATE-002".into(),
            severity: Severity::High,
            message: format!(
                "Critical state type `{}` lacks a recognized serialization round-trip property test",
                ty.name
            ),
            path: Some(ty.path.display().to_string()),
            line: Some(ty.line),
            detail: None,
        });
    }
}

fn check_restart_contract(
    source: &WorkspaceSource,
    ty: &SourceType,
    config: &QaConfig,
    findings: &mut Vec<Finding>,
) {
    if !config.state.require_restart_contract {
        return;
    }
    let leaf = ty.name.rsplit("::").next().unwrap_or(&ty.name);
    if !has_restart_test(source, leaf) {
        findings.push(Finding {
            rule_id: "QA-STATE-006".into(),
            severity: Severity::Medium,
            message: format!(
                "Critical state type `{}` lacks a recognized restart/persistence invariant test",
                ty.name
            ),
            path: Some(ty.path.display().to_string()),
            line: Some(ty.line),
            detail: None,
        });
    }
}

fn check_variant_reachability(
    source: &WorkspaceSource,
    ty: &SourceType,
    findings: &mut Vec<Finding>,
) {
    for variant in &ty.variant_names {
        let needle = format!("::{variant}");
        let used = source
            .functions
            .iter()
            .any(|function| !function.is_test && function.source.contains(&needle));
        if !used {
            findings.push(Finding {
                rule_id: "QA-STATE-003".into(),
                severity: Severity::Low,
                message: format!(
                    "State variant `{}::{variant}` has no recognized production transition/reference",
                    ty.name
                ),
                path: Some(ty.path.display().to_string()),
                line: Some(ty.line),
                detail: Some(
                    "Heuristic reachability signal; macros/generated transitions may require an exception."
                        .into(),
                ),
            });
        }
    }
}

fn check_terminal_states(
    source: &WorkspaceSource,
    ty: &SourceType,
    config: &QaConfig,
    findings: &mut Vec<Finding>,
) {
    if !config.state.reject_terminal_exit {
        return;
    }
    for variant in &ty.terminal_variants {
        check_terminal_variant(source, variant, findings);
    }
}

fn check_terminal_variant(source: &WorkspaceSource, variant: &str, findings: &mut Vec<Finding>) {
    let needle = format!("::{variant}");
    for function in source
        .functions
        .iter()
        .filter(|function| !function.is_test && function.source.contains(&needle))
    {
        let code = sanitize(&function.source);
        if terminal_arm_rejects(&code, variant) == Some(false) {
            emit(
                findings,
                function,
                "QA-STATE-005",
                Severity::High,
                &format!(
                    "Terminal state `{variant}` participates in a non-rejecting transition path"
                ),
            );
        }
    }
}

fn terminal_arm_rejects(code: &str, variant: &str) -> Option<bool> {
    let needle = format!("::{variant}");
    let mut rest = code;
    while let Some(index) = rest.find(&needle) {
        let after_variant = rest.get(index..)?.strip_prefix(&needle)?;
        let trimmed = after_variant.trim_start();
        if let Some(arm) = trimmed.strip_prefix("=>") {
            let arm = arm.split(',').next().unwrap_or(arm);
            return Some(
                ["Err(", "return Err", "InvalidTransition", "InvalidState", "reject", "Rejected"]
                    .iter()
                    .any(|token| arm.contains(token)),
            );
        }
        rest = after_variant;
    }
    None
}

fn is_state_function(function: &SourceFunction) -> bool {
    ["critical_state", "state_machine"].iter().any(|name| has_attr(&function.attributes, name))
}

fn mutates_state_before_await(code: &str) -> bool {
    let before = code.split(".await").next().unwrap_or(code);
    ["self.state =", "state =", "self.phase =", "phase =", "transition("]
        .iter()
        .any(|needle| before.contains(needle))
}

fn has_roundtrip_test(source: &WorkspaceSource, leaf: &str) -> bool {
    source.functions.iter().filter(|function| function.is_test).any(|function| {
        if !function.source.contains(leaf) {
            return false;
        }
        let roundtrip_tokens =
            ["roundtrip", "round_trip", "serialize", "deserialize", "encode", "decode"];
        let body =
            function.source.split_once('{').map_or(function.source.as_str(), |(_, body)| body);
        let matches = roundtrip_tokens.iter().filter(|token| body.contains(**token)).count();
        matches >= 2 || has_attr(&function.attributes, "property")
    })
}

fn has_restart_test(source: &WorkspaceSource, leaf: &str) -> bool {
    source.functions.iter().filter(|function| function.is_test).any(|function| {
        if !function.source.contains(leaf) {
            return false;
        }
        let name = function.name.to_ascii_lowercase();
        let body = function
            .source
            .split_once('{')
            .map_or(function.source.as_str(), |(_, body)| body)
            .to_ascii_lowercase();
        ["restart", "restore", "persist", "reload", "reopen"]
            .iter()
            .any(|token| name.contains(token) || body.contains(token))
    })
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
