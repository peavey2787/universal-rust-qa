use super::*;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(1);

fn temp_dir(name: &str) -> PathBuf {
    let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("urqa-mut-{name}-{}-{id}", std::process::id()));
    if path.exists() {
        fs::remove_dir_all(&path).unwrap();
    }
    fs::create_dir_all(&path).unwrap();
    path
}

fn fixture() -> Value {
    serde_json::json!({
        "outcomes": [
            {
                "summary": "CaughtMutant",
                "mutant": {"file": "src/a.rs", "line": 3, "description": "return 1 -> 0"}
            },
            {
                "summary": "MissedMutant",
                "mutant": {
                    "path": "src/b.rs",
                    "span": {"start": {"line": 7}},
                    "mutation": "delete branch"
                }
            },
            {"summary": "Timeout", "mutant": {"file": "src/c.rs", "line": 9}},
            {"summary": "Unviable", "mutant": {"file": "src/d.rs", "line": 11}},
            {"summary": "Unknown", "mutant": {"file": "src/e.rs"}}
        ]
    })
}

#[test]
fn mutation_command_always_exercises_workspace_and_all_features() {
    let mut config = QaConfig::default();
    config.mutation.timeout_seconds = 321;
    let output = Path::new("external-mutations");
    let args = mutation_args(&config, output);
    assert_eq!(
        &args[..8],
        [
            "mutants",
            "--output=external-mutations",
            "--no-shuffle",
            "--workspace",
            "--test-workspace=true",
            "--all-features",
            "--timeout",
            "321",
        ]
    );
    for pattern in SELF_HARDENING_EXCLUDE_RE.iter().chain(HOST_INAPPLICABLE_EXCLUDE_RE) {
        assert!(args.windows(2).any(|pair| pair[0] == "--exclude-re" && pair[1] == *pattern));
    }

    let local = mutation_args(&config, Path::new("workspace/mutants.out"));
    assert_eq!(local[1], "--output=workspace");
}

#[test]
fn parse_counts_outcomes_and_keeps_actionable_items() {
    let root = temp_dir("parse");
    let path = root.join("outcomes.json");
    fs::write(&path, serde_json::to_vec(&fixture()).unwrap()).unwrap();
    let evidence = parse(&path);
    assert_eq!(evidence.status, EvidenceStatus::Available);
    assert_eq!(
        (evidence.caught, evidence.missed, evidence.timeout, evidence.unviable),
        (1, 1, 1, 1)
    );
    assert_eq!(evidence.score_percent, Some(100.0 / 3.0));
    assert_eq!(evidence.items.len(), 2);
    assert_eq!(evidence.items[0].path.as_deref(), Some("src/b.rs"));
    assert_eq!(evidence.items[0].line, Some(7));
    assert_eq!(evidence.items[1].mutation, "mutation");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn modern_cargo_mutants_scenario_schema_is_actionable() {
    let outcome = serde_json::json!({
        "summary": "MissedMutant",
        "scenario": {
            "Mutant": {
                "name": "crates/qa-backends/src/mutation.rs:42:7: replace == with != in collect",
                "file": "crates/qa-backends/src/mutation.rs",
                "span": {"start": {"line": 42, "column": 7}},
                "replacement": "!="
            }
        }
    });
    let item = mutation_item(&outcome, "MissedMutant");
    assert_eq!(item.path.as_deref(), Some("crates/qa-backends/src/mutation.rs"));
    assert_eq!(item.line, Some(42));
    assert_eq!(
        item.mutation,
        "crates/qa-backends/src/mutation.rs:42:7: replace == with != in collect"
    );
}

#[test]
fn malformed_missing_and_disabled_evidence_have_precise_status() {
    let root = temp_dir("states");
    let path = root.join("bad.json");
    fs::write(&path, "not-json").unwrap();
    assert_eq!(parse(&path).status, EvidenceStatus::Failed);
    assert_eq!(parse(&root.join("missing.json")).status, EvidenceStatus::Failed);

    let mut config = QaConfig::default();
    let mutation_dir = root.join("mutations");
    config.mutation.mode = "off".into();
    assert_eq!(collect(&root, &config, &mutation_dir, false).status, EvidenceStatus::Disabled);

    config.mutation.mode = "existing".into();
    assert_eq!(collect(&root, &config, &mutation_dir, false).status, EvidenceStatus::Unavailable);
    let evidence_dir = mutation_dir.join("mutants.out");
    fs::create_dir_all(&evidence_dir).unwrap();
    fs::write(evidence_dir.join("outcomes.json"), serde_json::to_vec(&fixture()).unwrap()).unwrap();
    assert_eq!(collect(&root, &config, &mutation_dir, false).status, EvidenceStatus::Available);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn outcome_helpers_handle_nested_and_default_fields() {
    let value = serde_json::json!({
        "outcomes": [{"summary": "MissedMutant", "file": "x.rs", "line": 2}]
    });
    assert_eq!(outcomes(&value).count(), 1);
    assert_eq!(outcomes(&serde_json::json!({})).count(), 0);

    let mutant = serde_json::json!({
        "path": "x.rs",
        "span": {"start": {"line": 42}},
        "description": "change"
    });
    assert_eq!(mutation_path(&mutant).as_deref(), Some("x.rs"));
    assert_eq!(mutation_line(&mutant), Some(42));
    assert_eq!(mutation_description(&mutant), "change");
    assert_eq!(mutation_value(&mutant), &mutant);

    let mut evidence = MutationEvidence::default();
    for summary in ["CaughtMutant", "MissedMutant", "Timeout", "Unviable", "Other"] {
        increment_summary(&mut evidence, summary);
    }
    assert_eq!(
        (evidence.caught, evidence.missed, evidence.timeout, evidence.unviable),
        (1, 1, 1, 1)
    );
    finalize_score(&mut evidence);
    assert_eq!(evidence.score_percent, Some(100.0 / 3.0));
}

#[test]
fn mutation_command_and_missing_evidence_statuses_are_fail_closed() {
    let unavailable = classify_mutation_command(Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "cargo missing",
    )));
    assert_eq!(unavailable.unavailable.as_deref(), Some("cargo missing"));
    assert!(unavailable.error.is_none());
    assert!(unavailable.fallback.is_none());

    let missing_after_run = missing_evidence(true, Some("mutants failed".into()));
    assert_eq!(missing_after_run.status, EvidenceStatus::Failed);
    assert_eq!(missing_after_run.error.as_deref(), Some("mutants failed"));

    let missing_existing = missing_evidence(false, None);
    assert_eq!(missing_existing.status, EvidenceStatus::Unavailable);
    assert!(missing_existing.error.is_none());
}

#[test]
fn command_errors_are_attached_without_overwriting_parse_errors() {
    let available = MutationEvidence { status: EvidenceStatus::Available, ..Default::default() };
    let attached = attach_command_error(available, Some("cargo mutants exited 2".into()));
    assert_eq!(attached.error.as_deref(), Some("cargo mutants exited 2"));

    let parsed_failure = failed("invalid outcomes json".into());
    let preserved = attach_command_error(parsed_failure, Some("secondary command error".into()));
    assert_eq!(preserved.status, EvidenceStatus::Failed);
    assert_eq!(preserved.error.as_deref(), Some("invalid outcomes json"));
}

#[test]
fn empty_or_unscored_outcomes_do_not_invent_a_mutation_score() {
    let root = temp_dir("empty-score");
    let path = root.join("outcomes.json");
    fs::write(
        &path,
        serde_json::to_vec(&serde_json::json!({
            "outcomes": [
                {"summary": "Unviable", "mutant": {"file": "src/a.rs"}},
                {"summary": "Unknown", "mutant": {"file": "src/b.rs"}}
            ]
        }))
        .unwrap(),
    )
    .unwrap();

    let evidence = parse(&path);
    assert_eq!(evidence.status, EvidenceStatus::Available);
    assert_eq!(evidence.unviable, 1);
    assert_eq!(evidence.score_percent, None);
    assert!(evidence.items.is_empty());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn mutation_field_fallbacks_preserve_source_and_description_priority() {
    let direct = serde_json::json!({
        "file": "src/direct.rs",
        "line": 9,
        "name": "named mutation",
        "description": "lower priority",
        "mutation": "lower still",
        "replacement": "lowest"
    });
    assert_eq!(mutation_path(&direct).as_deref(), Some("src/direct.rs"));
    assert_eq!(mutation_line(&direct), Some(9));
    assert_eq!(mutation_description(&direct), "named mutation");

    let replacement_only = serde_json::json!({"replacement": "false"});
    assert_eq!(mutation_description(&replacement_only), "false");
    assert_eq!(mutation_description(&serde_json::json!({})), "mutation");
    assert_eq!(mutation_path(&serde_json::json!({})), None);
    assert_eq!(mutation_line(&serde_json::json!({})), None);
}

#[test]
fn mutation_command_classifies_process_success_failure_and_spawn_errors() {
    let root = temp_dir("command-status");
    let ok = super::super::process::run(&root, "rustc", &["--version".into()], &[]);
    let ok = classify_mutation_command(ok);
    assert!(ok.unavailable.is_none());
    assert!(ok.error.is_none());

    let mut quiet_output =
        super::super::process::run(&root, "rustc", &["--version".into()], &[]).unwrap();
    quiet_output.stdout.clear();
    quiet_output.stderr.clear();
    assert!(streams_are_blank(&quiet_output.stdout, &quiet_output.stderr));
    assert!(command_failure_detail(&quiet_output).starts_with("cargo-mutants exited with "));

    quiet_output.stdout = b"  \r\n".to_vec();
    quiet_output.stderr = b"\t".to_vec();
    assert!(streams_are_blank(&quiet_output.stdout, &quiet_output.stderr));

    let bad = super::super::process::run(
        &root,
        "rustc",
        &["--definitely-not-a-real-rustc-option".into()],
        &[],
    );
    let bad = classify_mutation_command(bad);
    assert!(bad.unavailable.is_none());
    assert!(bad.error.as_deref().is_some_and(|detail| detail.contains("stderr:")));

    let missing = super::super::process::run(
        &root,
        "definitely-not-a-real-universal-rust-qa-command",
        &[],
        &[],
    );
    let missing = classify_mutation_command(missing);
    assert!(missing.unavailable.as_deref().is_some_and(|detail| !detail.is_empty()));
    assert!(missing.error.is_none());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn load_evidence_preserves_command_failure_context_and_run_requirement() {
    let root = temp_dir("load-evidence");

    let absent_after_run = load_evidence(&root, true, Some("cargo mutants failed".into()), None);
    assert_eq!(absent_after_run.status, EvidenceStatus::Failed);
    assert_eq!(absent_after_run.error.as_deref(), Some("cargo mutants failed"));

    let evidence_dir = root.join("mutants.out");
    fs::create_dir_all(&evidence_dir).unwrap();
    fs::write(evidence_dir.join("outcomes.json"), serde_json::to_vec(&fixture()).unwrap()).unwrap();
    let parsed_with_command_error =
        load_evidence(&root, true, Some("survivors make cargo-mutants exit nonzero".into()), None);
    assert_eq!(parsed_with_command_error.status, EvidenceStatus::Available);
    assert_eq!(parsed_with_command_error.caught, 1);
    assert_eq!(
        parsed_with_command_error.error.as_deref(),
        Some("survivors make cargo-mutants exit nonzero")
    );
    fs::remove_dir_all(root).unwrap();
}

mod evidence_edges;
