use super::*;
use std::path::Path;

#[test]
fn cargo_mutants_text_fallback_preserves_complete_campaign_counts_and_items() {
    let text = b"Found 2352 mutants to test\n\
MISSED   crates/a/src/lib.rs:12:7: replace == with != in check in 1s build + 3s test\nTIMEOUT  crates/b/src/lib.rs:9:2: replace += with *= in loop_body in 1s build + 120s test\n2352 mutants tested in 5h: 342 missed, 1769 caught, 232 unviable, 9 timeouts\n";
    let evidence = parse_command_evidence(text, b"").unwrap();
    assert_eq!(evidence.status, EvidenceStatus::Available);
    assert_eq!(
        (evidence.caught, evidence.missed, evidence.unviable, evidence.timeout),
        (1769, 342, 232, 9)
    );
    assert_eq!(evidence.score_percent, Some(100.0 * 1769.0 / 2120.0));
    assert_eq!(evidence.items.len(), 2);
    assert_eq!(evidence.items[0].path.as_deref(), Some("crates/a/src/lib.rs"));
    assert_eq!(evidence.items[0].line, Some(12));
    assert_eq!(evidence.items[0].mutation, "replace == with != in check");
    assert_eq!(evidence.items[1].outcome, "Timeout");
    assert_eq!(evidence.items[1].mutation, "replace += with *= in loop_body");
}

#[test]
fn cargo_mutants_text_parsers_reject_incomplete_summaries_and_preserve_singular_timeout() {
    assert_eq!(
        parse_campaign_summary(
            "10 mutants tested in 1m: 7 caught, 2 missed, 0 unviable, 1 timeout"
        ),
        Some((7, 2, 0, 1))
    );
    assert_eq!(
        parse_campaign_summary(
            "10 mutants tested in 1m: 2 missed, 1 timeout, 7 caught, 0 unviable"
        ),
        Some((7, 2, 0, 1))
    );
    assert_eq!(parse_campaign_summary("10 mutants tested in 1m: 2 missed, 8 unviable"), None);
    assert_eq!(parse_campaign_summary("baseline failed"), None);

    let missed = parse_text_item(
        "MISSED   crates/a/src/lib.rs:12:7: replace == with != in check in 1s build + 3s test",
    )
    .unwrap();
    assert_eq!(missed.outcome, "MissedMutant");
    assert_eq!(missed.path.as_deref(), Some("crates/a/src/lib.rs"));
    assert_eq!(missed.line, Some(12));
    assert_eq!(missed.mutation, "replace == with != in check");

    let timeout = parse_text_item(
        "TIMEOUT  crates/b/src/lib.rs:9:2: replace += with *= in loop_body in 120s test",
    )
    .unwrap();
    assert_eq!(timeout.outcome, "Timeout");
    assert_eq!(timeout.line, Some(9));
    assert_eq!(timeout.mutation, "replace += with *= in loop_body");
    assert!(parse_text_item("CAUGHT crates/a/src/lib.rs:1:1: mutation").is_none());
    assert!(parse_text_item("MISSED not-a-rust-path").is_none());
    assert_eq!(strip_timing_suffix("replace x in 2s build"), "replace x");
    assert_eq!(strip_timing_suffix("replace x in function"), "replace x in function");
}

#[test]
fn missing_outcomes_uses_completed_command_fallback_instead_of_discarding_results() {
    let root = temp_dir("fallback-evidence");
    let fallback = MutationEvidence {
        status: EvidenceStatus::Available,
        caught: 9,
        missed: 1,
        score_percent: Some(90.0),
        ..Default::default()
    };
    let evidence = load_evidence(&root, true, None, Some(fallback));
    assert_eq!(evidence.status, EvidenceStatus::Available);
    assert_eq!((evidence.caught, evidence.missed), (9, 1));
    assert!(
        evidence
            .source
            .as_deref()
            .is_some_and(|source| source.contains("cargo-mutants process output"))
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn incomplete_command_output_is_not_misclassified_as_mutation_evidence() {
    assert!(parse_command_evidence(b"Found 12 mutants to test\nFAILED baseline", b"").is_none());
    assert!(parse_text_item("CAUGHT crates/a/src/lib.rs:1:1: mutation").is_none());
    assert_eq!(strip_timing_suffix("replace x in function"), "replace x in function");
}

#[test]
fn cargo_mutants_output_parent_and_evidence_directory_match_cli_contract() {
    let local = Path::new("workspace/mutants.out");
    assert_eq!(cargo_mutants_output_parent(local), Path::new("workspace"));
    assert_eq!(cargo_mutants_evidence_dir(local), local);

    let external = Path::new("state/mutations");
    assert_eq!(cargo_mutants_output_parent(external), external);
    assert_eq!(cargo_mutants_evidence_dir(external), external.join("mutants.out"));
    assert_eq!(old_evidence_dir(&external.join("mutants.out")), external.join("mutants.out.old"));
}

#[test]
fn previous_mutation_output_is_removed_before_a_fresh_campaign() {
    let root = temp_dir("clear-stale");

    let local = root.join("mutants.out");
    fs::create_dir_all(&local).unwrap();
    fs::write(local.join("outcomes.json"), b"stale").unwrap();
    fs::create_dir_all(root.join("mutants.out.old")).unwrap();
    clear_previous_mutation_output(&local).unwrap();
    assert!(!local.exists());
    assert!(!root.join("mutants.out.old").exists());

    let external = root.join("mutations");
    let external_evidence = external.join("mutants.out");
    fs::create_dir_all(&external_evidence).unwrap();
    fs::write(external_evidence.join("outcomes.json"), b"stale").unwrap();
    clear_previous_mutation_output(&external).unwrap();
    assert!(!external_evidence.exists());
    assert!(external.is_dir());

    clear_previous_mutation_output(&external).unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn collect_reports_unavailable_when_mutation_output_parent_cannot_be_created() {
    let root = temp_dir("collect-unavailable");
    let blocker = root.join("blocked-output");
    fs::write(&blocker, b"not a directory").unwrap();
    let config = QaConfig::default();
    let evidence = collect(&root, &config, &blocker, true);
    assert_eq!(evidence.status, EvidenceStatus::Unavailable);
    assert!(evidence.error.as_deref().is_some_and(|error| error.contains("mutation output")));
    assert_eq!(
        (evidence.caught, evidence.missed, evidence.timeout, evidence.unviable),
        (0, 0, 0, 0)
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn load_evidence_prefers_valid_json_and_parse_records_its_exact_source() {
    let root = temp_dir("load-valid");
    let evidence_dir = root.join("mutants.out");
    fs::create_dir_all(&evidence_dir).unwrap();
    let path = evidence_dir.join("outcomes.json");
    fs::write(&path, serde_json::to_vec(&fixture()).unwrap()).unwrap();

    let evidence = load_evidence(&root, true, None, None);
    let expected_source = path.display().to_string();
    assert_eq!(evidence.status, EvidenceStatus::Available);
    assert_eq!(evidence.source.as_deref(), Some(expected_source.as_str()));
    assert_eq!((evidence.caught, evidence.missed), (1, 1));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn timing_suffix_stripping_handles_build_test_and_combined_timings_exactly() {
    assert_eq!(strip_timing_suffix("replace x in 1s build + 3s test"), "replace x");
    assert_eq!(strip_timing_suffix("replace x in 120s test"), "replace x");
    assert_eq!(strip_timing_suffix("replace x in 4s build"), "replace x");
    assert_eq!(
        strip_timing_suffix("replace x in helper in function"),
        "replace x in helper in function"
    );
}

#[test]
fn malformed_json_uses_available_process_fallback_instead_of_failed_parse() {
    let root = temp_dir("malformed-fallback");
    let evidence_dir = root.join("mutants.out");
    fs::create_dir_all(&evidence_dir).unwrap();
    fs::write(evidence_dir.join("outcomes.json"), b"{not-json").unwrap();
    let fallback = MutationEvidence {
        status: EvidenceStatus::Available,
        caught: 9,
        missed: 1,
        score_percent: Some(90.0),
        ..Default::default()
    };
    let evidence = load_evidence(&root, true, None, Some(fallback));
    assert_eq!(evidence.status, EvidenceStatus::Available);
    assert_eq!((evidence.caught, evidence.missed), (9, 1));
    assert!(evidence.source.as_deref().is_some_and(|source| source.contains("process output")));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn timing_suffix_can_begin_at_the_start_without_underflow() {
    assert_eq!(strip_timing_suffix(" in 1s build"), "");
}

#[test]
fn finalized_campaign_requires_end_time_and_complete_exact_counts() {
    let complete = serde_json::json!({
        "total_mutants": 8,
        "caught": 5,
        "missed": 1,
        "timeout": 1,
        "unviable": 1,
        "end_time": "2026-08-31T02:29:03Z"
    });
    assert!(finalized_counts(&complete));

    let mut unfinished = complete.clone();
    unfinished.as_object_mut().unwrap().remove("end_time");
    assert!(!finalized_counts(&unfinished));

    let mut partial = complete.clone();
    partial["caught"] = serde_json::json!(4);
    assert!(!finalized_counts(&partial));

    let mut empty = complete.clone();
    empty["total_mutants"] = serde_json::json!(0);
    empty["caught"] = serde_json::json!(0);
    empty["missed"] = serde_json::json!(0);
    empty["timeout"] = serde_json::json!(0);
    empty["unviable"] = serde_json::json!(0);
    assert!(!finalized_counts(&empty));
}

#[test]
fn completed_disk_evidence_survives_post_campaign_process_cleanup_error() {
    let root = temp_dir("completed-cleanup-error");
    let evidence_dir = root.join("mutants.out");
    fs::create_dir_all(&evidence_dir).unwrap();
    fs::write(evidence_dir.join("outcomes.json"), serde_json::to_vec(&fixture()).unwrap()).unwrap();
    let evidence = load_evidence(
        &evidence_dir,
        true,
        Some("output pipes remained open after finalized campaign".into()),
        None,
    );
    assert_eq!(evidence.status, EvidenceStatus::Available);
    assert!(evidence.error.as_deref().unwrap().contains("output pipes remained open"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn finalized_campaign_uses_tail_marker_before_parsing_complete_counts() {
    let root = temp_dir("finalized-tail");
    let evidence_dir = root.join("mutants.out");
    fs::create_dir_all(&evidence_dir).unwrap();
    let path = evidence_dir.join("outcomes.json");
    fs::write(
        &path,
        serde_json::to_vec(&serde_json::json!({
            "outcomes": [],
            "total_mutants": 2,
            "missed": 1,
            "caught": 1,
            "timeout": 0,
            "unviable": 0,
            "end_time": null
        }))
        .unwrap(),
    )
    .unwrap();
    assert!(!outcomes_tail_has_end_time(&path));
    assert!(!finalized_campaign(&evidence_dir));
    assert!(!tail_has_nonempty_end_time(r#"{ "end_time" : null }"#));
    assert!(!tail_has_nonempty_end_time(r#"{ "end_time" : "" }"#));
    assert!(tail_has_nonempty_end_time(r#"{ "end_time" : "2026-08-31T02:29:03Z" }"#));

    fs::write(
        &path,
        serde_json::to_vec(&serde_json::json!({
            "outcomes": [],
            "total_mutants": 2,
            "missed": 1,
            "caught": 1,
            "timeout": 0,
            "unviable": 0,
            "end_time": "2026-08-31T02:29:03Z"
        }))
        .unwrap(),
    )
    .unwrap();
    assert!(outcomes_tail_has_end_time(&path));
    assert!(finalized_campaign(&evidence_dir));
    fs::remove_dir_all(root).unwrap();
}
