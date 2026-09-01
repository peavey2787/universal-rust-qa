use qa_model::{EvidenceStatus, QaReport};
use qa_policy::QaConfig;
use std::collections::BTreeMap;

pub fn summary_text(report: &QaReport, config: &QaConfig) -> String {
    let s = &report.summary;
    let coverage = s
        .coverage
        .percent
        .map(|value| format!("{:.2}%", floor_percent(value)))
        .unwrap_or_else(|| "N/A".into());
    let crap = s.average_crap.map(|v| format!("{v:.2}")).unwrap_or_else(|| "N/A".into());
    let mut families = BTreeMap::<String, (usize, usize, usize)>::new();
    for f in &report.findings {
        let family = f.rule_id.split('-').nth(1).unwrap_or("OTHER").to_string();
        let e = families.entry(family).or_default();
        match f.severity {
            qa_model::Severity::Critical => e.0 += 1,
            qa_model::Severity::High => e.1 += 1,
            _ => e.2 += 1,
        }
    }
    let findings = families
        .into_iter()
        .map(|(k, (c, h, o))| format!("{k:<10} critical {c:<3} high {h:<3} other {o}"))
        .collect::<Vec<_>>()
        .join("\n");
    let evidence = report
        .evidence
        .iter()
        .map(|e| {
            format!(
                "{:?} {:<6} {:<24} {}",
                e.status,
                e.family,
                e.check,
                e.detail.as_deref().unwrap_or("")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "Universal Rust QA — Summary\n===========================\nHealth: {:.1}%{}\nProfile: {}\n\n#1 LOC      avg file {:.1} | files > {}: {}\n#2 CC       avg fn   {:.2} | functions > {}: {}\n#3 CRAP     avg {} | functions > {:.1}: {}\n#4 Tests    total {} | flagged {} | coverage {}\n#5 Duplicate code   {:.2}% | target <= {:.1}%\n#6 Dead/unreachable {:.2}% | target <= {:.1}%\n\nMutation: {}\nFuzz targets: {} | critical parser targets missing {}\n\nHigh/Critical/Other by family\n-----------------------------\n{}\n\nBackend/compiler evidence\n-------------------------\n{}\n",
        s.health_score,
        if s.health_is_provisional { " (provisional)" } else { "" },
        report.profile,
        s.average_file_loc,
        config.metrics.file_loc,
        s.files_over_loc,
        s.average_cc,
        config.metrics.cyclomatic,
        s.functions_over_cc,
        crap,
        config.metrics.crap,
        s.functions_over_crap.map(|v| v.to_string()).unwrap_or_else(|| "N/A".into()),
        s.total_tests,
        s.invalid_tests,
        coverage,
        s.duplicate_percent,
        config.metrics.duplicate_percent,
        s.dead_code_percent,
        config.metrics.dead_code_percent,
        mutation_text(&s.mutation.status, s.mutation.score_percent),
        s.fuzz.target_count,
        s.fuzz.critical_targets_missing,
        findings,
        evidence
    )
}

fn floor_percent(value: f64) -> f64 {
    (value * 100.0).floor() / 100.0
}

fn mutation_text(status: &EvidenceStatus, score: Option<f64>) -> String {
    match score {
        Some(v) => format!("{v:.1}% ({status:?})"),
        None => format!("N/A ({status:?})"),
    }
}

#[cfg(test)]
mod tests;
