use qa_model::{Finding, Severity};
use qa_policy::QaConfig;
use qa_syntax::WorkspaceSource;
use std::fs;
pub fn analyze(s: &WorkspaceSource, c: &QaConfig, o: &mut Vec<Finding>) {
    if !c.hardening.enabled {
        return;
    }
    if c.hardening.release_overflow_checks {
        let p = s.root.join("Cargo.toml");
        if let Ok(t) = fs::read_to_string(&p) {
            let compact = t.split_whitespace().collect::<String>();
            if compact.contains("[profile.release]")
                && !compact.contains("overflow-checks=true")
                && !compact.contains("overflow_checks=true")
            {
                o.push(Finding{rule_id:"QA-HARDEN-001".into(),severity:Severity::High,message:"Release profile does not explicitly enable overflow checks".into(),path:Some(p.display().to_string()),line:None,detail:Some("Mission-critical strict profile requires [profile.release] overflow-checks = true.".into())});
            } else if !compact.contains("[profile.release]") {
                o.push(Finding {
                    rule_id: "QA-HARDEN-001".into(),
                    severity: Severity::Medium,
                    message: "No explicit [profile.release] hardening profile is declared".into(),
                    path: Some(p.display().to_string()),
                    line: None,
                    detail: Some(
                        "The final artifact backend still verifies applicable mitigations.".into(),
                    ),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests;
