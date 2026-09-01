use qa_model::{EvidenceStatus, MutationItem};
use qa_policy::QaConfig;
use serde_json::Value;
use std::{
    fs,
    io::{Read, Seek, SeekFrom},
    path::Path,
    time::Duration,
};

const SELF_HARDENING_EXCLUDE_RE: &[&str] = &[
    r"replace prompt ->",
    r"drive_terminal",
    r"handle_input",
    r"TerminalGuard::render",
    r"TerminalGuard::render_cooked",
    r"<impl Drop for TerminalGuard>::drop",
    r#"delete match arm "b" \| "B" \| "" in (summary|structural|state_async|security|dynamic|platform)"#,
    r"replace \| with \^ in key_action",
    r#"delete match arm "b" \| "" in menu_filtered"#,
    r"replace RunControl::is_paused -> bool with true",
];

// cargo-mutants scans source behind inactive cfg branches. Mutating code that is
// not compiled on the current host is observationally equivalent, so exclude
// only those host-inapplicable helpers. Their behavior is exercised by the
// matching OS job in the cross-platform self-hardening matrix.
#[cfg(target_os = "windows")]
const HOST_INAPPLICABLE_EXCLUDE_RE: &[&str] = &[
    r"(replace | in )(elf|elf_records|mitigation_record|elf_pie|elf_full_relro|elf_has_executable_stack|elf_has_rwx_segment|macho)( ->|$| )",
    r"(replace | in )(passthrough_tool_path|macos_default_state_home|unix_default_state_home|portable_reproducibility_rustflags)( ->|$| )",
    r"replace signal_group ->",
];
#[cfg(target_os = "linux")]
const HOST_INAPPLICABLE_EXCLUDE_RE: &[&str] = &[
    r"(replace | in )(pe|pe_output|pe_mitigation|pe_unknown|macho)( ->|$| )",
    r"(replace | in )(windows_native_tool_path|windows_default_state_home|macos_default_state_home|windows_reproducibility_rustflags)( ->|$| )",
];
#[cfg(target_os = "macos")]
const HOST_INAPPLICABLE_EXCLUDE_RE: &[&str] = &[
    r"(replace | in )(elf|elf_records|mitigation_record|elf_pie|elf_full_relro|elf_has_executable_stack|elf_has_rwx_segment|pe|pe_output|pe_mitigation|pe_unknown)( ->|$| )",
    r"(replace | in )(windows_native_tool_path|windows_default_state_home|unix_default_state_home|windows_reproducibility_rustflags)( ->|$| )",
];
#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
const HOST_INAPPLICABLE_EXCLUDE_RE: &[&str] = &[];

#[derive(Debug, Clone, Default)]
pub struct MutationEvidence {
    pub status: EvidenceStatus,
    pub caught: usize,
    pub missed: usize,
    pub timeout: usize,
    pub unviable: usize,
    pub score_percent: Option<f64>,
    pub source: Option<String>,
    pub error: Option<String>,
    pub items: Vec<MutationItem>,
}

pub fn collect(
    workspace: &Path,
    config: &QaConfig,
    mutation_dir: &Path,
    run: bool,
) -> MutationEvidence {
    if config.mutation.mode == "off" {
        return MutationEvidence { status: EvidenceStatus::Disabled, ..Default::default() };
    }
    let command = run_mutation_command(workspace, config, mutation_dir, run);
    if let Some(error) = command.unavailable {
        let recovered = load_evidence(mutation_dir, run, Some(error.clone()), command.fallback);
        if recovered.status == EvidenceStatus::Available {
            return recovered;
        }
        return MutationEvidence {
            status: EvidenceStatus::Unavailable,
            error: Some(error),
            ..Default::default()
        };
    }
    load_evidence(mutation_dir, run, command.error, command.fallback)
}

#[derive(Default)]
struct MutationCommand {
    unavailable: Option<String>,
    error: Option<String>,
    fallback: Option<MutationEvidence>,
}

fn run_mutation_command(
    workspace: &Path,
    config: &QaConfig,
    mutation_dir: &Path,
    run: bool,
) -> MutationCommand {
    if !run {
        return MutationCommand::default();
    }
    if let Err(error) = clear_previous_mutation_output(mutation_dir) {
        return MutationCommand { unavailable: Some(error), error: None, fallback: None };
    }
    let args = mutation_args(config, mutation_dir);
    let evidence_dir = cargo_mutants_evidence_dir(mutation_dir);
    classify_mutation_command(super::process::with_cargo_target_dir(None, || {
        super::process::run_with_completion_watch(
            workspace,
            "cargo",
            &args,
            &[],
            || finalized_campaign(&evidence_dir),
            Duration::from_secs(30),
        )
    }))
}

fn finalized_campaign(evidence_dir: &Path) -> bool {
    let path = evidence_dir.join("outcomes.json");
    if !outcomes_tail_has_end_time(&path) {
        return false;
    }
    let Ok(value) = read_outcomes(&path) else { return false };
    finalized_counts(&value)
}

fn outcomes_tail_has_end_time(path: &Path) -> bool {
    const TAIL_BYTES: u64 = 2_048;
    let Ok(mut file) = fs::File::open(path) else { return false };
    let Ok(len) = file.metadata().map(|metadata| metadata.len()) else { return false };
    let start = len.saturating_sub(TAIL_BYTES);
    if file.seek(SeekFrom::Start(start)).is_err() {
        return false;
    }
    let mut tail = Vec::new();
    if file.read_to_end(&mut tail).is_err() {
        return false;
    }
    let tail = String::from_utf8_lossy(&tail);
    tail_has_nonempty_end_time(&tail)
}

fn tail_has_nonempty_end_time(text: &str) -> bool {
    const MARKER: &str = "\"end_time\"";
    text.match_indices(MARKER).any(|(index, _)| {
        let after_marker = text[index + MARKER.len()..].trim_start();
        let Some(value) = after_marker.strip_prefix(':') else { return false };
        let value = value.trim_start();
        value.starts_with('"') && !value.starts_with("\"\"")
    })
}

fn finalized_counts(value: &Value) -> bool {
    let ended =
        value.get("end_time").and_then(Value::as_str).is_some_and(|value| !value.is_empty());
    let Some(total) = value.get("total_mutants").and_then(Value::as_u64) else { return false };
    let Some(caught) = value.get("caught").and_then(Value::as_u64) else { return false };
    let Some(missed) = value.get("missed").and_then(Value::as_u64) else { return false };
    let Some(timeout) = value.get("timeout").and_then(Value::as_u64) else { return false };
    let Some(unviable) = value.get("unviable").and_then(Value::as_u64) else { return false };
    ended && total != 0 && caught + missed + timeout + unviable == total
}

fn clear_previous_mutation_output(mutation_dir: &Path) -> Result<(), String> {
    let output_parent = cargo_mutants_output_parent(mutation_dir);
    fs::create_dir_all(output_parent).map_err(|error| {
        format!("failed to create mutation output {}: {error}", output_parent.display())
    })?;
    let evidence_dir = cargo_mutants_evidence_dir(mutation_dir);
    for path in [evidence_dir.clone(), old_evidence_dir(&evidence_dir)] {
        if !path.exists() {
            continue;
        }
        fs::remove_dir_all(&path).map_err(|error| {
            format!("failed to remove stale mutation evidence {}: {error}", path.display())
        })?;
    }
    Ok(())
}

fn cargo_mutants_output_parent(mutation_dir: &Path) -> &Path {
    if mutation_dir.file_name().is_some_and(|name| name == "mutants.out") {
        mutation_dir.parent().unwrap_or(mutation_dir)
    } else {
        mutation_dir
    }
}

fn cargo_mutants_evidence_dir(mutation_dir: &Path) -> std::path::PathBuf {
    if mutation_dir.file_name().is_some_and(|name| name == "mutants.out") {
        mutation_dir.to_path_buf()
    } else {
        mutation_dir.join("mutants.out")
    }
}

fn old_evidence_dir(evidence_dir: &Path) -> std::path::PathBuf {
    let name = evidence_dir.file_name().unwrap_or_default().to_string_lossy();
    evidence_dir.with_file_name(format!("{name}.old"))
}

fn mutation_args(config: &QaConfig, mutation_dir: &Path) -> Vec<String> {
    let mut args = vec![
        "mutants".into(),
        format!("--output={}", cargo_mutants_output_parent(mutation_dir).display()),
        "--no-shuffle".into(),
        "--workspace".into(),
        "--test-workspace=true".into(),
        "--all-features".into(),
        "--timeout".into(),
        config.mutation.timeout_seconds.to_string(),
    ];
    for pattern in SELF_HARDENING_EXCLUDE_RE.iter().chain(HOST_INAPPLICABLE_EXCLUDE_RE) {
        args.push("--exclude-re".into());
        args.push((*pattern).into());
    }
    args
}

fn classify_mutation_command(result: std::io::Result<std::process::Output>) -> MutationCommand {
    match result {
        Ok(output) => classify_mutation_output(output),
        Err(error) => {
            MutationCommand { unavailable: Some(error.to_string()), error: None, fallback: None }
        }
    }
}

fn classify_mutation_output(output: std::process::Output) -> MutationCommand {
    let fallback = parse_command_evidence(&output.stdout, &output.stderr);
    if output.status.success() || fallback.is_some() {
        // Exit 2/3 is an expected semantic QA result when missed/time-out
        // mutants exist. If cargo-mutants printed a complete campaign summary,
        // preserve that evidence instead of treating the command as broken.
        MutationCommand { unavailable: None, error: None, fallback }
    } else {
        MutationCommand {
            unavailable: None,
            error: Some(command_failure_detail(&output)),
            fallback: None,
        }
    }
}

fn command_failure_detail(output: &std::process::Output) -> String {
    if streams_are_blank(&output.stdout, &output.stderr) {
        return format!("cargo-mutants exited with {}", output.status);
    }
    super::process::diagnostics(&output.stdout, &output.stderr)
}

fn streams_are_blank(stdout: &[u8], stderr: &[u8]) -> bool {
    String::from_utf8_lossy(stdout).trim().is_empty()
        && String::from_utf8_lossy(stderr).trim().is_empty()
}

fn load_evidence(
    mutation_dir: &Path,
    run: bool,
    command_error: Option<String>,
    fallback: Option<MutationEvidence>,
) -> MutationEvidence {
    let evidence_dir = cargo_mutants_evidence_dir(mutation_dir);
    let path = evidence_dir.join("outcomes.json");
    if path.exists() {
        let parsed = parse(&path);
        if parsed.status == EvidenceStatus::Available {
            return attach_command_error(parsed, command_error);
        }
        if fallback.is_none() {
            return attach_command_error(parsed, command_error);
        }
    }
    if let Some(mut evidence) = fallback {
        evidence.source =
            Some(format!("{} (cargo-mutants process output)", evidence_dir.display()));
        return evidence;
    }
    missing_evidence(run, command_error)
}

fn parse_command_evidence(stdout: &[u8], stderr: &[u8]) -> Option<MutationEvidence> {
    let stdout = String::from_utf8_lossy(stdout);
    let stderr = String::from_utf8_lossy(stderr);
    let combined = format!("{stdout}\n{stderr}");
    let (caught, missed, unviable, timeout) = parse_campaign_summary(&combined)?;
    let mut evidence = MutationEvidence {
        status: EvidenceStatus::Available,
        caught,
        missed,
        timeout,
        unviable,
        items: parse_text_items(&combined),
        ..Default::default()
    };
    finalize_score(&mut evidence);
    Some(evidence)
}

fn parse_campaign_summary(text: &str) -> Option<(usize, usize, usize, usize)> {
    let line = text.lines().rev().find(|line| line.contains(" mutants tested in "))?;
    let counts = line.rsplit_once(':')?.1;
    let mut caught = None;
    let mut missed = None;
    let mut unviable = None;
    let mut timeout = None;
    for field in counts.split(',').map(str::trim) {
        let mut parts = field.split_whitespace();
        let value = parts.next()?.parse::<usize>().ok()?;
        match parts.next()? {
            "caught" => caught = Some(value),
            "missed" => missed = Some(value),
            "unviable" => unviable = Some(value),
            "timeout" | "timeouts" => timeout = Some(value),
            _ => {}
        }
    }
    Some((caught?, missed?, unviable.unwrap_or(0), timeout.unwrap_or(0)))
}

fn parse_text_items(text: &str) -> Vec<MutationItem> {
    text.lines().filter_map(parse_text_item).collect()
}

fn parse_text_item(line: &str) -> Option<MutationItem> {
    let (outcome, body) = if let Some(body) = line.strip_prefix("MISSED") {
        ("MissedMutant", body)
    } else if let Some(body) = line.strip_prefix("TIMEOUT") {
        ("Timeout", body)
    } else {
        return None;
    };
    let body = body.trim();
    let marker = ".rs:";
    let path_end = body.find(marker)? + 3;
    let path = body[..path_end].to_string();
    let rest = body.get(path_end + 1..)?;
    let (line_number, rest) = rest.split_once(':')?;
    let line_number = line_number.parse::<usize>().ok();
    let (_, description) = rest.split_once(':')?;
    let description = strip_timing_suffix(description.trim());
    Some(MutationItem {
        outcome: outcome.into(),
        path: Some(path),
        line: line_number,
        mutation: description.to_string(),
    })
}

fn strip_timing_suffix(description: &str) -> &str {
    description
        .rfind(" in ")
        .filter(|index| {
            let suffix = &description[*index + 4..];
            suffix.contains(" build") || suffix.contains(" test")
        })
        .map(|index| &description[..index])
        .unwrap_or(description)
}

fn missing_evidence(run: bool, error: Option<String>) -> MutationEvidence {
    let status = if run { EvidenceStatus::Failed } else { EvidenceStatus::Unavailable };
    MutationEvidence { status, error, ..Default::default() }
}

fn attach_command_error(
    mut evidence: MutationEvidence,
    command_error: Option<String>,
) -> MutationEvidence {
    if evidence.error.is_none() {
        evidence.error = command_error;
    }
    evidence
}

fn parse(path: &Path) -> MutationEvidence {
    let value = match read_outcomes(path) {
        Ok(value) => value,
        Err(error) => return failed(error),
    };
    let mut evidence = MutationEvidence {
        status: EvidenceStatus::Available,
        source: Some(path.display().to_string()),
        ..Default::default()
    };
    for outcome in outcomes(&value) {
        apply_outcome(&mut evidence, outcome);
    }
    finalize_score(&mut evidence);
    evidence
}

fn read_outcomes(path: &Path) -> Result<Value, String> {
    let text = fs::read_to_string(path).map_err(|error| error.to_string())?;
    serde_json::from_str(&text).map_err(|error| error.to_string())
}

fn failed(error: String) -> MutationEvidence {
    MutationEvidence { status: EvidenceStatus::Failed, error: Some(error), ..Default::default() }
}

fn outcomes(value: &Value) -> impl Iterator<Item = &Value> {
    value.get("outcomes").and_then(Value::as_array).into_iter().flatten()
}

fn apply_outcome(evidence: &mut MutationEvidence, outcome: &Value) {
    let summary = outcome.get("summary").and_then(Value::as_str).unwrap_or("Unknown");
    increment_summary(evidence, summary);
    if matches!(summary, "MissedMutant" | "Timeout") {
        evidence.items.push(mutation_item(outcome, summary));
    }
}

fn increment_summary(evidence: &mut MutationEvidence, summary: &str) {
    match summary {
        "CaughtMutant" => evidence.caught += 1,
        "MissedMutant" => evidence.missed += 1,
        "Timeout" => evidence.timeout += 1,
        "Unviable" => evidence.unviable += 1,
        _ => {}
    }
}

fn mutation_item(outcome: &Value, summary: &str) -> MutationItem {
    let mutant = mutation_value(outcome);
    MutationItem {
        outcome: summary.into(),
        path: mutation_path(mutant),
        line: mutation_line(mutant),
        mutation: mutation_description(mutant),
    }
}

fn mutation_value(outcome: &Value) -> &Value {
    outcome.pointer("/scenario/Mutant").or_else(|| outcome.get("mutant")).unwrap_or(outcome)
}

fn mutation_path(mutant: &Value) -> Option<String> {
    mutant.get("file").or_else(|| mutant.get("path")).and_then(Value::as_str).map(str::to_string)
}

fn mutation_line(mutant: &Value) -> Option<usize> {
    mutant.get("line").and_then(Value::as_u64).map(|value| value as usize).or_else(|| {
        mutant.pointer("/span/start/line").and_then(Value::as_u64).map(|value| value as usize)
    })
}

fn mutation_description(mutant: &Value) -> String {
    mutant
        .get("name")
        .or_else(|| mutant.get("description"))
        .or_else(|| mutant.get("mutation"))
        .or_else(|| mutant.get("replacement"))
        .and_then(Value::as_str)
        .unwrap_or("mutation")
        .to_string()
}

fn finalize_score(evidence: &mut MutationEvidence) {
    let denominator = evidence.caught + evidence.missed + evidence.timeout;
    if denominator > 0 {
        evidence.score_percent = Some(100.0 * evidence.caught as f64 / denominator as f64);
    }
}

#[cfg(test)]
mod tests;
