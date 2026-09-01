use super::*;
use crate::test_support::{cleanup, discover, ids};

#[test]
fn sprawl_emits_file_function_and_type_findings_and_metrics() {
    let (root, source) = discover(&[(
        "src/lib.rs",
        "pub struct Big { a:u8,b:u8,c:u8 }\npub enum Many { A,B,C }\nfn wide(a:u8,b:u8,c:u8){ let _=a; let _=b; let _=c; }\n",
    )]);
    let mut config = QaConfig::default();
    config.metrics.file_loc = 1;
    config.sprawl.parameters = 2;
    config.sprawl.struct_fields_warn = 2;
    config.sprawl.enum_variants_warn = 2;
    let mut findings = Vec::new();
    let (types, interfaces) = analyze(&source, &config, &mut findings);
    let found = ids(&findings);
    assert!(found.contains(&"QA-SPRAWL-001"));
    assert!(found.contains(&"QA-SPRAWL-003"));
    assert!(found.contains(&"QA-SPRAWL-004"));
    assert_eq!(types.len(), 2);
    assert!(interfaces.is_empty());
    cleanup(&root);
}

#[test]
fn sprawl_thresholds_are_strict_and_each_dimension_is_independently_observable() {
    let (root, source) = discover(&[(
        "src/lib.rs",
        "pub struct Big { a:u8,b:u8,c:u8 }\npub enum Many { A,B,C }\nfn wide<T,U>(a:u8,b:u8,c:u8){ let _=a; let _=b; let _=c; }\n",
    )]);
    let file_loc = super::super::metrics::logical_loc(&source.files[0].text);
    let function = source.functions.iter().find(|function| function.name == "wide").unwrap();
    let big = source.types.iter().find(|ty| ty.name == "Big").unwrap();
    let many = source.types.iter().find(|ty| ty.name == "Many").unwrap();

    let mut config = QaConfig::default();
    config.metrics.file_loc = file_loc;
    config.sprawl.function_statements = function.statements;
    config.sprawl.parameters = function.parameters;
    config.sprawl.generic_parameters = function.generic_parameters;
    config.sprawl.struct_fields_warn = big.field_count;
    config.sprawl.enum_variants_warn = many.variant_count;
    let mut findings = Vec::new();
    analyze(&source, &config, &mut findings);
    assert!(!ids(&findings).iter().any(|id| id.starts_with("QA-SPRAWL-")));

    config.metrics.file_loc = file_loc - 1;
    let mut findings = Vec::new();
    analyze(&source, &config, &mut findings);
    assert_eq!(ids(&findings).iter().filter(|id| **id == "QA-SPRAWL-001").count(), 1);
    config.metrics.file_loc = file_loc;

    config.sprawl.function_statements = function.statements - 1;
    config.sprawl.parameters = usize::MAX;
    config.sprawl.generic_parameters = usize::MAX;
    let mut findings = Vec::new();
    analyze(&source, &config, &mut findings);
    assert_eq!(ids(&findings).iter().filter(|id| **id == "QA-SPRAWL-003").count(), 1);

    config.sprawl.function_statements = usize::MAX;
    config.sprawl.parameters = function.parameters - 1;
    let mut findings = Vec::new();
    analyze(&source, &config, &mut findings);
    assert_eq!(ids(&findings).iter().filter(|id| **id == "QA-SPRAWL-003").count(), 1);

    config.sprawl.parameters = usize::MAX;
    config.sprawl.generic_parameters = function.generic_parameters - 1;
    let mut findings = Vec::new();
    analyze(&source, &config, &mut findings);
    assert_eq!(ids(&findings).iter().filter(|id| **id == "QA-SPRAWL-003").count(), 1);

    config.sprawl.generic_parameters = usize::MAX;
    config.sprawl.struct_fields_warn = big.field_count - 1;
    config.sprawl.enum_variants_warn = usize::MAX;
    let mut findings = Vec::new();
    analyze(&source, &config, &mut findings);
    assert_eq!(ids(&findings).iter().filter(|id| **id == "QA-SPRAWL-004").count(), 1);

    config.sprawl.struct_fields_warn = usize::MAX;
    config.sprawl.enum_variants_warn = many.variant_count - 1;
    let mut findings = Vec::new();
    analyze(&source, &config, &mut findings);
    assert_eq!(ids(&findings).iter().filter(|id| **id == "QA-SPRAWL-004").count(), 1);
    cleanup(&root);
}
