use super::*;
use crate::test_support::{cleanup, discover};

#[test]
fn private_and_closed_world_exported_dead_functions_are_classified() {
    let (root, source) = discover(&[(
        "src/lib.rs",
        "fn live(){ helper(); }\nfn helper(){}\nfn dead(){}\npub fn exported(){}\nfn main(){}\n",
    )]);
    let mut findings = Vec::new();
    let items = analyze(&source, &QaConfig::default(), &mut findings);
    assert!(items.iter().any(|item| item.name.ends_with("dead")));
    assert!(!items.iter().any(|item| item.name.ends_with("helper")));
    assert!(!items.iter().any(|item| item.name.ends_with("exported")));

    let mut config = QaConfig::default();
    config.dead_code.closed_world = true;
    findings.clear();
    let items = analyze(&source, &config, &mut findings);
    assert!(items.iter().any(|item| item.name.ends_with("exported")));
    assert!(findings.iter().any(|f| f.rule_id == "QA-DEAD-002"));
    cleanup(&root);
}

#[test]
fn call_counts_strip_module_qualification_and_root_names_are_excluded() {
    let (root, source) = discover(&[(
        "src/lib.rs",
        "fn caller(){ module::callee(); }\nfn callee(){}\nfn recursive(){ recursive(); }\nfn qualified(){ module::qualified(); }\nfn new(){}\nfn default(){}\nfn drop(){}\n",
    )]);
    let calls = call_counts(&source);
    assert_eq!(calls.get("callee"), Some(&1));
    assert!(!calls.contains_key("recursive"));
    assert_eq!(calls.get("qualified"), Some(&1));
    let mut findings = Vec::new();
    let items = analyze(&source, &QaConfig::default(), &mut findings);
    assert!(!items.iter().any(|item| item.name.ends_with("callee")));
    assert!(items.iter().any(|item| item.name.ends_with("recursive")));
    assert!(!items.iter().any(|item| item.name.ends_with("qualified")));
    assert!(!items.iter().any(|item| item.name.ends_with("new")));
    cleanup(&root);
}

#[test]
fn function_pointer_tables_and_turbofish_calls_count_as_live_references() {
    let (root, source) = discover(&[(
        "src/lib.rs",
        r#"
fn action_loc() {}
fn action_cc() {}
const ACTIONS: [fn(); 2] = [action_loc, crate::action_cc];
struct Ops;
impl Ops { fn action_method() {} }
const METHOD: fn() = Ops::action_method;
fn read_json_map<T>() -> Option<T> { None }
fn caller() { let _ = crate::read_json_map::<usize>(); }
fn truly_dead() {}
"#,
    )]);
    let calls = call_counts(&source);
    assert_eq!(calls.get("action_loc"), Some(&1));
    assert_eq!(calls.get("action_cc"), Some(&1));
    assert_eq!(calls.get("action_method"), Some(&1));
    assert_eq!(calls.get("read_json_map"), Some(&1));

    let mut findings = Vec::new();
    let items = analyze(&source, &QaConfig::default(), &mut findings);
    assert!(!items.iter().any(|item| item.name.ends_with("action_loc")));
    assert!(!items.iter().any(|item| item.name.ends_with("action_cc")));
    assert!(!items.iter().any(|item| item.name.ends_with("action_method")));
    assert!(!items.iter().any(|item| item.name.ends_with("read_json_map")));
    assert!(items.iter().any(|item| item.name.ends_with("truly_dead")));
    cleanup(&root);
}

#[test]
fn impl_trait_default_and_method_calls_are_all_live_reference_sources() {
    let (root, source) = discover(&[(
        "src/lib.rs",
        r#"
fn helper() {}
struct Ops;
impl Ops {
    fn target(&self) {}
    fn caller(&self) { helper(); self.target(); }
}
trait Defaults {
    fn default_caller(&self) { helper(); }
}
fn truly_dead() {}
"#,
    )]);
    let calls = call_counts(&source);
    assert_eq!(calls.get("helper"), Some(&2));
    assert_eq!(calls.get("target"), Some(&1));

    let mut findings = Vec::new();
    let items = analyze(&source, &QaConfig::default(), &mut findings);
    assert!(!items.iter().any(|item| item.name.ends_with("helper")));
    assert!(!items.iter().any(|item| item.name.ends_with("target")));
    assert!(items.iter().any(|item| item.name.ends_with("truly_dead")));
    cleanup(&root);
}

#[test]
fn macro_token_references_and_trait_impl_methods_are_not_reported_dead() {
    let (root, source) = discover(&[(
        "src/lib.rs",
        r#"
fn formatted_helper() -> &'static str { "ok" }
fn vector_helper() -> usize { 1 }
fn truly_dead() {}

trait Render { fn render(&self) -> String; }
struct Item;
impl Render for Item {
    fn render(&self) -> String { format!("{}", formatted_helper()) }
}

fn caller() {
    let _ = format!("{}", formatted_helper());
    let _ = vec![vector_helper()];
}
"#,
    )]);
    let calls = call_counts(&source);
    assert!(calls.get("formatted_helper").is_some_and(|count| *count >= 1));
    assert!(calls.get("vector_helper").is_some_and(|count| *count >= 1));

    let mut findings = Vec::new();
    let items = analyze(&source, &QaConfig::default(), &mut findings);
    assert!(!items.iter().any(|item| item.name.ends_with("formatted_helper")));
    assert!(!items.iter().any(|item| item.name.ends_with("vector_helper")));
    assert!(!items.iter().any(|item| item.name.ends_with("render")));
    assert!(items.iter().any(|item| item.name.ends_with("truly_dead")));
    cleanup(&root);
}

#[test]
fn trait_impl_detection_uses_the_qualified_owner_only() {
    let (root, source) = discover(&[(
        "src/lib.rs",
        r#"
trait Worker { fn work(&self); }
struct Ops;
impl Worker for Ops { fn work(&self) {} }
impl Ops { fn ordinary_dead(&self) {} }
"#,
    )]);
    let trait_method = source.functions.iter().find(|function| function.name == "work").unwrap();
    let inherent =
        source.functions.iter().find(|function| function.name == "ordinary_dead").unwrap();
    assert!(trait_impl_function(trait_method));
    assert!(!trait_impl_function(inherent));

    let mut findings = Vec::new();
    let items = analyze(&source, &QaConfig::default(), &mut findings);
    assert!(!items.iter().any(|item| item.name.ends_with("work")));
    assert!(items.iter().any(|item| item.name.ends_with("ordinary_dead")));
    cleanup(&root);
}

#[test]
fn uninvoked_macro_definition_does_not_make_a_function_live() {
    let (root, source) = discover(&[(
        "src/lib.rs",
        r#"
fn helper_only_in_macro_definition() {}
macro_rules! invoke_helper { () => { helper_only_in_macro_definition(); } }
fn caller() {}
"#,
    )]);
    let mut findings = Vec::new();
    let items = analyze(&source, &QaConfig::default(), &mut findings);
    assert!(items.iter().any(|item| item.name.ends_with("helper_only_in_macro_definition")));
    cleanup(&root);
}
