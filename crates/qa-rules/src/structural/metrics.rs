use qa_model::{Finding, Severity};
use qa_policy::QaConfig;
use qa_syntax::SourceFunction;
pub fn logical_loc(t: &str) -> usize {
    t.lines()
        .filter(|l| {
            let s = l.trim();
            !s.is_empty() && !s.starts_with("//") && !matches!(s, "{" | "}" | "};")
        })
        .count()
}
pub fn cyclomatic(t: &str) -> usize {
    let mut c = 1;
    for l in t.lines() {
        let s = l.trim();
        c += s.matches("if ").count()
            + s.matches("while ").count()
            + s.matches("for ").count()
            + s.matches("&&").count()
            + s.matches("||").count();
        if s.contains("=>") {
            c += 1
        }
    }
    c
}
pub fn cognitive(t: &str) -> usize {
    let mut score = 0;
    let mut depth: usize = 0;
    for l in t.lines() {
        let s = l.trim();
        if s.starts_with('}') {
            depth = depth.saturating_sub(1)
        }
        if ["if ", "for ", "while ", "match ", "loop "].iter().any(|x| s.contains(x)) {
            score += 1 + depth
        }
        if s.ends_with('{') {
            depth += 1
        }
    }
    score
}
pub fn findings(
    function: &SourceFunction,
    config: &QaConfig,
    loc: usize,
    cc: usize,
    cognitive: usize,
) -> Vec<Finding> {
    let mut findings = Vec::new();
    let loc_limit =
        attribute_limit(&function.attributes, "loc").unwrap_or(config.metrics.function_loc);
    let cc_limit = effective_cc_limit(function, config);
    if loc > loc_limit {
        findings.push(Finding {
            rule_id: "QA-SPRAWL-002".into(),
            severity: Severity::Medium,
            message: format!("Function `{}` has {loc} logical LOC", function.qualified_name),
            path: Some(function.path.display().to_string()),
            line: Some(function.line),
            detail: None,
        });
    }
    if cc > cc_limit {
        findings.push(Finding {
            rule_id: "QA-METRIC-001".into(),
            severity: Severity::High,
            message: format!("Function `{}` CC {cc} exceeds {cc_limit}", function.qualified_name),
            path: Some(function.path.display().to_string()),
            line: Some(function.line),
            detail: None,
        });
    }
    if cognitive > config.metrics.cognitive {
        findings.push(Finding {
            rule_id: "QA-METRIC-002".into(),
            severity: Severity::Medium,
            message: format!(
                "Function `{}` cognitive complexity {cognitive} exceeds {}",
                function.qualified_name, config.metrics.cognitive
            ),
            path: Some(function.path.display().to_string()),
            line: Some(function.line),
            detail: None,
        });
    }
    findings
}

pub fn effective_cc_limit(function: &SourceFunction, config: &QaConfig) -> usize {
    effective_cc_limit_for_attributes(&function.attributes, config)
}

pub fn effective_cc_limit_for_attributes(attributes: &[String], config: &QaConfig) -> usize {
    attribute_limit(attributes, "cc").unwrap_or(config.metrics.cyclomatic)
}

fn attribute_limit(attributes: &[String], key: &str) -> Option<usize> {
    attributes
        .iter()
        .filter(|attribute| attribute.contains("allow") && attribute.contains(key))
        .filter_map(|attribute| parse_limit(attribute, key))
        .max()
}

fn parse_limit(attribute: &str, key: &str) -> Option<usize> {
    let start = attribute.find(key)? + key.len();
    let rest = attribute.get(start..)?.trim_start();
    let rest = rest.strip_prefix('=')?.trim_start();
    let digits = rest.chars().take_while(char::is_ascii_digit).collect::<String>();
    digits.parse().ok()
}

#[cfg(test)]
mod tests;
