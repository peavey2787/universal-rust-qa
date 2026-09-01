use super::*;
use crate::test_support::{cleanup, discover, ids};

#[test]
fn build_layout_proc_macro_and_ffi_rules_cover_strict_failures() {
    let (root, source) = discover(&[
        (
            "src/lib.rs",
            r#"
#[qa_attr::critical_layout]
struct Wire { a: u8, b: u32 }
fn bytes(w: Wire) { let _ = unsafe { std::slice::from_raw_parts((&w as *const Wire) as *const u8, 8) }; }
pub unsafe extern "C" fn bad(v: Vec<u8>) { panic!("boom"); }
pub extern "C" fn pointer(p: *const u8) { let _=p; }
"#,
        ),
        (
            "build.rs",
            r#"
fn main(){ let _=std::net::TcpStream::connect("x"); let _=std::process::Command::new("tool"); let _=std::fs::read("input"); std::fs::write("output","x").unwrap(); }
"#,
        ),
        (
            "src/macros.rs",
            "fn expand(){ let _ = proc_macro2::TokenStream::new(); let _=std::net::TcpStream::connect(\"x\"); let _=std::process::Command::new(\"x\"); let _=std::env::var(\"X\"); }\n",
        ),
    ]);
    let mut findings = Vec::new();
    analyze(&source, &QaConfig::default(), &mut findings);
    let found = ids(&findings);
    for expected in [
        "QA-LAYOUT-001",
        "QA-LAYOUT-006",
        "QA-BUILD-003",
        "QA-BUILD-004",
        "QA-BUILD-002",
        "QA-BUILD-006",
        "QA-BUILD-007",
        "QA-BUILD-008",
        "QA-BUILD-009",
        "QA-FFI-003",
        "QA-FFI-002",
        "QA-FFI-001",
        "QA-FFI-004",
    ] {
        assert!(found.contains(&expected), "missing {expected}: {found:?}");
    }
    cleanup(&root);
}

#[test]
fn stable_and_packed_layouts_and_build_allowances_take_expected_paths() {
    let (root, source) = discover(&[
        (
            "src/lib.rs",
            "#[repr(C, packed)] #[qa_attr::critical_layout] struct Wire { a:u8 }\nextern \"C\" fn ok(v:u32){let _=v;}\n",
        ),
        (
            "build.rs",
            "fn main(){ println!(\"cargo::rerun-if-changed=input\"); let out=std::env::var(\"OUT_DIR\").unwrap(); std::fs::write(out,\"x\").unwrap(); }\n",
        ),
    ]);
    let mut findings = Vec::new();
    analyze(&source, &QaConfig::default(), &mut findings);
    let found = ids(&findings);
    assert!(!found.contains(&"QA-LAYOUT-001"));
    assert!(found.contains(&"QA-LAYOUT-008"));
    assert!(!found.contains(&"QA-BUILD-002"));
    assert!(!found.contains(&"QA-BUILD-006"));
    assert!(!found.contains(&"QA-FFI-001"));
    cleanup(&root);
}

#[test]
fn raw_byte_cast_detector_distinguishes_cast_forms() {
    for code in ["transmute(x)", "slice::from_raw_parts(p,n)", "p as *const u8", "p as *mut u8"] {
        assert!(raw_byte_cast(code));
    }
    assert!(!raw_byte_cast("safe_copy(value)"));
}

#[test]
fn repr_and_out_dir_detection_survive_token_spacing_and_string_literals() {
    let (root, source) = discover(&[(
        "src/lib.rs",
        "#[repr(C)] #[qa_attr::critical_layout] struct Wire { value: u32 }\n",
    )]);
    let mut findings = Vec::new();
    analyze(&source, &QaConfig::default(), &mut findings);
    assert!(!ids(&findings).contains(&"QA-LAYOUT-001"));
    cleanup(&root);

    let (root, source) = discover(&[(
        "build.rs",
        "fn main(){ println!(\"cargo::rerun-if-env-changed=OUT_DIR\"); let out=std::env::var(\"OUT_DIR\").unwrap(); std::fs::write(out,\"x\").unwrap(); }\n",
    )]);
    findings.clear();
    analyze(&source, &QaConfig::default(), &mut findings);
    assert!(!ids(&findings).contains(&"QA-BUILD-002"));
    cleanup(&root);
}

#[test]
fn out_dir_mentions_in_comments_do_not_suppress_write_findings() {
    let (root, source) = discover(&[(
        "build.rs",
        "fn main(){ /* OUT_DIR */ std::fs::write(\"output\",\"x\").unwrap(); }\n",
    )]);
    let mut findings = Vec::new();
    analyze(&source, &QaConfig::default(), &mut findings);
    assert!(ids(&findings).contains(&"QA-BUILD-002"));
    cleanup(&root);
}

#[test]
fn conjunction_guards_require_both_policy_and_signal() {
    let (root, source) = discover(&[
        (
            "src/lib.rs",
            r#"
#[repr(C)] #[qa_attr::critical_layout]
struct Plain { a:u32 }
#[repr(C, packed)] #[qa_attr::critical_layout]
struct Packed { a:u32 }
fn mentions_plain(_: Plain) {}
fn casts_other(p:*const u32) { let _ = p as *const u8; }
"#,
        ),
        (
            "build.rs",
            "fn main(){ let _=std::net::TcpStream::connect(\"x\"); let _=std::process::Command::new(\"x\"); }\n",
        ),
    ]);
    let mut config = QaConfig::default();
    config.layout.deny_packed_references = false;
    config.build.deny_network = false;
    config.build.process_spawn = "allow".into();
    let mut findings = Vec::new();
    analyze(&source, &config, &mut findings);
    let found = ids(&findings);
    assert!(!found.contains(&"QA-LAYOUT-008"));
    assert!(!found.contains(&"QA-LAYOUT-006"));
    assert!(!found.contains(&"QA-BUILD-003"));
    assert!(!found.contains(&"QA-BUILD-004"));
    cleanup(&root);
}

#[test]
fn proc_macro_checks_distinguish_each_independent_signal() {
    let file = qa_syntax::SourceFile {
        path: "src/macros.rs".into(),
        text: String::new(),
        module_depth: 0,
    };
    let mut config = QaConfig::default();
    let mut findings = Vec::new();

    config.build.deny_network = false;
    analyze_proc_macro(&file, &config, "TcpStream", &mut findings);
    assert!(!ids(&findings).contains(&"QA-BUILD-007"));

    findings.clear();
    config.build.process_spawn = "allow".into();
    analyze_proc_macro(&file, &config, "Command::new", &mut findings);
    assert!(!ids(&findings).contains(&"QA-BUILD-008"));

    findings.clear();
    analyze_proc_macro(&file, &config, "std::env::var(\"X\")", &mut findings);
    assert!(ids(&findings).contains(&"QA-BUILD-009"));
    findings.clear();
    analyze_proc_macro(&file, &config, "env::var(\"X\")", &mut findings);
    assert!(ids(&findings).contains(&"QA-BUILD-009"));
}

#[test]
fn ffi_safety_and_raw_pointer_guards_have_independent_negative_cases() {
    let (root, source) = discover(&[(
        "src/lib.rs",
        r#"
/// # Safety
/// caller validates the pointer
pub unsafe extern "C" fn documented(p:*const u8) { let _=p; }
pub unsafe extern "C" fn source_contract(p:*const u8) { /* SAFETY: fixture contract */ let _=p; }
pub unsafe extern "C" fn unsafe_pointer(p:*const u8) { let _=p; }
pub extern "C" fn safe_scalar(v:u32) { let _=v; }
"#,
    )]);
    let mut config = QaConfig::default();
    let mut findings = Vec::new();
    let documented =
        source.functions.iter().find(|function| function.name == "documented").unwrap();
    check_ffi_safety_docs(documented, &config, &mut findings);
    assert!(findings.is_empty());
    let source_contract =
        source.functions.iter().find(|function| function.name == "source_contract").unwrap();
    check_ffi_safety_docs(source_contract, &config, &mut findings);
    assert!(findings.is_empty());

    config.ffi.require_safety_docs = false;
    let unsafe_pointer =
        source.functions.iter().find(|function| function.name == "unsafe_pointer").unwrap();
    check_ffi_safety_docs(unsafe_pointer, &config, &mut findings);
    assert!(findings.is_empty());
    check_ffi_raw_pointer(unsafe_pointer, &mut findings);
    assert!(findings.is_empty());

    let safe_scalar =
        source.functions.iter().find(|function| function.name == "safe_scalar").unwrap();
    check_ffi_raw_pointer(safe_scalar, &mut findings);
    assert!(findings.is_empty());
    cleanup(&root);
}

#[test]
fn ffi_safety_docs_require_both_policy_and_unsafe_function() {
    let (root, source) = discover(&[(
        "src/lib.rs",
        r#"
pub unsafe extern "C" fn unsafe_undocumented(value: u32) { let _ = value; }
pub extern "C" fn safe_undocumented(value: u32) { let _ = value; }
"#,
    )]);
    let unsafe_fn =
        source.functions.iter().find(|function| function.name == "unsafe_undocumented").unwrap();
    let safe_fn =
        source.functions.iter().find(|function| function.name == "safe_undocumented").unwrap();
    let mut config = QaConfig::default();
    let mut findings = Vec::new();
    check_ffi_safety_docs(unsafe_fn, &config, &mut findings);
    assert_eq!(findings.iter().filter(|finding| finding.rule_id == "QA-FFI-003").count(), 1);

    findings.clear();
    check_ffi_safety_docs(safe_fn, &config, &mut findings);
    assert!(findings.is_empty());

    config.ffi.require_safety_docs = false;
    check_ffi_safety_docs(unsafe_fn, &config, &mut findings);
    assert!(findings.is_empty());
    cleanup(&root);
}
