use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(1);

fn temp_dir(name: &str) -> PathBuf {
    let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("urqa-syntax-{name}-{}-{id}", std::process::id()));
    if path.exists() {
        fs::remove_dir_all(&path).unwrap();
    }
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn discover_collects_functions_types_interfaces_modules_and_calls() {
    let root = temp_dir("discover");
    fs::create_dir_all(root.join("src/deep")).unwrap();
    fs::write(
        root.join("src/lib.rs"),
        r#"
#[derive(Debug)]
pub struct Packet { pub a: u32, b: Vec<u8> }

#[qa_attr::critical_state]
pub enum State {
    Init,
    #[qa_attr::terminal]
    Done(u32),
}

pub trait Worker { fn work(&self); fn defaulted(&self) {} }

impl Worker for Packet {
    fn work(&self) { helper(); std::mem::drop(1u8); }
}

mod inner {
    #[test]
    fn unit_test() { assert_eq!(1 + 1, 2); }
    pub async unsafe extern "C" fn exported<T>(x: T, y: u32) where T: Copy {
        super::helper();
        if true { return; }
    }
}

fn helper() {}
"#,
    )
    .unwrap();
    fs::write(root.join("src/deep/more.rs"), "pub fn deeper() {}\n").unwrap();

    let source = discover(&root);
    assert_eq!(source.parse_findings.len(), 0);
    assert_eq!(source.files.len(), 2);
    assert!(
        source
            .functions
            .iter()
            .any(|function| function.qualified_name.contains("Worker for Packet::work"))
    );
    let exported = source.functions.iter().find(|function| function.name == "exported").unwrap();
    assert!(exported.is_public);
    assert!(exported.is_async);
    assert!(exported.is_unsafe);
    assert_eq!(exported.abi.as_deref(), Some("C"));
    assert_eq!(exported.parameters, 2);
    assert_eq!(exported.generic_parameters, 1);
    assert!(exported.calls.iter().any(|call| call == "super::helper"));
    assert!(!exported.calls.iter().any(|call| call == "if"));

    let test = source.functions.iter().find(|function| function.name == "unit_test").unwrap();
    assert!(test.is_test);
    assert!(source.modules.iter().any(|module| module.name == "inner"));

    let packet = source.types.iter().find(|ty| ty.name == "Packet").unwrap();
    assert_eq!(packet.kind, "struct");
    assert_eq!(packet.field_count, 2);
    assert!(packet.is_public);
    assert!(packet.field_types.iter().any(|ty| ty.contains("Vec")));

    let state = source.types.iter().find(|ty| ty.name == "State").unwrap();
    assert_eq!(state.kind, "enum");
    assert_eq!(state.variant_count, 2);
    assert_eq!(state.variant_names, vec!["Init", "Done"]);
    assert_eq!(state.terminal_variants, vec!["Done"]);

    assert!(
        source
            .interfaces
            .iter()
            .any(|interface| interface.kind == "trait" && interface.item_count == 2)
    );
    assert!(
        source
            .interfaces
            .iter()
            .any(|interface| interface.kind == "impl" && interface.item_count == 1)
    );
    assert!(
        source
            .files
            .iter()
            .any(|file| file.path.ends_with("deep/more.rs") && file.module_depth >= 1)
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn discover_reports_parse_errors_and_excludes_generated_or_fixture_trees() {
    let root = temp_dir("parse");
    for directory in ["src", "target", "qa-out", "mutants.out", "vendor", "fixtures", ".git"] {
        fs::create_dir_all(root.join(directory)).unwrap();
    }
    fs::write(root.join("src/bad.rs"), "fn broken( {\n").unwrap();
    for directory in ["target", "qa-out", "mutants.out", "vendor", "fixtures", ".git"] {
        fs::write(root.join(directory).join("ignored.rs"), "fn ignored() {}\n").unwrap();
    }
    let source = discover(&root);
    assert_eq!(source.files.len(), 0);
    assert_eq!(source.parse_findings.len(), 1);
    assert_eq!(source.parse_findings[0].rule_id, "QA-SYNTAX-001");
    assert!(excluded(Path::new("root/target/file.rs")));
    assert!(!excluded(Path::new("root/src/file.rs")));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn lexical_helpers_handle_boundaries_and_deduplicate_calls() {
    assert_eq!(qual("", "name"), "name");
    assert_eq!(qual("outer", "name"), "outer::name");
    assert_eq!(lines("a\nb\nc\nd", 2, 3), "b\nc");
    assert_eq!(lines("a\nb", 1, 20), "a\nb");

    let parsed: syn::File = syn::parse_str(
        r#"#[test] #[qa_attr::critical] fn f() { alpha(); alpha(); module::beta (); if true { gamma(); } Ok(()) ; }"#,
    )
    .unwrap();
    let Item::Fn(function) = &parsed.items[0] else { panic!("expected function") };
    assert!(has_test(&function.attrs));
    let attributes = attrs(&function.attrs);
    assert!(attributes.iter().any(|attribute| attribute.contains("critical")));

    let found =
        calls("alpha(); alpha(); module::beta (); if cond { gamma(); } Some(1); Ok(2); Err(3);");
    assert_eq!(found, vec!["alpha", "gamma", "module::beta"]);
}

#[test]
fn module_depth_saturates_for_root_and_counts_nested_paths() {
    let root = Path::new("workspace");
    assert_eq!(depth(root, Path::new("workspace/lib.rs")), 0);
    assert_eq!(depth(root, Path::new("workspace/src/lib.rs")), 0);
    assert_eq!(depth(root, Path::new("workspace/src/deep/file.rs")), 1);
}

#[test]
fn cfg_test_modules_and_test_support_files_propagate_test_context_to_helpers() {
    let root = temp_dir("test-context");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("src/lib.rs"),
        r#"
fn production() {}
#[cfg(test)]
mod tests {
    fn helper() { super::production(); }
    #[test]
    fn case() { helper(); assert_eq!(2 + 2, 4); }
}
"#,
    )
    .unwrap();
    fs::write(root.join("src/test_support.rs"), "pub fn fixture_helper() {}\n").unwrap();

    let source = discover(&root);
    let production =
        source.functions.iter().find(|function| function.name == "production").unwrap();
    let case = source.functions.iter().find(|function| function.name == "case").unwrap();
    assert!(!production.is_test);
    assert!(case.is_test);
    assert!(!source.functions.iter().any(|function| function.name == "helper"));
    assert!(!source.functions.iter().any(|function| function.name == "fixture_helper"));
    assert!(!source.files.iter().any(|file| file.path.ends_with("test_support.rs")));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn discovery_ignores_non_rust_files_and_tracks_nested_module_depth_exactly() {
    let root = temp_dir("nested-depth");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/not-rust.txt"), "fn should_not_exist() {}\n").unwrap();
    fs::write(
        root.join("src/lib.rs"),
        r#"mod outer {
    mod inner {
        fn nested() {}
    }
}
"#,
    )
    .unwrap();

    let source = discover(&root);
    assert!(!source.functions.iter().any(|function| function.name == "should_not_exist"));
    let outer = source.modules.iter().find(|module| module.name == "outer").unwrap();
    let inner = source.modules.iter().find(|module| module.name == "outer::inner").unwrap();
    assert_eq!(outer.depth, 1);
    assert_eq!(inner.depth, 2);
    assert_eq!(
        source.functions.iter().find(|f| f.name == "nested").unwrap().qualified_name,
        "outer::inner::nested"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn nested_test_modules_preserve_qualified_test_identity() {
    let root = temp_dir("nested-tests");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("src/lib.rs"),
        r#"#[cfg(test)]
mod tests {
    mod nested {
        #[test]
        fn case() { helper(); }
        fn helper() {}
    }
}
"#,
    )
    .unwrap();

    let source = discover(&root);
    let case = source.functions.iter().find(|function| function.name == "case").unwrap();
    assert!(case.is_test);
    assert_eq!(case.qualified_name, "tests::nested::case");
    assert!(!source.functions.iter().any(|function| function.name == "helper"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cfg_and_test_attribute_helpers_distinguish_similar_attributes() {
    let cfg_test: syn::File = syn::parse_str("#[cfg(test)] mod x {}").unwrap();
    assert!(has_cfg_test(match &cfg_test.items[0] {
        Item::Mod(item) => &item.attrs,
        _ => unreachable!(),
    }));

    let cfg_unix: syn::File = syn::parse_str("#[cfg(unix)] mod x {}").unwrap();
    assert!(!has_cfg_test(match &cfg_unix.items[0] {
        Item::Mod(item) => &item.attrs,
        _ => unreachable!(),
    }));

    let allow_test: syn::File = syn::parse_str("#[allow(test)] fn x() {}").unwrap();
    let Item::Fn(allow_test) = &allow_test.items[0] else { unreachable!() };
    assert!(!has_test(&allow_test.attrs));

    let qualified_test: syn::File = syn::parse_str("#[tokio::test] async fn x() {}").unwrap();
    let Item::Fn(qualified_test) = &qualified_test.items[0] else { unreachable!() };
    assert!(has_test(&qualified_test.attrs));
}

#[test]
fn discovered_type_lines_and_sources_are_exact_for_multiline_items() {
    let root = temp_dir("type-lines");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("src/lib.rs"),
        r#"pub struct Packet {
    first: u32,
    second: Vec<u8>,
}

pub enum State {
    Start,
    Stop,
}

pub struct Pair(
    u8,
    u16,
);

pub struct Marker;
"#,
    )
    .unwrap();

    let source = discover(&root);
    let packet = source.types.iter().find(|ty| ty.name == "Packet").unwrap();
    assert_eq!(packet.line, 1);
    assert_eq!(packet.field_count, 2);
    assert_eq!(packet.source, "pub struct Packet {\n    first: u32,\n    second: Vec<u8>,\n}");

    let state = source.types.iter().find(|ty| ty.name == "State").unwrap();
    assert_eq!(state.line, 6);
    assert_eq!(state.variant_count, 2);
    assert_eq!(state.variant_names, ["Start", "Stop"]);
    assert_eq!(state.source, "pub enum State {\n    Start,\n    Stop,\n}");

    let pair = source.types.iter().find(|ty| ty.name == "Pair").unwrap();
    assert_eq!(pair.line, 11);
    assert_eq!(pair.field_count, 2);
    assert_eq!(pair.source, "pub struct Pair(\n    u8,\n    u16,\n);");

    let marker = source.types.iter().find(|ty| ty.name == "Marker").unwrap();
    assert_eq!(marker.line, 16);
    assert_eq!(marker.field_count, 0);
    assert_eq!(marker.source, "pub struct Marker;");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn call_scanner_handles_adjacent_tokens_whitespace_and_qualified_names_exactly() {
    assert_eq!(calls("a();b(); c (); module::d();"), vec!["a", "b", "c", "module::d"]);
    assert_eq!(calls("if(x){} for(x){} while(x){} Some(1); Ok(2); Err(3);"), Vec::<String>::new());
    assert_eq!(calls("_first(); z9(); ::root::call();"), vec!["_first", "root::call", "z9"]);
}

#[test]
fn call_scanner_helpers_report_progress_for_identifiers_and_calls() {
    assert_eq!(next_identifier("  alpha()", 0), Some(("alpha", 7)));
    assert_eq!(next_identifier("alpha beta", 5), Some(("beta", 10)));
    assert_eq!(next_identifier("123", 0), None);

    assert_eq!(next_call_token("if(x){} alpha();", 0), Some(("alpha".into(), 13)));
    assert_eq!(next_call_token("module::beta ();", 0), Some(("module::beta".into(), 12)));
    assert_eq!(next_call_token("if(x){}", 0), None);
}
