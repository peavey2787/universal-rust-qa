use super::*;
use crate::test_support::{cleanup, discover, ids};
use qa_syntax::SourceInterface;

#[test]
fn error_secret_and_constant_time_rules_cover_critical_paths() {
    let (root, source) = discover(&[(
        "src/lib.rs",
        r#"
#[derive(Debug)]
#[qa_attr::secret]
struct SecretKey([u8; 32]);
#[qa_attr::critical_crypto]
fn crypto(secret: usize, table: &[u8]) { if secret > 0 { println!("secret={secret}"); } let _ = table[secret]; }
fn errors() { let _ = save(); let _ = save().map_err(|_| ()); }
fn save() -> Result<(),()> { Ok(()) }
#[derive(Debug)] struct E;
impl std::fmt::Display for E { fn fmt(&self, f:&mut std::fmt::Formatter<'_>)->std::fmt::Result { write!(f,"e") } }
impl std::error::Error for E {}
"#,
    )]);
    let mut findings = Vec::new();
    analyze(&source, &QaConfig::default(), &mut findings);
    let found = ids(&findings);
    for expected in [
        "QA-ERR-001",
        "QA-ERR-004",
        "QA-ERR-002",
        "QA-SECRET-002",
        "QA-ERR-003",
        "QA-CT-001",
        "QA-CT-002",
    ] {
        assert!(found.contains(&expected), "missing {expected}: {found:?}");
    }
    cleanup(&root);
}

#[test]
fn zeroize_source_chain_and_allow_policies_suppress_corresponding_findings() {
    let (root, source) = discover(&[(
        "src/lib.rs",
        r#"
#[derive(Zeroize, ZeroizeOnDrop)]
#[qa_attr::secret]
struct SecretKey([u8; 32]);
fn log(token: u8) { println!("token={token}"); }
#[derive(Debug)] struct E;
impl std::fmt::Display for E { fn fmt(&self, f:&mut std::fmt::Formatter<'_>)->std::fmt::Result { write!(f,"e") } }
impl std::error::Error for E { fn source(&self)->Option<&(dyn std::error::Error+'static)>{None} }
"#,
    )]);
    let mut config = QaConfig::default();
    config.errors.secret_logging = "allow".into();
    let mut findings = Vec::new();
    analyze(&source, &config, &mut findings);
    let found = ids(&findings);
    assert!(!found.contains(&"QA-SECRET-002"));
    assert!(!found.contains(&"QA-ERR-003"));
    assert!(!found.contains(&"QA-ERR-002"));
    cleanup(&root);
}

#[test]
fn identifier_and_sink_helpers_distinguish_strong_ambiguous_and_unrelated_names() {
    assert!(discarded_important_result("let _ = file.write(data);"));
    assert!(!discarded_important_result("let _ = calculate();"));
    assert!(logging_sink("tracing::info!(\"x\")"));
    assert!(!logging_sink("format!(\"x\")"));
    assert!(strong_secret_identifier("private_key = 1"));
    assert!(ambiguous_secret_identifier("seed = 1"));
    assert!(secret_identifier("token = 1"));
    assert!(!secret_identifier("monkey = 1"));
    assert_eq!(identifiers("API_Key + Token-value"), vec!["api_key", "token", "value"]);
}

#[test]
fn secret_and_constant_time_helpers_have_exact_positive_negative_and_line_semantics() {
    let (root, source) = discover(&[(
        "src/lib.rs",
        r#"
#[qa_attr::secret]
struct SecretKey([u8; 32]);
struct PublicKey([u8; 32]);
#[qa_attr::critical_crypto]
fn crypto(secret: usize, table: &[u8]) {
    if secret > 0 { work(); }
    let _ = table[secret];
}
fn plain(value: usize, table: &[u8]) { let _ = table[value]; }
fn work() {}
"#,
    )]);
    assert_eq!(secret_type_names(&source), vec!["SecretKey"]);

    let crypto = source.functions.iter().find(|function| function.name == "crypto").unwrap();
    let plain = source.functions.iter().find(|function| function.name == "plain").unwrap();
    let mut config = QaConfig::default();
    assert!(constant_time_candidate(crypto, &config));
    assert!(!constant_time_candidate(plain, &config));
    config.constant_time.enabled = false;
    assert!(!constant_time_candidate(crypto, &config));
    config.constant_time.enabled = true;

    let mut findings = Vec::new();
    check_secret_branch(crypto, &config, "if secret > 0 { work(); }", 3, &mut findings);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].rule_id, "QA-CT-001");
    assert_eq!(findings[0].line, Some(crypto.line + 3));
    check_secret_branch(crypto, &config, "if value > 0 { work(); }", 4, &mut findings);
    assert_eq!(findings.len(), 1);

    check_secret_index(crypto, &config, "let _ = table[secret];", 5, &mut findings);
    assert_eq!(findings.len(), 2);
    assert_eq!(findings[1].rule_id, "QA-CT-002");
    assert_eq!(findings[1].line, Some(crypto.line + 5));
    check_secret_index(crypto, &config, "let _ = table[value];", 6, &mut findings);
    assert_eq!(findings.len(), 2);
    cleanup(&root);
}

#[test]
fn secret_logging_and_formatting_checks_require_both_policy_and_real_secret_signal() {
    let (root, source) = discover(&[(
        "src/lib.rs",
        r#"
#[derive(Debug)]
#[qa_attr::secret]
struct SecretKey([u8; 32]);
fn log_named(value: SecretKey) { println!("value={:?}", value); }
fn log_strong(private_key: u8) { println!("{private_key}"); }
fn log_ambiguous(token: u8) { println!("{token}"); }
fn no_sink(private_key: u8) { let _ = private_key; }
"#,
    )]);
    let config = QaConfig::default();
    let names = secret_type_names(&source);

    for (name, expected_severity) in [
        ("log_named", Severity::Critical),
        ("log_strong", Severity::Critical),
        ("log_ambiguous", Severity::Medium),
    ] {
        let function = source.functions.iter().find(|function| function.name == name).unwrap();
        let mut findings = Vec::new();
        check_secret_logging(function, &config, &names, &sanitize(&function.source), &mut findings);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, expected_severity);
    }

    let no_sink = source.functions.iter().find(|function| function.name == "no_sink").unwrap();
    let mut findings = Vec::new();
    check_secret_logging(no_sink, &config, &names, &sanitize(&no_sink.source), &mut findings);
    assert!(findings.is_empty());

    let secret = source.types.iter().find(|ty| ty.name == "SecretKey").unwrap();
    check_secret_formatting(secret, &config, &mut findings);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, Severity::Critical);
    let mut relaxed = config.clone();
    relaxed.secrets.deny_debug_display = false;
    findings.clear();
    check_secret_formatting(secret, &relaxed, &mut findings);
    assert!(findings.is_empty());
    cleanup(&root);
}

#[test]
fn custom_error_source_detection_requires_an_error_impl_without_source_method() {
    let (root, source) = discover(&[(
        "src/lib.rs",
        r#"
struct A;
impl std::error::Error for A {}
struct B;
impl std::error::Error for B { fn source(&self)->Option<&(dyn std::error::Error+'static)>{None} }
struct C;
impl C { fn source(&self) {} }
"#,
    )]);
    let a =
        source.interfaces.iter().find(|interface| interface.name.contains("Error for A")).unwrap();
    let b =
        source.interfaces.iter().find(|interface| interface.name.contains("Error for B")).unwrap();
    let c = source.interfaces.iter().find(|interface| interface.name == "C").unwrap();
    assert!(custom_error_without_source(a));
    assert!(!custom_error_without_source(b));
    assert!(!custom_error_without_source(c));
    cleanup(&root);
}

#[test]
fn error_impl_detection_accepts_name_or_source_signal_independently() {
    let by_name = SourceInterface {
        path: "src/lib.rs".into(),
        name: "Error for Named".into(),
        line: 1,
        kind: "impl".into(),
        item_count: 0,
        source: "impl Named {}".into(),
    };
    let by_source = SourceInterface {
        path: "src/lib.rs".into(),
        name: "Named".into(),
        line: 1,
        kind: "impl".into(),
        item_count: 0,
        source: "impl std::error::Error for Named {}".into(),
    };
    assert!(custom_error_without_source(&by_name));
    assert!(custom_error_without_source(&by_source));
}

#[test]
fn secret_index_requires_both_indexing_and_a_secret_signal() {
    let (root, source) = discover(&[(
        "src/lib.rs",
        r#"
#[qa_attr::critical_crypto]
fn secret_without_index(private_key: usize) { let _ = private_key; }
#[qa_attr::critical_crypto]
fn index_without_secret(value: usize, table: &[u8]) { let _ = table[value]; }
#[qa_attr::critical_crypto]
fn secret_index(private_key: usize, table: &[u8]) { let _ = table[private_key]; }
"#,
    )]);
    let mut findings = Vec::new();
    analyze(&source, &QaConfig::default(), &mut findings);
    let ct = findings.iter().filter(|finding| finding.rule_id == "QA-CT-002").collect::<Vec<_>>();
    assert_eq!(ct.len(), 1);
    assert!(ct[0].message.contains("secret_index"));
    cleanup(&root);
}

#[test]
fn secret_index_helper_requires_indexing_and_secret_signal_independently() {
    let (root, source) = discover(&[(
        "src/lib.rs",
        "#[qa_attr::critical_crypto]\nfn crypto(private_key: usize, table: &[u8]) { let _ = (private_key, table); }\n",
    )]);
    let function = source.functions.iter().find(|function| function.name == "crypto").unwrap();
    let config = QaConfig::default();
    let mut findings = Vec::new();
    check_secret_index(function, &config, "let x = private_key;", 0, &mut findings);
    assert!(findings.is_empty());
    check_secret_index(function, &config, "let x = table[index];", 0, &mut findings);
    assert!(findings.is_empty());
    check_secret_index(function, &config, "let x = private_key[;", 0, &mut findings);
    assert!(findings.is_empty());
    check_secret_index(function, &config, "let x = private_key];", 0, &mut findings);
    assert!(findings.is_empty());
    check_secret_index(function, &config, "let x = table[private_key];", 0, &mut findings);
    assert_eq!(findings.len(), 1);
    cleanup(&root);
}
