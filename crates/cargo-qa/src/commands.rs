use super::{CommandResult, doctor, exceptions, settings};
use std::path::Path;

pub(super) type BasicAction = fn(&Path) -> CommandResult;

const BASIC_ACTIONS: [(&str, BasicAction); 3] =
    [("doctor", doctor::run), ("settings", settings::menu), ("exceptions", exceptions::menu)];

pub(super) fn basic_action(command: &str) -> Option<BasicAction> {
    BASIC_ACTIONS.iter().find(|(name, _)| *name == command).map(|(_, action)| *action)
}

pub(super) fn command_options(command: &str) -> Option<qa_sdk::RunOptions> {
    assurance_options(command)
        .or_else(|| analysis_options(command))
        .or_else(|| suite_options(command))
}

pub(super) fn assurance_options(command: &str) -> Option<qa_sdk::RunOptions> {
    let options = match command {
        "coverage" => qa_sdk::RunOptions::default(),
        "mutants" => qa_sdk::RunOptions { run_mutation: true, ..focused_options() },
        "fuzz" => qa_sdk::RunOptions { check_fuzz: true, ..focused_options() },
        "concurrency" => qa_sdk::RunOptions { run_concurrency: true, ..focused_options() },
        "constant-time" => qa_sdk::RunOptions { run_constant_time: true, ..focused_options() },
        "sanitizers" => qa_sdk::RunOptions { run_sanitizers: true, ..focused_options() },
        "differential" => qa_sdk::RunOptions { run_differential: true, ..focused_options() },
        _ => return None,
    };
    Some(options)
}

pub(super) fn analysis_options(command: &str) -> Option<qa_sdk::RunOptions> {
    let options = match command {
        "fault" => qa_sdk::RunOptions { run_fault: true, ..focused_options() },
        "mir" => qa_sdk::RunOptions { run_mir: true, ..focused_options() },
        "platform" => qa_sdk::RunOptions { run_platform: true, ..focused_options() },
        "hardware" => qa_sdk::RunOptions { run_hardware: true, ..focused_options() },
        "performance" => qa_sdk::RunOptions { run_performance: true, ..focused_options() },
        "performance-baseline" => qa_sdk::RunOptions {
            run_performance: true,
            update_performance_baseline: true,
            ..focused_options()
        },
        "hardening" => qa_sdk::RunOptions { run_hardening: true, ..focused_options() },
        _ => return None,
    };
    Some(options)
}

fn focused_options() -> qa_sdk::RunOptions {
    qa_sdk::RunOptions::existing_coverage()
}

pub(super) fn suite_options(command: &str) -> Option<qa_sdk::RunOptions> {
    let mut options = full_options();
    match command {
        "full" => {}
        "release" => {
            options.run_hardening = true;
            options.run_release = true;
        }
        "self-hardening" => {
            options.run_hardening = true;
            options.run_release = true;
            options.run_self_hardening = true;
        }
        _ => return None,
    }
    Some(options)
}

pub(super) fn is_help(command: &str) -> bool {
    matches!(command, "--help" | "-h" | "help")
}

pub(super) fn full_options() -> qa_sdk::RunOptions {
    qa_sdk::RunOptions {
        run_mutation: true,
        check_fuzz: true,
        run_sanitizers: true,
        run_concurrency: true,
        run_constant_time: true,
        run_differential: true,
        run_fault: true,
        run_mir: true,
        run_platform: true,
        run_hardware: true,
        run_performance: true,
        ..Default::default()
    }
}
