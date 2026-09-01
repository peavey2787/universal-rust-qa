use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(1);

fn temp_dir(name: &str) -> PathBuf {
    let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("urqa-diff-{name}-{}-{id}", std::process::id()));
    if path.exists() {
        fs::remove_dir_all(&path).unwrap();
    }
    fs::create_dir_all(&path).unwrap();
    path
}

fn target(reference: &str, candidate: &str, equivalence: &str) -> DifferentialTarget {
    DifferentialTarget {
        name: "codec".into(),
        reference_command: reference.into(),
        candidate_command: candidate.into(),
        corpus: "corpus".into(),
        equivalence: equivalence.into(),
    }
}

fn outcome(success: bool, stdout: &[u8]) -> Option<Outcome> {
    Some(Outcome { success, stdout: stdout.to_vec(), stderr: Vec::new() })
}

#[test]
fn disabled_identical_and_pending_targets_report_expected_status() {
    let root = temp_dir("status");
    let config = QaConfig::default();
    assert_eq!(
        run(&root, &config, &root.join("qa-out"), false)[0].status,
        EvidenceStatus::Disabled
    );

    let mut config = QaConfig::default();
    config.differential.enabled = true;
    config.differential.target = vec![target("same", "same", "exact")];
    let records = run(&root, &config, &root.join("qa-out"), false);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].status, EvidenceStatus::Failed);

    config.differential.target = vec![target("reference", "candidate", "exact")];
    let records = run(&root, &config, &root.join("qa-out"), false);
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].status, EvidenceStatus::Available);
    assert_eq!(records[1].status, EvidenceStatus::Unknown);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn missing_corpus_fails_before_commands_execute() {
    let root = temp_dir("missing");
    let mut config = QaConfig::default();
    config.differential.enabled = true;
    config.differential.target = vec![target("reference", "candidate", "exact")];
    let records = run(&root, &config, &root.join("qa-out"), true);
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].status, EvidenceStatus::Available);
    assert_eq!(records[1].status, EvidenceStatus::Failed);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn equivalence_modes_compare_success_and_output_precisely() {
    let mut t = target("r", "c", "exact");
    assert!(equivalent(&t, &outcome(true, b"abc"), &outcome(true, b"abc")));
    assert!(!equivalent(&t, &outcome(true, b"abc"), &outcome(true, b"abd")));
    assert!(!equivalent(&t, &outcome(true, b"abc"), &outcome(false, b"abc")));
    assert!(!equivalent(&t, &None, &outcome(true, b"abc")));

    t.equivalence = "trimmed".into();
    assert!(equivalent(&t, &outcome(true, b" abc\n"), &outcome(true, b"abc")));

    t.equivalence = "canonical-json".into();
    assert!(equivalent(
        &t,
        &outcome(true, br#"{"a":1,"b":2}"#),
        &outcome(true, br#"{"b":2,"a":1}"#),
    ));
    assert!(!equivalent(&t, &outcome(true, b"{"), &outcome(true, b"{}")));

    t.equivalence = "unknown".into();
    assert!(!equivalent(&t, &outcome(true, b"x"), &outcome(true, b"x")));
}

#[test]
fn corpus_persistence_hash_and_encoding_are_deterministic() {
    let root = temp_dir("persist");
    let corpus = root.join("corpus");
    fs::create_dir_all(&corpus).unwrap();
    fs::write(corpus.join("b.bin"), b"b").unwrap();
    fs::write(corpus.join("a.bin"), b"a").unwrap();
    fs::create_dir_all(corpus.join("nested")).unwrap();
    let files = corpus_files(&corpus).unwrap();
    assert_eq!(files.len(), 2);
    assert!(corpus_files(&root.join("absent")).is_err());

    assert_eq!(trim(b"  hi\r\n"), b"hi");
    assert_eq!(json(br#"{"x":1}"#).unwrap()["x"], 1);
    assert!(json(b"not json").is_none());
    assert_eq!(hex(&[0, 1, 0xab, 0xff]), "0001abff");
    assert_eq!(fnv1a(b"hello"), 0xa430d84680aabd0b);

    let config = QaConfig::default();
    let t = target("r", "c", "exact");
    let case = corpus.join("a.bin");
    let reference = outcome(true, b"ok");
    let candidate = outcome(false, b"bad");
    persist(&root.join("qa-out"), &t, &case, b"input", &reference, &candidate).unwrap();
    let out = root.join(&config.output_dir).join("differential/codec");
    assert_eq!(fs::read_dir(out).unwrap().count(), 1);
    assert!(outcome_json(&reference).is_object());
    assert!(outcome_json(&None).is_null());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn pipe_executes_a_stdin_filter() {
    let root = temp_dir("pipe");
    #[cfg(windows)]
    let command = "more";
    #[cfg(not(windows))]
    let command = "cat";
    let result = pipe(command, &root, b"payload").unwrap();
    assert!(result.success);
    assert!(String::from_utf8_lossy(&result.stdout).contains("payload"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn execute_cases_counts_and_persists_every_real_divergence() {
    let root = temp_dir("execute-cases");
    let corpus = root.join("corpus");
    fs::create_dir_all(&corpus).unwrap();
    let a = corpus.join("a.bin");
    let b = corpus.join("b.bin");
    fs::write(&a, b"alpha").unwrap();
    fs::write(&b, b"beta").unwrap();

    #[cfg(windows)]
    let commands = ("more", "exit /B 1");
    #[cfg(not(windows))]
    let commands = ("cat", "false");
    let target = target(commands.0, commands.1, "exact");
    let config = QaConfig::default();
    let stats = execute_cases(&root, &root.join("qa-out"), &target, &[a, b]);
    assert_eq!(stats.executed, 2);
    assert_eq!(stats.divergences, 2);
    assert_eq!(stats.persisted, 2);
    let persisted = root.join(&config.output_dir).join("differential/codec");
    assert_eq!(fs::read_dir(persisted).unwrap().count(), 2);
    fs::remove_dir_all(root).unwrap();
}
