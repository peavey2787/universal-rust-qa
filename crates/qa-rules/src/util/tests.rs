use super::*;
use qa_model::Severity;

#[test]
fn sanitize_masks_strings_comments_and_escapes_but_preserves_code_shape() {
    let source = "let x = \"panic!(\\\"hidden\\\")\"; // unwrap()\nvalue.unwrap();\n";
    let clean = sanitize(source);
    assert!(!clean.contains("hidden"));
    assert!(!clean.contains("// unwrap"));
    assert!(clean.contains("value.unwrap()"));
    assert_eq!(clean.lines().count(), source.lines().count());
}

#[test]
fn sanitize_state_machine_preserves_exact_code_and_line_shape() {
    assert_eq!(sanitize("a//b\nc"), "a   \nc");
    assert_eq!(sanitize("a\"x\"b"), "a   b");
    assert_eq!(sanitize("a\"x\\\"y\"b"), "a      b");
}

#[test]
fn attributes_and_policy_severity_use_documented_matching() {
    let attrs = ["qa_attr :: critical_parser".to_string()];
    assert!(has_attr(&attrs, "critical_parser"));
    assert!(!has_attr(&attrs, "hot_path"));
    assert_eq!(policy_severity("deny"), Severity::High);
    assert_eq!(policy_severity("DENY"), Severity::High);
    assert_eq!(policy_severity("warn"), Severity::Medium);
}

#[test]
fn comment_stripper_preserves_strings_and_line_structure() {
    let source =
        "let a = \"/home/user/file\"; // /Users/comment\n/* C:\\Users\\block */ let b = 1;\n";
    let clean = strip_comments_preserve_strings(source);
    assert!(clean.contains("/home/user/file"));
    assert!(!clean.contains("/Users/comment"));
    assert!(!clean.contains(r"C:\Users\block"));
    assert_eq!(clean.lines().count(), source.lines().count());

    let nested = r#"let url = "http://example"; /* outer /* /home/nested */ block */ let c = 1;"#;
    let nested_clean = strip_comments_preserve_strings(nested);
    assert!(nested_clean.contains("http://example"));
    assert!(!nested_clean.contains("/home/nested"));
    assert!(nested_clean.contains("let c = 1;"));

    let escaped = r#"let text = "quote: \" // still string"; // trailing comment"#;
    let escaped_clean = strip_comments_preserve_strings(escaped);
    assert!(escaped_clean.contains("// still string"));
    assert!(!escaped_clean.contains("trailing comment"));
}

#[test]
fn comment_stripper_operator_and_nested_depth_boundaries_are_exact() {
    assert_eq!(strip_comments_preserve_strings("a/b"), "a/b");
    assert_eq!(strip_comments_preserve_strings("/*x*/y"), "     y");

    let nested = strip_comments_preserve_strings("/* outer /* inner */ still */ tail");
    assert!(!nested.contains("inner"));
    assert!(!nested.contains("still"));
    assert!(nested.ends_with(" tail"));

    let mut chars = "/rest".chars().peekable();
    let mut out = String::new();
    mask_pair(&mut chars, &mut out);
    assert_eq!(out, "  ");
    assert_eq!(chars.next(), Some('r'));
}
