mod cli;
mod commands;
mod dashboard;
mod doctor;
mod editor;
mod exceptions;
mod live_ui;
mod paths;
mod settings;

pub(crate) const BUILD_REVISION: &str = "r72";

#[cfg(test)]
use commands::full_options;
use commands::{basic_action, command_options, is_help};
use qa_model::{EvidenceStatus, Severity};
use std::{
    env,
    io::IsTerminal,
    path::{Path, PathBuf},
};

fn main() {
    if let Err(e) = run() {
        eprintln!("\x1b[31merror:\x1b[0m {e}");
        std::process::exit(2)
    }
}

type CommandResult = Result<(), Box<dyn std::error::Error>>;
fn run() -> CommandResult {
    let mut args: Vec<String> = env::args().skip(1).collect();
    strip_qa_prefix(&mut args);
    let cwd = env::current_dir()?;
    let path_options = paths::take_path_options(&mut args, &cwd)?;
    let no_interactive = non_interactive(&mut args);
    let existing_coverage = existing_coverage_requested(&mut args);
    let command = args.first().map(String::as_str);

    if command.is_some_and(|value| matches!(value, "--version" | "-V")) {
        println!("cargo-qa {} ({})", env!("CARGO_PKG_VERSION"), BUILD_REVISION);
        return Ok(());
    }

    let workspace = paths::workspace(&cwd, &path_options)?;
    if let Some(action) = command.and_then(basic_action) {
        action(&workspace)?;
        return Ok(());
    }
    if let Some(mut options) = command.and_then(command_options) {
        apply_coverage_policy(&mut options, existing_coverage);
        let layout = run_layout(&workspace, &path_options)?;
        execute(&workspace, options, &layout, no_interactive, true)?;
        return Ok(());
    }
    if command.is_some_and(is_help) {
        cli::help();
        return Ok(());
    }

    run_fallback(&workspace, &args, command, &path_options, no_interactive, existing_coverage)
}

fn run_fallback(
    workspace: &Path,
    args: &[String],
    command: Option<&str>,
    path_options: &paths::PathOptions,
    no_interactive: bool,
    existing_coverage: bool,
) -> CommandResult {
    match command {
        Some("reports") => {
            let config = qa_policy::QaConfig::load(workspace)?;
            let layout = paths::resolve_layout(workspace, &config, path_options)?;
            dashboard::reports_menu_at(&layout.reports_dir, &config)?;
        }
        Some("export-config") => {
            let path = args.get(1).map(PathBuf::from).unwrap_or_else(|| "qa-export.toml".into());
            cli::export_config(workspace, &path)?;
        }
        Some("import-config") => {
            let path = args.get(1).ok_or("import-config requires file")?;
            cli::import_config(workspace, &PathBuf::from(path))?;
        }
        _ => {
            let layout = run_layout(workspace, path_options)?;
            let mut options = qa_sdk::RunOptions::default();
            apply_coverage_policy(&mut options, existing_coverage);
            execute(workspace, options, &layout, no_interactive, false)?;
        }
    }
    Ok(())
}

fn strip_qa_prefix(args: &mut Vec<String>) {
    if args.first().map(String::as_str) == Some("qa") {
        args.remove(0);
    }
}

fn existing_coverage_requested(args: &mut Vec<String>) -> bool {
    let existing = take_flag(args, "--existing-coverage");
    let reuse = take_flag(args, "--reuse-coverage");
    existing || reuse
}

fn apply_coverage_policy(options: &mut qa_sdk::RunOptions, existing_coverage: bool) {
    if existing_coverage {
        options.force_coverage = false;
    }
}

fn non_interactive(args: &mut Vec<String>) -> bool {
    let force_interactive = take_flag(args, "--interactive");
    let force_non_interactive = take_flag(args, "--no-interactive");
    non_interactive_mode(force_interactive, force_non_interactive)
}

fn non_interactive_mode(force_interactive: bool, force_non_interactive: bool) -> bool {
    force_non_interactive || !force_interactive
}

fn live_ui_enabled(stdin_terminal: bool, stdout_terminal: bool) -> bool {
    stdin_terminal && stdout_terminal
}

fn run_layout(
    workspace: &Path,
    options: &paths::PathOptions,
) -> Result<qa_sdk::QaRunLayout, Box<dyn std::error::Error>> {
    let config = qa_policy::QaConfig::load(workspace)?;
    paths::resolve_layout(workspace, &config, options).map_err(Into::into)
}

fn execute(
    workspace: &Path,
    options: qa_sdk::RunOptions,
    layout: &qa_sdk::QaRunLayout,
    no_interactive: bool,
    gate: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let live = live_ui_enabled(std::io::stdin().is_terminal(), std::io::stdout().is_terminal());
    let run = if live {
        live_ui::run(workspace, options.clone(), layout.clone())?
    } else {
        qa_sdk::run_workspace_with_options_and_layout(workspace, &options, layout)?
    };
    if no_interactive {
        dashboard::print_dashboard(&run.report, &run.config);
        let _ = dashboard::print_blockers(&run.report);
        println!("Reports: {}", run.output_dir.display());
    } else {
        let mut dashboard_config = run.config.clone();
        dashboard_config.output_dir = run.output_dir.display().to_string();
        dashboard::main_menu(workspace, &run.report, &dashboard_config)?;
    }
    if gate {
        enforce_gate(&run.report, &run.output_dir)?;
    }
    Ok(())
}
fn enforce_gate(
    report: &qa_model::QaReport,
    output_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let blocking = report
        .findings
        .iter()
        .filter(|f| matches!(f.severity, Severity::High | Severity::Critical))
        .count();
    let failed = report
        .evidence
        .iter()
        .filter(|e| matches!(e.status, EvidenceStatus::Failed | EvidenceStatus::Unavailable))
        .count();
    let coverage_missing = matches!(
        report.summary.coverage.status,
        EvidenceStatus::Partial | EvidenceStatus::Failed | EvidenceStatus::Unavailable
    );
    let mutation_missing = matches!(
        report.summary.mutation.status,
        EvidenceStatus::Failed | EvidenceStatus::Unavailable
    );
    let fuzz_failed = report
        .fuzz_targets
        .iter()
        .filter(|t| matches!(t.build_status, EvidenceStatus::Failed | EvidenceStatus::Unavailable))
        .count();
    let missing = usize::from(coverage_missing) + usize::from(mutation_missing) + fuzz_failed;
    if blocking > 0 || failed > 0 || missing > 0 {
        return Err(format!(
            "QA gate failed: {blocking} high/critical findings, {failed} failed/unavailable backend checks, and {missing} required partial/failed/unavailable coverage/mutation/fuzz evidence failures; inspect {} and {}",
            output_dir.join("summary.txt").display(),
            output_dir.join("report.json").display()
        )
        .into());
    }
    Ok(())
}
fn take_flag(args: &mut Vec<String>, flag: &str) -> bool {
    if let Some(i) = args.iter().position(|a| a == flag) {
        args.remove(i);
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qa_model::{CoverageSummary, FuzzSummary, MutationSummary, QaReport, SummaryMetrics};

    fn report() -> QaReport {
        QaReport {
            schema: 21,
            generated_unix_seconds: 0,
            workspace: ".".into(),
            profile: "strict".into(),
            summary: SummaryMetrics {
                health_score: 100.0,
                health_is_provisional: false,
                average_file_loc: 0.0,
                files_over_loc: 0,
                average_cc: 0.0,
                functions_over_cc: 0,
                average_crap: Some(0.0),
                functions_over_crap: Some(0),
                total_tests: 0,
                invalid_tests: 0,
                coverage: CoverageSummary {
                    percent: Some(100.0),
                    functions_below_threshold: Some(0),
                    source: None,
                    status: EvidenceStatus::Available,
                    ..CoverageSummary::default()
                },
                mutation: MutationSummary {
                    status: EvidenceStatus::Available,
                    caught: 1,
                    missed: 0,
                    timeout: 0,
                    unviable: 0,
                    score_percent: Some(100.0),
                    source: None,
                },
                fuzz: FuzzSummary {
                    target_count: 0,
                    critical_targets_missing: 0,
                    regression_artifacts: 0,
                    unpersisted_crashes: 0,
                    property_test_count: 0,
                    status: EvidenceStatus::Available,
                },
                duplicate_percent: 0.0,
                dead_code_percent: 0.0,
                high_findings: 0,
                critical_findings: 0,
            },
            files: vec![],
            functions: vec![],
            types: vec![],
            interfaces: vec![],
            mutations: vec![],
            fuzz_targets: vec![],
            duplicates: vec![],
            dead_items: vec![],
            evidence: vec![],
            findings: vec![],
        }
    }

    #[test]
    fn full_options_enables_every_dynamic_assurance_family() {
        let options = full_options();
        assert!(options.force_coverage);
        assert!(options.run_mutation);
        assert!(options.check_fuzz);
        assert!(options.run_sanitizers);
        assert!(options.run_concurrency);
        assert!(options.run_constant_time);
        assert!(options.run_differential);
        assert!(options.run_fault);
        assert!(options.run_mir);
        assert!(options.run_platform);
        assert!(options.run_hardware);
        assert!(options.run_performance);
    }

    #[test]
    fn command_option_tables_cover_every_assurance_route_exactly() {
        let coverage = command_options("coverage").unwrap();
        assert!(coverage.force_coverage);
        assert!(!coverage.run_mutation);

        let mutants = command_options("mutants").unwrap();
        assert!(mutants.run_mutation);
        assert!(!mutants.force_coverage);
        assert!(command_options("fuzz").unwrap().check_fuzz);
        assert!(command_options("concurrency").unwrap().run_concurrency);
        assert!(command_options("constant-time").unwrap().run_constant_time);
        assert!(command_options("sanitizers").unwrap().run_sanitizers);
        assert!(command_options("differential").unwrap().run_differential);
        assert!(command_options("fault").unwrap().run_fault);
        assert!(command_options("mir").unwrap().run_mir);
        assert!(command_options("platform").unwrap().run_platform);
        assert!(command_options("hardware").unwrap().run_hardware);
        assert!(command_options("performance").unwrap().run_performance);

        let baseline = command_options("performance-baseline").unwrap();
        assert!(baseline.run_performance);
        assert!(baseline.update_performance_baseline);
        assert!(command_options("hardening").unwrap().run_hardening);

        let full = command_options("full").unwrap();
        assert!(full.force_coverage);
        assert!(!full.run_release);
        assert!(!full.run_self_hardening);

        let release = command_options("release").unwrap();
        assert!(release.run_hardening);
        assert!(release.run_release);
        assert!(!release.run_self_hardening);

        let self_hardening = command_options("self-hardening").unwrap();
        assert!(self_hardening.run_hardening);
        assert!(self_hardening.run_release);
        assert!(self_hardening.run_self_hardening);
        assert!(command_options("unknown").is_none());
    }

    #[test]
    fn basic_and_help_command_tables_are_complete() {
        assert!(basic_action("doctor").is_some());
        assert!(basic_action("settings").is_some());
        assert!(basic_action("exceptions").is_some());
        assert!(basic_action("reports").is_none());
        assert!(is_help("--help"));
        assert!(is_help("-h"));
        assert!(is_help("help"));
        assert!(!is_help("full"));
    }

    #[test]
    fn take_flag_and_qa_prefix_normalization_are_exact() {
        let mut args = vec!["qa".into(), "full".into(), "--no-interactive".into(), "tail".into()];
        strip_qa_prefix(&mut args);
        assert_eq!(
            args.iter().map(String::as_str).collect::<Vec<_>>(),
            vec!["full", "--no-interactive", "tail"]
        );
        strip_qa_prefix(&mut args);
        assert_eq!(args.first().map(String::as_str), Some("full"));
        assert!(take_flag(&mut args, "--no-interactive"));
        assert_eq!(args.iter().map(String::as_str).collect::<Vec<_>>(), vec!["full", "tail"]);
        assert!(!take_flag(&mut args, "--missing"));
    }

    #[test]
    fn coverage_defaults_to_fresh_and_both_reuse_flags_select_existing_evidence() {
        let mut options = qa_sdk::RunOptions::default();
        assert!(options.force_coverage);
        apply_coverage_policy(&mut options, false);
        assert!(options.force_coverage);

        let mut existing = vec!["full".into(), "--existing-coverage".into()];
        assert!(existing_coverage_requested(&mut existing));
        assert_eq!(existing, vec!["full"]);
        apply_coverage_policy(&mut options, true);
        assert!(!options.force_coverage);

        let mut reuse = vec!["--reuse-coverage".into(), "release".into()];
        assert!(existing_coverage_requested(&mut reuse));
        assert_eq!(reuse, vec!["release"]);

        let mut neither = vec!["full".into()];
        assert!(!existing_coverage_requested(&mut neither));
        assert_eq!(neither, vec!["full"]);
    }

    #[test]
    fn non_interactive_is_default_and_interactive_is_explicit_opt_in() {
        let mut defaults = Vec::new();
        assert!(non_interactive(&mut defaults));

        let mut interactive = vec!["--interactive".to_string()];
        assert!(!non_interactive(&mut interactive));
        assert!(interactive.is_empty());

        let mut non_interactive_args = vec!["--no-interactive".to_string()];
        assert!(non_interactive(&mut non_interactive_args));
        assert!(non_interactive_args.is_empty());

        let mut both = vec!["--interactive".to_string(), "--no-interactive".to_string()];
        assert!(non_interactive(&mut both));
        assert!(both.is_empty());
    }

    #[test]
    fn terminal_mode_decisions_separate_auto_exit_from_live_tty_progress() {
        assert!(!non_interactive_mode(true, false));
        assert!(non_interactive_mode(true, true));
        assert!(non_interactive_mode(false, false));
        assert!(non_interactive_mode(false, true));

        assert!(!live_ui_enabled(false, false));
        assert!(!live_ui_enabled(false, true));
        assert!(!live_ui_enabled(true, false));
        assert!(live_ui_enabled(true, true));
    }

    #[test]
    fn gate_passes_clean_report_and_blocks_findings_backend_and_required_evidence_failures() {
        let clean = report();
        assert!(enforce_gate(&clean, Path::new("reports")).is_ok());

        let mut finding = report();
        finding.findings.push(qa_model::Finding {
            rule_id: "QA-X".into(),
            severity: Severity::High,
            message: "bad".into(),
            path: None,
            line: None,
            detail: None,
        });
        assert!(
            enforce_gate(&finding, Path::new("reports"))
                .unwrap_err()
                .to_string()
                .contains("1 high/critical")
        );

        let mut backend = report();
        backend.evidence.push(qa_model::EvidenceRecord {
            family: "SAN".into(),
            check: "address".into(),
            status: EvidenceStatus::Failed,
            source: None,
            detail: None,
        });
        assert!(
            enforce_gate(&backend, Path::new("reports"))
                .unwrap_err()
                .to_string()
                .contains("1 failed/unavailable")
        );

        let mut partial = report();
        partial.summary.coverage.status = EvidenceStatus::Partial;
        assert!(
            enforce_gate(&partial, Path::new("reports"))
                .unwrap_err()
                .to_string()
                .contains("1 required")
        );

        let mut missing = report();
        missing.summary.coverage.status = EvidenceStatus::Unavailable;
        missing.summary.mutation.status = EvidenceStatus::Failed;
        missing.fuzz_targets.push(qa_model::FuzzTargetEvidence {
            name: "fuzz".into(),
            path: "fuzz.rs".into(),
            line: 1,
            reaches_production: true,
            critical_targets: vec![],
            build_status: EvidenceStatus::Unavailable,
        });
        assert!(
            enforce_gate(&missing, Path::new("reports"))
                .unwrap_err()
                .to_string()
                .contains("3 required")
        );
    }
}
