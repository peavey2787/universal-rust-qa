use qa_model::{Finding, InterfaceMetric, Severity, TypeMetric};
use qa_policy::QaConfig;
use qa_syntax::WorkspaceSource;
pub fn analyze(
    s: &WorkspaceSource,
    c: &QaConfig,
    f: &mut Vec<Finding>,
) -> (Vec<TypeMetric>, Vec<InterfaceMetric>) {
    for sf in &s.files {
        let l = super::metrics::logical_loc(&sf.text);
        if l > c.metrics.file_loc {
            f.push(Finding {
                rule_id: "QA-SPRAWL-001".into(),
                severity: Severity::Medium,
                message: format!("File has {l} logical LOC; limit {}", c.metrics.file_loc),
                path: Some(sf.path.display().to_string()),
                line: Some(1),
                detail: None,
            })
        }
    }
    for x in &s.functions {
        if x.statements > c.sprawl.function_statements
            || x.parameters > c.sprawl.parameters
            || x.generic_parameters > c.sprawl.generic_parameters
        {
            f.push(Finding {
                rule_id: "QA-SPRAWL-003".into(),
                severity: Severity::Medium,
                message: format!(
                    "Function `{}` interface/body exceeds sprawl policy",
                    x.qualified_name
                ),
                path: Some(x.path.display().to_string()),
                line: Some(x.line),
                detail: None,
            })
        }
    }
    let ty = s
        .types
        .iter()
        .map(|t| {
            if t.field_count > c.sprawl.struct_fields_warn
                || t.variant_count > c.sprawl.enum_variants_warn
            {
                f.push(Finding {
                    rule_id: "QA-SPRAWL-004".into(),
                    severity: Severity::Medium,
                    message: format!("Type `{}` exceeds shape threshold", t.name),
                    path: Some(t.path.display().to_string()),
                    line: Some(t.line),
                    detail: None,
                })
            }
            TypeMetric {
                path: t.path.display().to_string(),
                name: t.name.clone(),
                line: t.line,
                kind: t.kind.clone(),
                field_count: t.field_count,
                variant_count: t.variant_count,
                is_public: t.is_public,
                attributes: t.attributes.clone(),
            }
        })
        .collect();
    let it = s
        .interfaces
        .iter()
        .map(|i| InterfaceMetric {
            path: i.path.display().to_string(),
            name: i.name.clone(),
            line: i.line,
            kind: i.kind.clone(),
            item_count: i.item_count,
        })
        .collect();
    (ty, it)
}

#[cfg(test)]
mod tests;
