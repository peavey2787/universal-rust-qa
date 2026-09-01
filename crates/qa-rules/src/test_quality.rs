use qa_model::{Finding, Severity};
use qa_policy::QaConfig;
use qa_syntax::{SourceFunction, WorkspaceSource};
use std::collections::HashSet;
use syn::visit::Visit;

pub fn analyze(source: &WorkspaceSource, config: &QaConfig, findings: &mut Vec<Finding>) -> usize {
    let production: HashSet<_> = source
        .functions
        .iter()
        .filter(|function| !function.is_test)
        .map(|function| function.name.as_str())
        .collect();
    source
        .functions
        .iter()
        .filter(|function| function.is_test)
        .filter(|test| analyze_test(test, config, &production, findings))
        .count()
}

fn analyze_test(
    test: &SourceFunction,
    config: &QaConfig,
    production: &HashSet<&str>,
    findings: &mut Vec<Finding>,
) -> bool {
    let mut invalid = false;
    invalid |= check_assertions(test, findings);
    invalid |= check_tautology(test, config, findings);
    invalid |= check_reachability(test, config, production, findings);
    invalid |= check_determinism(test, config, findings);
    invalid
}

fn check_assertions(test: &SourceFunction, findings: &mut Vec<Finding>) -> bool {
    let assertion = ["assert!(", "assert_eq!(", "assert_ne!(", "unwrap_err(", "expect_err("]
        .iter()
        .any(|needle| test.source.contains(needle));
    let explicit_kind = test.attributes.iter().any(|attribute| {
        ["test_kind", "should_panic"].iter().any(|needle| attribute.contains(needle))
    });
    if assertion || explicit_kind {
        return false;
    }
    push(findings, test, "QA-TEST-001", Severity::Medium, "Test has no recognized assertion");
    true
}

fn check_tautology(test: &SourceFunction, config: &QaConfig, findings: &mut Vec<Finding>) -> bool {
    if !config.tests.reject_tautological_assertions {
        return false;
    }
    if tautological_assertion(&test.source) {
        push(findings, test, "QA-TEST-002", Severity::High, "Test contains tautological assertion");
        return true;
    }
    false
}

fn tautological_assertion(source: &str) -> bool {
    let Ok(function) = syn::parse_str::<syn::ItemFn>(source) else {
        return false;
    };
    let mut visitor = TautologyVisitor { found: false };
    visitor.visit_item_fn(&function);
    visitor.found
}

struct TautologyVisitor {
    found: bool,
}

impl<'ast> Visit<'ast> for TautologyVisitor {
    fn visit_macro(&mut self, item: &'ast syn::Macro) {
        if self.found {
            return;
        }
        let name = item.path.segments.last().map(|segment| segment.ident.to_string());
        let compact = item.tokens.to_string().replace(' ', "");
        self.found = match name.as_deref() {
            Some("assert") => compact == "true",
            Some("assert_eq") => self_eq(&format!("assert_eq!({compact})")),
            _ => false,
        };
    }
}

fn check_reachability(
    test: &SourceFunction,
    config: &QaConfig,
    production: &HashSet<&str>,
    findings: &mut Vec<Finding>,
) -> bool {
    if !config.tests.require_production_reachability {
        return false;
    }
    let reaches =
        test.calls.iter().any(|call| production.contains(call.rsplit("::").next().unwrap_or(call)));
    if reaches {
        return false;
    }
    push(
        findings,
        test,
        "QA-TEST-003",
        Severity::Medium,
        "Test does not reach recognized production function",
    );
    true
}

fn check_determinism(
    test: &SourceFunction,
    config: &QaConfig,
    findings: &mut Vec<Finding>,
) -> bool {
    if !config.tests.reject_unseeded_randomness {
        return false;
    }
    let nondeterministic = ["thread_rng(", "rand::random(", "SystemTime::now("]
        .iter()
        .any(|needle| test.source.contains(needle));
    if nondeterministic {
        push(findings, test, "QA-TEST-005", Severity::Medium, "Test uses nondeterministic input");
        return true;
    }
    false
}

fn push(
    findings: &mut Vec<Finding>,
    test: &SourceFunction,
    id: &str,
    severity: Severity,
    message: &str,
) {
    findings.push(Finding {
        rule_id: id.into(),
        severity,
        message: format!("{message} `{}`", test.qualified_name),
        path: Some(test.path.display().to_string()),
        line: Some(test.line),
        detail: None,
    });
}

fn self_eq(source: &str) -> bool {
    source.lines().any(|line| {
        let compact = line.replace(' ', "");
        let Some(index) = compact.find("assert_eq!(") else { return false };
        let arguments = &compact[index + 11..];
        let Some((left, right)) = top_level_assert_eq_arguments(arguments) else { return false };
        left == right
    })
}

fn top_level_assert_eq_arguments(arguments: &str) -> Option<(&str, &str)> {
    let mut depth = 0usize;
    let mut first_comma = None;
    for (index, character) in arguments.char_indices() {
        match character {
            '(' | '[' | '{' => depth += 1,
            ')' if depth == 0 => {
                let comma = first_comma?;
                return Some((&arguments[..comma], &arguments[comma + 1..index]));
            }
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                if let Some(comma) = first_comma {
                    return Some((&arguments[..comma], &arguments[comma + 1..index]));
                }
                first_comma = Some(index);
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests;
