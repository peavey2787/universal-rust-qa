mod detail;
mod reports;

use qa_model::{QaReport, SummaryMetrics};
use qa_policy::QaConfig;
use qa_sdk::ProgressSnapshot;
use std::{
    io::{self, Write},
    path::Path,
};

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const CYAN: &str = "\x1b[38;5;110m";
const GREEN: &str = "\x1b[38;5;114m";
const YELLOW: &str = "\x1b[38;5;179m";
const RED: &str = "\x1b[38;5;167m";

type DashboardResult = Result<(), Box<dyn std::error::Error>>;
type DashboardAction = fn(&Path, &QaReport, &QaConfig) -> DashboardResult;

const MAIN_ACTIONS: [(&str, DashboardAction); 11] = [
    ("1", action_loc),
    ("2", action_cc),
    ("3", action_crap),
    ("4", action_tests),
    ("5", action_duplicate),
    ("6", action_dead),
    ("7", action_findings),
    ("8", action_evidence),
    ("r", action_reports),
    ("s", action_settings),
    ("e", action_exceptions),
];

pub fn print_dashboard(report: &QaReport, config: &QaConfig) {
    let mut output = summary_dashboard_text(
        &report.profile,
        &report.summary,
        report.findings.len(),
        report.evidence.len(),
        config,
    );
    output.push_str(&coverage_diagnostic_text(report));
    output.push_str(&format!(
        "\n  {CYAN}R{RESET} Reports   {CYAN}S{RESET} Settings   {CYAN}E{RESET} Exceptions   {CYAN}Q{RESET} Quit\n\n"
    ));
    print!("{output}");
}

pub fn live_dashboard_text(config: &QaConfig, progress: &ProgressSnapshot) -> String {
    let mut output = match &progress.summary {
        Some(summary) => summary_dashboard_text(
            &config.profile,
            summary,
            progress.finding_count,
            progress.evidence_count,
            config,
        ),
        None => pending_dashboard_text(&config.profile),
    };
    output.push_str(&progress_text(progress));
    output
}

fn summary_dashboard_text(
    profile: &str,
    summary: &SummaryMetrics,
    finding_count: usize,
    evidence_count: usize,
    config: &QaConfig,
) -> String {
    let health_color = health_color(summary.health_score);
    let mut output = format!(
        "\n{BOLD}{CYAN}╭────────────────────────────────────────────────────────────────────╮{RESET}\n"
    );
    output.push_str(&format!(
        "{BOLD}{CYAN}│{RESET}  UNIVERSAL RUST QA {}  profile {:<10} {health_color}HEALTH {:>5.1}%{RESET}  {BOLD}{CYAN}│{RESET}\n",
        crate::BUILD_REVISION, profile, summary.health_score
    ));
    output.push_str(&format!(
        "{BOLD}{CYAN}╰────────────────────────────────────────────────────────────────────╯{RESET}\n"
    ));
    output.push_str(provisional_text(summary));
    output.push_str(&row_text(
        1,
        "LOC",
        format!("avg file {:>7.1}", summary.average_file_loc),
        format!("{} files exceed {}", summary.files_over_loc, config.metrics.file_loc),
    ));
    output.push_str(&row_text(
        2,
        "CC",
        format!("avg fn   {:>7.2}", summary.average_cc),
        format!("{} functions exceed {}", summary.functions_over_cc, config.metrics.cyclomatic),
    ));
    output.push_str(&row_text(
        3,
        "CRAP",
        crap_average(summary.average_crap),
        crap_excess(summary.functions_over_crap, config.metrics.crap),
    ));
    output.push_str(&row_text(
        4,
        "Tests",
        format!("{:>5} total", summary.total_tests),
        format!("{} flagged | coverage {}", summary.invalid_tests, coverage_label(summary)),
    ));
    output.push_str(&row_text(
        5,
        "Duplicate",
        format!("{:>10.2}%", summary.duplicate_percent),
        format!("target ≤ {:.1}%", config.metrics.duplicate_percent),
    ));
    output.push_str(&row_text(
        6,
        "Dead",
        format!("{:>10.2}%", summary.dead_code_percent),
        format!("target ≤ {:.1}%", config.metrics.dead_code_percent),
    ));
    output.push_str(&format!(
        "\n  {YELLOW}#7{RESET}  {BOLD}Findings{RESET}     {RED}critical {}{RESET} | {YELLOW}high {}{RESET} | total {}\n",
        summary.critical_findings, summary.high_findings, finding_count
    ));
    output.push_str(&format!(
        "  {CYAN}#8{RESET}  Evidence     {evidence_count} backend/compiler evidence records\n"
    ));
    output.push_str(&format!(
        "      Mutation     score {} | caught {} | missed {} | timeout {}\n",
        percent_label(summary.mutation.score_percent),
        summary.mutation.caught,
        summary.mutation.missed,
        summary.mutation.timeout
    ));
    output
}

fn coverage_diagnostic_text(report: &QaReport) -> String {
    if matches!(
        report.summary.coverage.status,
        qa_model::EvidenceStatus::Available | qa_model::EvidenceStatus::Disabled
    ) {
        return String::new();
    }
    let detail = report
        .evidence
        .iter()
        .find(|record| record.family == "COV" && record.check == "workspace")
        .and_then(|record| record.detail.as_deref());
    let Some(detail) = detail else {
        return String::new();
    };
    let manifest = report
        .summary
        .coverage
        .failure_manifest
        .as_deref()
        .map(|path| format!("\n  {CYAN}coverage manifest{RESET}: {path}"))
        .unwrap_or_default();
    format!("  {YELLOW}coverage diagnostic{RESET}: {detail}{manifest}\n")
}

fn pending_dashboard_text(profile: &str) -> String {
    let mut output = format!(
        "\n{BOLD}{CYAN}╭────────────────────────────────────────────────────────────────────╮{RESET}\n"
    );
    output.push_str(&format!(
        "{BOLD}{CYAN}│{RESET}  UNIVERSAL RUST QA {}  profile {:<10} {YELLOW}HEALTH   N/A{RESET}  {BOLD}{CYAN}│{RESET}\n",
        crate::BUILD_REVISION, profile
    ));
    output.push_str(&format!(
        "{BOLD}{CYAN}╰────────────────────────────────────────────────────────────────────╯{RESET}\n"
    ));
    for (number, label) in
        [(1, "LOC"), (2, "CC"), (3, "CRAP"), (4, "Tests"), (5, "Duplicate"), (6, "Dead")]
    {
        output.push_str(&row_text(number, label, "pending".into(), "collecting evidence".into()));
    }
    output.push_str(&format!("\n  {YELLOW}#7{RESET}  {BOLD}Findings{RESET}     pending\n"));
    output.push_str(&format!("  {CYAN}#8{RESET}  Evidence     pending\n"));
    output.push_str("      Mutation     score N/A | caught 0 | missed 0 | timeout 0\n");
    output
}

fn progress_text(progress: &ProgressSnapshot) -> String {
    let (bar, percent, completed, total) = progress_bar(progress);
    let state = progress_state(progress);
    let mut output = format!("\n  {CYAN}[{bar}]{RESET} {percent:>3}%   {completed}/{total}\n");
    output.push_str(&format!(
        "  {state}  {:<18} elapsed {}\n",
        progress.category,
        elapsed_label(progress.elapsed_seconds)
    ));
    output.push_str(&format!("  {CYAN}status{RESET}  {}\n", progress.item));
    output.push_str(&format!(
        "  {CYAN}P/Space{RESET} pause/resume   {CYAN}S{RESET} skip current test/check   {CYAN}C{RESET} skip category\n"
    ));
    if let Some(note) = progress_note(progress) {
        output.push_str(&format!("  {note}\n"));
    }
    output
}

fn progress_bar(progress: &ProgressSnapshot) -> (String, usize, usize, usize) {
    const WIDTH: usize = 44;
    let total = progress.total.max(1);
    let completed = progress.completed.min(total);
    let filled = WIDTH * completed / total;
    let marker = progress_marker(progress, filled, WIDTH);
    let marker_width = usize::from(!marker.is_empty());
    let empty = WIDTH.saturating_sub(filled + marker_width);
    let bar = format!("{}{}{}", "━".repeat(filled), marker, "─".repeat(empty));
    (bar, 100 * completed / total, completed, total)
}

fn progress_marker(progress: &ProgressSnapshot, filled: usize, width: usize) -> &'static str {
    if progress.running && filled < width { "●" } else { "" }
}

fn progress_state(progress: &ProgressSnapshot) -> String {
    if progress.paused {
        return format!("{YELLOW}PAUSED{RESET}");
    }
    active_progress_state(progress)
}

fn active_progress_state(progress: &ProgressSnapshot) -> String {
    if progress.skip_category_pending {
        return format!("{YELLOW}SKIPPING CATEGORY{RESET}");
    }
    if progress.running {
        format!("{GREEN}RUNNING{RESET}")
    } else {
        format!("{GREEN}COMPLETE{RESET}")
    }
}

fn progress_note(progress: &ProgressSnapshot) -> Option<&'static str> {
    if progress.paused {
        return Some(paused_progress_note(progress));
    }
    running_progress_note(progress)
}

fn paused_progress_note(progress: &ProgressSnapshot) -> &'static str {
    if progress.process_active {
        "active process tree is suspended; resume with P or Space"
    } else {
        "pause queued; in-process work stops at the next controllable boundary"
    }
}

fn running_progress_note(progress: &ProgressSnapshot) -> Option<&'static str> {
    if !progress.process_active && progress.running {
        Some("in-process or between child commands; external-check controls remain armed")
    } else {
        None
    }
}

fn elapsed_label(seconds: u64) -> String {
    let hours = seconds / 3600;
    let minutes = seconds % 3600 / 60;
    let seconds = seconds % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}")
}

fn health_color(score: f64) -> &'static str {
    if score >= 90.0 {
        GREEN
    } else if score >= 75.0 {
        YELLOW
    } else {
        RED
    }
}

fn provisional_text(summary: &SummaryMetrics) -> &'static str {
    if !summary.health_is_provisional {
        return "";
    }
    match summary.coverage.status {
        qa_model::EvidenceStatus::Partial => {
            "  \x1b[38;5;179mprovisional health: coverage collection PARTIAL; measured functions keep real coverage/CRAP while unmeasured packages remain unknown\x1b[0m\n"
        }
        qa_model::EvidenceStatus::Failed => {
            "  \x1b[38;5;179mprovisional health: coverage collection FAILED; raw profiles and failure manifest may still contain partial execution evidence\x1b[0m\n"
        }
        qa_model::EvidenceStatus::Disabled => {
            "  \x1b[38;5;179mprovisional health: coverage is disabled; CRAP and coverage remain unavailable\x1b[0m\n"
        }
        qa_model::EvidenceStatus::Unavailable => {
            "  \x1b[38;5;179mprovisional health: coverage evidence is unavailable; CRAP cannot be calculated\x1b[0m\n"
        }
        _ => "  \x1b[38;5;179mprovisional health: real coverage evidence is not available\x1b[0m\n",
    }
}

fn crap_average(value: Option<f64>) -> String {
    value
        .map(|value| format!("avg      {:>7.2}", value))
        .unwrap_or_else(|| "avg          N/A".into())
}

fn crap_excess(value: Option<usize>, limit: f64) -> String {
    value
        .map(|value| format!("{value} functions exceed {limit:.1}"))
        .unwrap_or_else(|| "requires coverage evidence".into())
}

fn coverage_label(summary: &SummaryMetrics) -> String {
    let percent = coverage_percent_label(summary.coverage.percent);
    match summary.coverage.status {
        qa_model::EvidenceStatus::Partial => match summary.coverage.scope_percent {
            Some(scope) => format!(
                "{percent} PARTIAL (scope {scope:.1}%, {}/{})",
                summary.coverage.covered_packages, summary.coverage.eligible_packages
            ),
            None => format!("{percent} PARTIAL (scope unknown)"),
        },
        qa_model::EvidenceStatus::Available if summary.coverage.eligible_packages > 0 => format!(
            "{percent} COMPLETE (scope 100.0%, {}/{}, N/A {})",
            summary.coverage.covered_packages,
            summary.coverage.eligible_packages,
            summary.coverage.not_applicable_packages
        ),
        qa_model::EvidenceStatus::Failed if summary.coverage.profile_count > 0 => {
            format!("N/A FAILED ({} raw profiles retained)", summary.coverage.profile_count)
        }
        _ => percent,
    }
}

fn coverage_percent_label(value: Option<f64>) -> String {
    value.map(|value| format!("{:.2}%", floor_percent(value))).unwrap_or_else(|| "N/A".into())
}

fn floor_percent(value: f64) -> f64 {
    (value * 100.0).floor() / 100.0
}

fn percent_label(value: Option<f64>) -> String {
    value.map(|value| format!("{value:.1}%")).unwrap_or_else(|| "N/A".into())
}

pub fn print_blockers(report: &QaReport) -> String {
    let output = blockers_text(report);
    print!("{output}");
    output
}

fn blockers_text(report: &QaReport) -> String {
    let blocking = report
        .findings
        .iter()
        .filter(|finding| {
            matches!(finding.severity, qa_model::Severity::High | qa_model::Severity::Critical)
        })
        .collect::<Vec<_>>();
    let failed = report
        .evidence
        .iter()
        .filter(|record| {
            matches!(
                record.status,
                qa_model::EvidenceStatus::Failed | qa_model::EvidenceStatus::Unavailable
            )
        })
        .collect::<Vec<_>>();
    if no_blockers(&blocking, &failed) {
        return String::new();
    }

    let mut output = format!("  {RED}{BOLD}Blocking details{RESET}\n");
    output.push_str(&finding_blockers_text(&blocking));
    output.push_str(&evidence_blockers_text(&failed));
    output.push_str(&mutation_blockers_text(&report.mutations));
    output.push('\n');
    output
}

fn no_blockers(blocking: &[&qa_model::Finding], failed: &[&qa_model::EvidenceRecord]) -> bool {
    blocking.is_empty() && failed.is_empty()
}

fn finding_blockers_text(blocking: &[&qa_model::Finding]) -> String {
    let mut output = String::new();
    for finding in blocking.iter().take(20) {
        let location = finding_location(finding);
        output.push_str(&format!(
            "    {RED}{:?}{RESET} {}  {}  [{}]\n",
            finding.severity, finding.rule_id, finding.message, location
        ));
    }
    output.push_str(&remaining_text("blocking finding", blocking.len()));
    output
}

fn evidence_blockers_text(failed: &[&qa_model::EvidenceRecord]) -> String {
    let mut output = String::new();
    for record in failed.iter().take(20) {
        output.push_str(&format!(
            "    {RED}{:?}{RESET} {}:{}  {}\n",
            record.status,
            record.family,
            record.check,
            record.detail.as_deref().unwrap_or("no detail")
        ));
    }
    output.push_str(&remaining_text("failed/unavailable evidence record", failed.len()));
    output
}

fn mutation_blockers_text(items: &[qa_model::MutationItem]) -> String {
    if items.is_empty() {
        return String::new();
    }
    let mut output = "    Mutation survivors/timeouts:\n".to_string();
    for item in items.iter().take(20) {
        output.push_str(&format!(
            "      {}  {}  [{}]\n",
            item.outcome,
            item.mutation,
            mutation_location(item)
        ));
    }
    output.push_str(&remaining_text("mutation outcome", items.len()));
    output
}

fn remaining_text(label: &str, total: usize) -> String {
    let remaining = total.saturating_sub(20);
    if remaining > 0 { format!("      ... {remaining} more {label}(s)\n") } else { String::new() }
}

fn finding_location(finding: &qa_model::Finding) -> String {
    match (&finding.path, finding.line) {
        (Some(path), Some(line)) => format!("{path}:{line}"),
        (Some(path), None) => path.clone(),
        _ => "workspace".into(),
    }
}

fn mutation_location(item: &qa_model::MutationItem) -> String {
    match (&item.path, item.line) {
        (Some(path), Some(line)) => format!("{path}:{line}"),
        (Some(path), None) => path.clone(),
        _ => "workspace".into(),
    }
}

pub fn main_menu(workspace: &Path, report: &QaReport, config: &QaConfig) -> DashboardResult {
    loop {
        print_dashboard(report, config);
        let choice = prompt("qa> ")?.to_ascii_lowercase();
        if is_exit(&choice) {
            break;
        }
        if let Some(action) = main_action(&choice) {
            action(workspace, report, config)?;
        }
    }
    Ok(())
}

fn is_exit(choice: &str) -> bool {
    choice.is_empty() || choice == "q"
}

fn main_action(choice: &str) -> Option<DashboardAction> {
    MAIN_ACTIONS.iter().find(|(key, _)| *key == choice).map(|(_, action)| *action)
}

fn action_loc(workspace: &Path, report: &QaReport, config: &QaConfig) -> DashboardResult {
    detail::loc_menu(workspace, report, config)
}

fn action_cc(workspace: &Path, report: &QaReport, config: &QaConfig) -> DashboardResult {
    detail::cc_menu(workspace, report, config)
}

fn action_crap(workspace: &Path, report: &QaReport, config: &QaConfig) -> DashboardResult {
    detail::crap_menu(workspace, report, config)
}

fn action_tests(workspace: &Path, report: &QaReport, config: &QaConfig) -> DashboardResult {
    detail::tests_menu(workspace, report, config)
}

fn action_duplicate(workspace: &Path, report: &QaReport, config: &QaConfig) -> DashboardResult {
    detail::duplicate_menu(workspace, report, config)
}

fn action_dead(workspace: &Path, report: &QaReport, config: &QaConfig) -> DashboardResult {
    detail::dead_menu(workspace, report, config)
}

fn action_findings(workspace: &Path, report: &QaReport, config: &QaConfig) -> DashboardResult {
    detail::findings_menu(workspace, report, config)
}

fn action_evidence(_: &Path, report: &QaReport, _: &QaConfig) -> DashboardResult {
    detail::evidence_menu(report)
}

fn action_reports(workspace: &Path, _: &QaReport, config: &QaConfig) -> DashboardResult {
    reports::reports_menu(workspace, config)
}

fn action_settings(workspace: &Path, _: &QaReport, _: &QaConfig) -> DashboardResult {
    crate::settings::menu(workspace)
}

fn action_exceptions(workspace: &Path, _: &QaReport, _: &QaConfig) -> DashboardResult {
    crate::exceptions::menu(workspace)
}
pub use reports::reports_menu_at;
pub(crate) fn prompt(label: &str) -> io::Result<String> {
    print!("{label}");
    io::stdout().flush()?;
    let mut s = String::new();
    io::stdin().read_line(&mut s)?;
    Ok(s.trim().to_string())
}
fn row_text(n: u8, label: &str, a: String, b: String) -> String {
    format!("  {CYAN}#{n}{RESET}  {GREEN}●{RESET} {BOLD}{label:<10}{RESET} {a:<23} {b}\n")
}

#[cfg(test)]
mod tests;
