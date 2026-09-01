use super::*;
use crate::test_support::{cleanup, discover};

#[test]
fn analyze_populates_file_function_type_interface_and_rule_outputs() {
    let (root, source) = discover(&[(
        "src/lib.rs",
        r#"
pub struct Item { a:u8 }
trait T { fn run(&self); }
impl T for Item { fn run(&self) {} }
fn helper(x:bool){ if x { work(); } }
fn work(){}
#[test] fn helper_test(){ helper(false); assert_eq!(1 + 1, 2); }
"#,
    )]);
    let output = analyze(&source, &QaConfig::default());
    assert_eq!(output.files.len(), 1);
    assert!(output.functions.len() >= 4);
    assert_eq!(output.types.len(), 1);
    assert!(output.interfaces.len() >= 2);
    assert!(output.total_logical_loc > 0);
    let helper = output.functions.iter().find(|f| f.name == "helper").unwrap();
    assert!(helper.cyclomatic > 1);
    assert!(helper.logical_loc > 0);
    cleanup(&root);
}

#[test]
fn average_handles_empty_and_nonempty_vectors() {
    assert_eq!(avg(&[]), 0.0);
    assert_eq!(avg(&[1, 2, 3]), 2.0);
}

#[test]
fn file_function_counts_are_scoped_to_the_matching_source_file() {
    let (root, source) =
        discover(&[("src/a.rs", "fn a() {}\nfn b() {}\n"), ("src/b.rs", "fn c() {}\n")]);
    let output = analyze(&source, &QaConfig::default());
    let a = output.files.iter().find(|file| file.path.ends_with("a.rs")).unwrap();
    let b = output.files.iter().find(|file| file.path.ends_with("b.rs")).unwrap();
    assert_eq!(a.function_count, 2);
    assert_eq!(b.function_count, 1);
    cleanup(&root);
}
