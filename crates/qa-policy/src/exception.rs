use qa_model::{Finding, Severity};
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QaException {
    pub rule: String,
    pub path: String,
    pub reason: String,
    pub expires: String,
    #[serde(default)]
    pub limit: Option<f64>,
}
#[derive(Debug, Default)]
pub struct ExceptionResult {
    pub findings: Vec<Finding>,
    pub suppressed: usize,
}
pub fn apply_exceptions(
    workspace: &Path,
    config: &crate::QaConfig,
    findings: Vec<Finding>,
) -> ExceptionResult {
    let today = today();
    let mut result = ExceptionResult::default();
    result.findings.extend(exception_governance(config, &today));
    for finding in findings {
        if suppressed(workspace, config, &today, &finding) {
            result.suppressed += 1;
        } else {
            result.findings.push(finding);
        }
    }
    result
}

fn exception_governance(config: &crate::QaConfig, today: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    for exception in &config.exception {
        if config.exceptions.require_reason && exception.reason.trim().is_empty() {
            findings.push(f(
                "QA-EXC-002",
                Severity::High,
                format!("Exception for {} has no reason", exception.rule),
            ));
        }
        let active = !exception.expires.trim().is_empty() && exception.expires.as_str() >= today;
        if config.exceptions.require_expiry && !active {
            findings.push(f(
                "QA-EXC-001",
                Severity::High,
                format!("Exception for {} is expired or missing expiry", exception.rule),
            ));
        }
    }
    findings
}

fn suppressed(workspace: &Path, config: &crate::QaConfig, today: &str, finding: &Finding) -> bool {
    config.exception.iter().any(|exception| {
        exception.rule == finding.rule_id
            && exception.expires.as_str() >= today
            && !exception.reason.trim().is_empty()
            && path_match(workspace, &exception.path, finding.path.as_deref())
    })
}

fn f(id: &str, s: Severity, m: String) -> Finding {
    Finding { rule_id: id.into(), severity: s, message: m, path: None, line: None, detail: None }
}
fn path_match(workspace: &Path, pat: &str, p: Option<&str>) -> bool {
    if matches!(pat, "*" | "**" | "**/*") {
        return true;
    }
    let Some(p) = p else { return false };
    let rel = Path::new(p)
        .strip_prefix(workspace)
        .unwrap_or(Path::new(p))
        .to_string_lossy()
        .replace('\\', "/");
    let pat = pat.replace('\\', "/");
    rel == pat || rel.ends_with(&format!("/{pat}")) || (pat.contains('*') && wild(&rel, &pat))
}
fn wild(s: &str, p: &str) -> bool {
    let parts: Vec<_> = p.split('*').filter(|x| !x.is_empty()).collect();
    let mut pos = 0;
    for part in parts {
        let Some(i) = s[pos..].find(part) else { return false };
        pos += i + part.len()
    }
    true
}
fn today() -> String {
    let seconds = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    iso_date_from_unix_days(unix_days(seconds))
}

fn unix_days(seconds: u64) -> u64 {
    seconds / 86_400
}

fn iso_date_from_unix_days(days: u64) -> String {
    let z = days as i64 + 719468;
    let era = z / 146097;
    let doe = z - era.saturating_mul(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut y = yoe + era.saturating_mul(400);
    let doy = doe - (yoe.saturating_mul(365) + yoe / 4 - yoe / 100);
    let mp = (doy.saturating_mul(5) + 2) / 153;
    let d = doy - (mp.saturating_mul(153) + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    y += if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests;
