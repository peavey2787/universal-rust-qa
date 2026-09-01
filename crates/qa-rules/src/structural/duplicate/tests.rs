use super::*;
use crate::test_support::{cleanup, discover, workspace};
use qa_syntax::SourceFile;

#[test]
fn normalization_and_hash_are_deterministic() {
    assert_eq!(norm(" let  x = 1; // comment"), "let x = 1;");
    assert_eq!(hash(b"abc"), 16_654_208_175_385_433_931);
    assert_ne!(hash(b"abc"), hash(b"abd"));
}

#[test]
fn duplicate_analysis_requires_real_repeated_windows() {
    let block = "let alpha = value + 1;\nlet beta = alpha * 2;\nlet gamma = beta + alpha;\nlet delta = gamma * beta;\n";
    let left = format!("fn left(value:i32){{\n{block}}}\n");
    let right = format!("fn right(value:i32){{\n{block}}}\n");
    let (root, source) = discover(&[("src/a.rs", &left), ("src/b.rs", &right)]);
    let mut config = QaConfig::default();
    config.duplicates.minimum_loc = 4;
    config.duplicates.minimum_nodes = 4;
    let mut findings = Vec::new();
    let (groups, covered) = analyze(&source, &config, &mut findings);
    assert!(!groups.is_empty());
    assert_eq!(covered, 8);
    assert!(findings.iter().any(|f| f.rule_id == "QA-DUP-002"));
    cleanup(&root);
}

#[test]
fn short_files_do_not_create_duplicate_groups() {
    let (root, source) = discover(&[("src/lib.rs", "fn one() {}\n")]);
    let mut findings = Vec::new();
    let (groups, covered) = analyze(&source, &QaConfig::default(), &mut findings);
    assert!(groups.is_empty());
    assert_eq!(covered, 0);
    cleanup(&root);
}

#[test]
fn exact_window_and_filter_boundaries_are_inclusive_only_where_documented() {
    let exact = [
        "aaaaaaaaaaaaaaaaaaaa",
        "bbbbbbbbbbbbbbbbbbbb",
        "cccccccccccccccccccc",
        "ddddddddddddddddd",
    ]
    .join("\n");
    assert_eq!(exact.len(), 80);
    let left = format!("{exact}\n");
    let right = format!("{exact}\n");
    let root = workspace(&[]);
    let source = WorkspaceSource {
        root: root.clone(),
        files: vec![
            SourceFile { path: root.join("src/a.rs"), text: left, module_depth: 0 },
            SourceFile { path: root.join("src/b.rs"), text: right, module_depth: 0 },
        ],
        ..WorkspaceSource::default()
    };
    let mut config = QaConfig::default();
    config.duplicates.minimum_loc = 4;
    config.duplicates.minimum_nodes = 4;
    let mut findings = Vec::new();
    let (groups, covered) = analyze(&source, &config, &mut findings);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].logical_lines, 4);
    assert_eq!(groups[0].occurrences.len(), 2);
    assert!(groups[0].occurrences.iter().all(|span| span.line == 1));
    assert_eq!(covered, 8);
    cleanup(&root);
}

#[test]
fn either_short_text_or_too_few_nodes_is_sufficient_to_reject_a_window() {
    let short_many_nodes = "a b c d\ne f g h\ni j k l\nm n o p\n";
    let long_few_nodes =
        format!("{}\n{}\n{}\n{}\n", "a".repeat(30), "b".repeat(30), "c".repeat(30), "d".repeat(30));
    for (name, block, minimum_nodes) in
        [("short", short_many_nodes.to_string(), 4usize), ("few-nodes", long_few_nodes, 5usize)]
    {
        let (root, source) = discover(&[("src/a.rs", &block), ("src/b.rs", &block)]);
        let mut config = QaConfig::default();
        config.duplicates.minimum_loc = 4;
        config.duplicates.minimum_nodes = minimum_nodes;
        let mut findings = Vec::new();
        let (groups, covered) = analyze(&source, &config, &mut findings);
        assert!(groups.is_empty(), "{name} window should be rejected");
        assert_eq!(covered, 0);
        cleanup(&root);
    }
}

#[test]
fn three_distinct_occurrences_are_not_discarded() {
    let block = concat!(
        "let alpha = source + one;\n",
        "let beta = alpha + two;\n",
        "let gamma = beta + three;\n",
        "let delta = gamma + four;\n",
    );
    let root = workspace(&[]);
    let source = WorkspaceSource {
        root: root.clone(),
        files: ["a", "b", "c"]
            .into_iter()
            .map(|name| SourceFile {
                path: root.join(format!("src/{name}.rs")),
                text: block.to_string(),
                module_depth: 0,
            })
            .collect(),
        ..WorkspaceSource::default()
    };
    let mut config = QaConfig::default();
    config.duplicates.minimum_loc = 4;
    config.duplicates.minimum_nodes = 4;
    let mut findings = Vec::new();
    let (groups, covered) = analyze(&source, &config, &mut findings);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].occurrences.len(), 3);
    assert_eq!(covered, 12);
    cleanup(&root);
}
