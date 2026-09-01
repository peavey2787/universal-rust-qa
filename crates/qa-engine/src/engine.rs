mod findings;
mod report;

use qa_model::{EvidenceRecord, EvidenceStatus, QaReport};
use qa_policy::QaConfig;
use qa_rules::analyze;
use qa_syntax::discover;
use std::path::{Path, PathBuf};

use crate::RunControl;
use findings::{apply_coverage, apply_mutation_findings};
#[cfg(test)]
use findings::{crap, crap_finding, floor_percent, production_crap};

#[derive(Debug, Clone)]
pub struct RunOptions {
    pub force_coverage: bool,
    pub run_mutation: bool,
    pub check_fuzz: bool,
    pub run_sanitizers: bool,
    pub run_concurrency: bool,
    pub run_constant_time: bool,
    pub run_differential: bool,
    pub run_fault: bool,
    pub run_mir: bool,
    pub run_platform: bool,
    pub run_hardware: bool,
    pub run_performance: bool,
    pub update_performance_baseline: bool,
    pub run_hardening: bool,
    pub run_release: bool,
    pub run_self_hardening: bool,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            force_coverage: true,
            run_mutation: false,
            check_fuzz: false,
            run_sanitizers: false,
            run_concurrency: false,
            run_constant_time: false,
            run_differential: false,
            run_fault: false,
            run_mir: false,
            run_platform: false,
            run_hardware: false,
            run_performance: false,
            update_performance_baseline: false,
            run_hardening: false,
            run_release: false,
            run_self_hardening: false,
        }
    }
}

impl RunOptions {
    pub fn existing_coverage() -> Self {
        Self { force_coverage: false, ..Self::default() }
    }
}

pub fn run(workspace: &Path, config: &QaConfig) -> QaReport {
    run_with_options(workspace, config, &RunOptions::default())
}

pub const RUN_CATEGORY_COUNT: usize = 18;

#[derive(Debug, Clone)]
pub struct RunPaths {
    pub artifact_root: PathBuf,
    pub coverage_dir: PathBuf,
    pub mutation_dir: PathBuf,
    pub cargo_target_dir: Option<PathBuf>,
}

impl RunPaths {
    pub fn local(workspace: &Path, config: &QaConfig) -> Self {
        let artifact_root = workspace.join(&config.output_dir);
        Self {
            coverage_dir: artifact_root.clone(),
            mutation_dir: workspace.join("mutants.out"),
            artifact_root,
            cargo_target_dir: None,
        }
    }
}

pub fn run_with_options(workspace: &Path, config: &QaConfig, options: &RunOptions) -> QaReport {
    let paths = RunPaths::local(workspace, config);
    run_with_options_and_paths(workspace, config, options, &paths)
}

pub fn run_with_options_and_paths(
    workspace: &Path,
    config: &QaConfig,
    options: &RunOptions,
    paths: &RunPaths,
) -> QaReport {
    qa_backends::process::with_cargo_target_dir(paths.cargo_target_dir.as_deref(), || {
        run_internal(workspace, config, options, paths, None)
    })
}

pub fn run_with_progress(
    workspace: &Path,
    config: &QaConfig,
    options: &RunOptions,
    control: &RunControl,
) -> QaReport {
    let paths = RunPaths::local(workspace, config);
    run_with_progress_and_paths(workspace, config, options, &paths, control)
}

pub fn run_with_progress_and_paths(
    workspace: &Path,
    config: &QaConfig,
    options: &RunOptions,
    paths: &RunPaths,
    control: &RunControl,
) -> QaReport {
    qa_backends::process::with_cargo_target_dir(paths.cargo_target_dir.as_deref(), || {
        run_internal(workspace, config, options, paths, Some(control))
    })
}

macro_rules! run_phase {
    ($progress:expr, $name:expr, $operation:expr) => {{
        match $progress {
            Some(control) => control.category($name, || $operation),
            None => $operation,
        }
    }};
}

fn run_internal(
    workspace: &Path,
    config: &QaConfig,
    options: &RunOptions,
    paths: &RunPaths,
    progress: Option<&RunControl>,
) -> QaReport {
    let source = run_phase!(progress, "Source discovery", discover(workspace));
    let mut rules = run_phase!(progress, "Static analysis", analyze(&source, config));
    let output_dir = &paths.artifact_root;
    let empty_coverage = qa_backends::coverage::CoverageEvidence::default();
    let empty_mutation = qa_backends::mutation::MutationEvidence::default();
    refresh_progress(progress, config, &rules, &empty_coverage, &empty_mutation, &[]);

    let coverage = run_phase!(
        progress,
        "Coverage",
        qa_backends::coverage::collect(
            workspace,
            config,
            &paths.coverage_dir,
            options.force_coverage,
        )
    );
    apply_coverage(&mut rules, config, &coverage);
    refresh_progress(progress, config, &rules, &coverage, &empty_mutation, &[]);

    let mutation = run_phase!(progress, "Mutation", {
        if should_skip_mutation_after_coverage(options, &coverage) {
            skipped_mutation_after_coverage(&coverage)
        } else {
            qa_backends::mutation::collect(
                workspace,
                config,
                &paths.mutation_dir,
                options.run_mutation,
            )
        }
    });
    apply_mutation_findings(&mut rules, config, &mutation);
    refresh_progress(progress, config, &rules, &coverage, &mutation, &[]);

    let targets = rules.fuzz_targets.iter().map(|target| target.name.clone()).collect::<Vec<_>>();
    let fuzz = run_phase!(
        progress,
        "Fuzz",
        qa_backends::fuzz::check(workspace, config, &targets, options.check_fuzz)
    );
    apply_fuzz_status(&mut rules, &fuzz);
    let mut evidence =
        vec![coverage_record(&coverage), mutation_record(&mutation), fuzz_record(&rules, &fuzz)];
    refresh_progress(progress, config, &rules, &coverage, &mutation, &evidence);

    let dynamic = DynamicEvidenceContext {
        workspace,
        config,
        options,
        progress,
        coverage: &coverage,
        mutation: &mutation,
        artifact_root: output_dir,
    };
    collect_dynamic_evidence(&dynamic, &mut rules, &mut evidence);
    run_phase!(progress, "Finalize", {
        let exceptions =
            qa_policy::apply_exceptions(workspace, config, std::mem::take(&mut rules.findings));
        rules.findings = exceptions.findings;
    });
    refresh_progress(progress, config, &rules, &coverage, &mutation, &evidence);
    let report = report::build_report(workspace, config, rules, coverage, mutation, evidence);
    finish_progress(progress);
    report
}

fn should_skip_mutation_after_coverage(
    options: &RunOptions,
    coverage: &qa_backends::coverage::CoverageEvidence,
) -> bool {
    options.force_coverage
        && options.run_mutation
        && matches!(coverage.status, EvidenceStatus::Failed | EvidenceStatus::Unavailable)
}

fn skipped_mutation_after_coverage(
    coverage: &qa_backends::coverage::CoverageEvidence,
) -> qa_backends::mutation::MutationEvidence {
    qa_backends::mutation::MutationEvidence {
        error: Some(format!(
            "mutation skipped because required coverage evidence was {:?}",
            coverage.status
        )),
        ..Default::default()
    }
}

fn finish_progress(progress: Option<&RunControl>) {
    if let Some(control) = progress {
        control.finish();
    }
}

fn apply_fuzz_status(rules: &mut qa_rules::RuleOutput, fuzz: &qa_backends::fuzz::FuzzBackend) {
    for target in &mut rules.fuzz_targets {
        if let Some(status) = fuzz.targets.get(&target.name) {
            target.build_status = status.clone();
        }
    }
}

struct DynamicEvidenceContext<'a> {
    workspace: &'a Path,
    config: &'a QaConfig,
    options: &'a RunOptions,
    progress: Option<&'a RunControl>,
    coverage: &'a qa_backends::coverage::CoverageEvidence,
    mutation: &'a qa_backends::mutation::MutationEvidence,
    artifact_root: &'a Path,
}

fn collect_dynamic_evidence(
    context: &DynamicEvidenceContext<'_>,
    rules: &mut qa_rules::RuleOutput,
    evidence: &mut Vec<EvidenceRecord>,
) {
    evidence.push(run_phase!(
        context.progress,
        "Concurrency",
        qa_backends::loom::run(context.workspace, context.config, context.options.run_concurrency)
    ));
    refresh_dynamic_progress(context, rules, evidence);
    evidence.push(run_phase!(
        context.progress,
        "Constant-time",
        qa_backends::constant_time::run(
            context.workspace,
            context.config,
            context.options.run_constant_time
        )
    ));
    refresh_dynamic_progress(context, rules, evidence);
    evidence.extend(run_phase!(
        context.progress,
        "Sanitizers",
        qa_backends::sanitizer::run(
            context.workspace,
            context.config,
            context.options.run_sanitizers
        )
    ));
    refresh_dynamic_progress(context, rules, evidence);
    evidence.extend(run_phase!(
        context.progress,
        "Differential",
        qa_backends::differential::run(
            context.workspace,
            context.config,
            context.artifact_root,
            context.options.run_differential
        )
    ));
    refresh_dynamic_progress(context, rules, evidence);
    evidence.extend(run_phase!(
        context.progress,
        "Fault injection",
        qa_backends::fault::run(
            context.workspace,
            context.config,
            context.artifact_root,
            context.options.run_fault,
        )
    ));
    refresh_dynamic_progress(context, rules, evidence);
    let mir = run_phase!(
        context.progress,
        "MIR",
        qa_backends::mir::run(
            context.workspace,
            context.config,
            context.artifact_root,
            context.options.run_mir,
        )
    );
    evidence.extend(mir.records);
    rules.findings.extend(mir.findings);
    refresh_dynamic_progress(context, rules, evidence);
    evidence.extend(run_phase!(
        context.progress,
        "Platform",
        qa_backends::platform::run(context.workspace, context.config, context.options.run_platform)
    ));
    refresh_dynamic_progress(context, rules, evidence);
    evidence.extend(run_phase!(
        context.progress,
        "Hardware",
        qa_backends::hardware::run(context.workspace, context.config, context.options.run_hardware)
    ));
    refresh_dynamic_progress(context, rules, evidence);
    evidence.extend(run_phase!(
        context.progress,
        "Performance",
        qa_backends::performance::run(
            context.workspace,
            context.config,
            context.options.run_performance,
            context.options.update_performance_baseline,
            &rules.functions
        )
    ));
    refresh_dynamic_progress(context, rules, evidence);
    evidence.extend(run_phase!(
        context.progress,
        "Hardening",
        qa_backends::hardening::run(
            context.workspace,
            context.config,
            context.options.run_hardening
        )
    ));
    refresh_dynamic_progress(context, rules, evidence);
    evidence.extend(run_phase!(
        context.progress,
        "Release",
        qa_backends::release::run(
            context.workspace,
            context.config,
            context.artifact_root,
            context.options.run_release,
        )
    ));
    refresh_dynamic_progress(context, rules, evidence);
    evidence.extend(run_phase!(
        context.progress,
        "Self-hardening",
        qa_backends::self_hardening::run(
            context.workspace,
            context.config,
            context.options.run_self_hardening,
            &qa_rules::rule_registry()
        )
    ));
    refresh_dynamic_progress(context, rules, evidence);
}

fn refresh_dynamic_progress(
    context: &DynamicEvidenceContext<'_>,
    rules: &qa_rules::RuleOutput,
    evidence: &[EvidenceRecord],
) {
    refresh_progress(
        context.progress,
        context.config,
        rules,
        context.coverage,
        context.mutation,
        evidence,
    );
}

fn refresh_progress(
    progress: Option<&RunControl>,
    config: &QaConfig,
    rules: &qa_rules::RuleOutput,
    coverage: &qa_backends::coverage::CoverageEvidence,
    mutation: &qa_backends::mutation::MutationEvidence,
    evidence: &[EvidenceRecord],
) {
    if let Some(control) = progress {
        control.update_summary(
            report::build_summary(config, rules, coverage, mutation, evidence),
            rules.findings.len(),
            evidence.len(),
        );
    }
}

mod evidence;
use evidence::*;

#[cfg(test)]
use report::{
    HealthInputs, functions_below_coverage, functions_over_cc, health_score, optional_average,
    optional_count_over,
};

#[cfg(test)]
mod tests;
