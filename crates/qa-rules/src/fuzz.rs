use crate::util::has_attr;
use qa_model::{EvidenceStatus, Finding, FuzzTargetEvidence, Severity};
use qa_policy::QaConfig;
use qa_syntax::WorkspaceSource;
#[derive(Default)]
pub struct FuzzOutput {
    pub targets: Vec<FuzzTargetEvidence>,
    pub critical_missing: usize,
    pub regression_artifacts: usize,
    pub unpersisted_crashes: usize,
    pub property_test_count: usize,
}
pub fn analyze(s: &WorkspaceSource, c: &QaConfig, f: &mut Vec<Finding>) -> FuzzOutput {
    let mut o = FuzzOutput::default();
    for sf in &s.files {
        if sf.path.to_string_lossy().contains("fuzz_targets") && sf.text.contains("fuzz_target!") {
            let reaches = s.functions.iter().any(|x| !x.is_test && sf.text.contains(&x.name));
            o.targets.push(FuzzTargetEvidence {
                name: sf.path.file_stem().unwrap_or_default().to_string_lossy().into(),
                path: sf.path.display().to_string(),
                line: 1,
                reaches_production: reaches,
                critical_targets: vec![],
                build_status: EvidenceStatus::Unknown,
            });
            if c.fuzz.reject_vacuous_targets && !reaches {
                f.push(Finding {
                    rule_id: "QA-FUZZ-004".into(),
                    severity: Severity::High,
                    message: "Fuzz target does not reach recognized production code".into(),
                    path: Some(sf.path.display().to_string()),
                    line: Some(1),
                    detail: None,
                })
            }
        }
    }
    for x in s.functions.iter().filter(|x| has_attr(&x.attributes, "critical_parser")) {
        if !o.targets.iter().any(|t| {
            s.files
                .iter()
                .find(|sf| sf.path.to_string_lossy() == t.path)
                .map(|sf| sf.text.contains(&x.name))
                .unwrap_or(false)
        }) {
            o.critical_missing += 1;
            f.push(Finding {
                rule_id: "QA-FUZZ-001".into(),
                severity: Severity::High,
                message: format!("Critical parser `{}` lacks fuzz target", x.qualified_name),
                path: Some(x.path.display().to_string()),
                line: Some(x.line),
                detail: None,
            })
        }
    }
    o.property_test_count = s
        .functions
        .iter()
        .filter(|x| {
            x.is_test
                && (x.source.contains("proptest!")
                    || x.attributes.iter().any(|a| a.contains("proptest")))
        })
        .count();
    o
}

#[cfg(test)]
mod tests;
