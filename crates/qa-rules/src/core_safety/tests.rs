use super::*;
use crate::test_support::{cleanup, discover, ids};

#[test]
fn production_safety_rules_detect_panics_math_parser_channels_leaks_and_host_paths() {
    let (root, source) = discover(&[(
        "src/lib.rs",
        r#"
#[qa_attr::critical_math]
fn math(a: u64, b: u64) -> u64 { a + b }
#[qa_attr::critical_parser]
fn parse(mut r: impl std::io::Read) { let mut v=Vec::new(); r.read_to_end(&mut v).unwrap(); }
fn bad() { let x: Result<(),()> = Err(()); x.expect("x"); panic!("x"); async_channel::unbounded::<u8>(); std::mem::forget(vec![1]); }
fn host() { let _ = include_str!("/home/person/file"); }
"#,
    )]);
    let mut findings = Vec::new();
    analyze(&source, &QaConfig::default(), &mut findings);
    let found = ids(&findings);
    for expected in [
        "QA-MATH-001",
        "QA-PARSE-002",
        "QA-SAFE-001",
        "QA-SAFE-002",
        "QA-SAFE-003",
        "QA-RES-001",
        "QA-ALLOC-001",
        "QA-ENV-002",
    ] {
        assert!(found.contains(&expected), "missing {expected}: {found:?}");
    }
    cleanup(&root);
}

#[test]
fn allow_and_bounded_paths_suppress_corresponding_findings() {
    let (root, source) = discover(&[(
        "src/lib.rs",
        r#"
#[qa_attr::critical_math]
fn math(a:u64,b:u64)->u64 { a.checked_add(b).unwrap_or(0) }
#[qa_attr::critical_parser]
fn parse(mut r: impl std::io::Read) { let mut v=Vec::new(); r.take(32).read_to_end(&mut v).ok(); }
fn allowed() { async_channel::unbounded::<u8>(); std::mem::forget(vec![1]); }
"#,
    )]);
    let mut config = QaConfig::default();
    config.resources.unbounded_channels = "allow".into();
    config.alloc.explicit_leaks = "allow".into();
    config.environment.detect_absolute_host_paths = false;
    let mut findings = Vec::new();
    analyze(&source, &config, &mut findings);
    let found = ids(&findings);
    assert!(!found.contains(&"QA-MATH-001"));
    assert!(!found.contains(&"QA-PARSE-002"));
    assert!(!found.contains(&"QA-RES-001"));
    assert!(!found.contains(&"QA-ALLOC-001"));
    cleanup(&root);
}

#[test]
fn host_and_bound_helpers_cover_all_recognized_forms() {
    assert!(contains_host_path("/Users/a/project"));
    assert!(contains_host_path(r"C:\Users\a\project"));
    assert!(!contains_host_path("relative/project"));
    let code = strip_comments_preserve_strings(
        "let x = include_str!(\"/home/user/key\"); // /Users/comment-only\n/* C:\\Users\\comment */",
    );
    assert!(contains_host_path(&code));
    assert!(!code.contains("/Users/comment-only"));
    assert!(!code.contains(r"C:\Users\comment"));
    for source in ["MAX_PACKET", "reader.take(1)", "BoundedVec", "size_limit"] {
        assert!(bound(source));
    }
    assert!(!bound("read_to_end(&mut out)"));
}

#[test]
fn host_path_rule_ignores_comment_only_paths() {
    let (root, source) = discover(&[(
        "src/lib.rs",
        "fn safe() {} // /home/comment-only\n/* C:\\Users\\comment-only */\n",
    )]);
    let mut findings = Vec::new();
    analyze(&source, &QaConfig::default(), &mut findings);
    assert!(!ids(&findings).contains(&"QA-ENV-002"));
    cleanup(&root);
}

#[test]
fn panic_math_and_host_path_conjunctions_have_independent_negative_cases() {
    let (root, source) = discover(&[(
        "src/lib.rs",
        r#"
fn panic_code() { panic!("boom"); }
#[qa_attr::critical_math]
fn critical_plain(a:u64,b:u64)->u64 { a + b }
fn ordinary_math(a:u64,b:u64)->u64 { a + b }
#[qa_attr::critical_math]
fn critical_no_math(a:u64)->u64 { a }
#[qa_attr::critical_math]
fn critical_checked(a:u64,b:u64)->u64 { a.checked_add(b).unwrap_or(0) + 0 }
fn paths() {
    let _a = include_str!("/home/first/file");
    let _b = include_str!("/home/second/file");
}
"#,
    )]);
    let panic_code =
        source.functions.iter().find(|function| function.name == "panic_code").unwrap();
    let mut config = QaConfig::default();
    config.safety.panic = "allow".into();
    let mut findings = Vec::new();
    check_panic_hygiene(panic_code, &config, &sanitize(&panic_code.source), &mut findings);
    assert!(findings.is_empty());

    config.safety.panic = "deny".into();
    let critical_plain =
        source.functions.iter().find(|function| function.name == "critical_plain").unwrap();
    check_critical_math(critical_plain, &config, &sanitize(&critical_plain.source), &mut findings);
    assert_eq!(ids(&findings), vec!["QA-MATH-001"]);

    findings.clear();
    for name in ["ordinary_math", "critical_no_math", "critical_checked"] {
        let function = source.functions.iter().find(|function| function.name == name).unwrap();
        check_critical_math(function, &config, &sanitize(&function.source), &mut findings);
    }
    assert!(findings.is_empty());

    config.safety.critical_checked_arithmetic = false;
    check_critical_math(critical_plain, &config, &sanitize(&critical_plain.source), &mut findings);
    assert!(findings.is_empty());

    findings.clear();
    analyze_host_paths(&source, &QaConfig::default(), &mut findings);
    let host_lines = findings
        .iter()
        .filter(|finding| finding.rule_id == "QA-ENV-002")
        .map(|finding| finding.line.unwrap())
        .collect::<Vec<_>>();
    assert_eq!(host_lines.len(), 2);
    assert_eq!(host_lines[1], host_lines[0] + 1);
    cleanup(&root);
}

#[test]
fn host_path_findings_preserve_the_exact_source_line() {
    let (root, source) = discover(&[(
        "src/lib.rs",
        "fn before() {}\nfn host() { let _ = r\"C:\\Users\\fixture\"; }\n",
    )]);
    let mut findings = Vec::new();
    analyze_host_paths(&source, &QaConfig::default(), &mut findings);
    let finding = findings.iter().find(|finding| finding.rule_id == "QA-ENV-002").unwrap();
    assert_eq!(finding.line, Some(2));
    cleanup(&root);
}
