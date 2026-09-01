use qa_model::{
    DeadItem, DuplicateGroup, FileMetric, Finding, FunctionMetric, FuzzTargetEvidence,
    InterfaceMetric, TypeMetric,
};
use qa_policy::QaConfig;
use qa_syntax::WorkspaceSource;
#[derive(Default)]
pub struct RuleOutput {
    pub files: Vec<FileMetric>,
    pub functions: Vec<FunctionMetric>,
    pub types: Vec<TypeMetric>,
    pub interfaces: Vec<InterfaceMetric>,
    pub duplicates: Vec<DuplicateGroup>,
    pub dead_items: Vec<DeadItem>,
    pub findings: Vec<Finding>,
    pub total_logical_loc: usize,
    pub duplicate_logical_loc: usize,
    pub invalid_tests: usize,
    pub fuzz_targets: Vec<FuzzTargetEvidence>,
    pub critical_fuzz_missing: usize,
    pub fuzz_regression_artifacts: usize,
    pub fuzz_unpersisted_crashes: usize,
    pub property_test_count: usize,
}
pub fn analyze(s: &WorkspaceSource, c: &QaConfig) -> RuleOutput {
    let mut o = RuleOutput::default();
    o.findings.extend(s.parse_findings.clone());
    for sf in &s.files {
        let physical = sf.text.lines().count();
        let logical = crate::structural::metrics::logical_loc(&sf.text);
        o.total_logical_loc += logical;
        let fs: Vec<_> = s.functions.iter().filter(|x| x.path == sf.path).collect();
        let cc: Vec<_> =
            fs.iter().map(|x| crate::structural::metrics::cyclomatic(&x.source)).collect();
        let cg: Vec<_> =
            fs.iter().map(|x| crate::structural::metrics::cognitive(&x.source)).collect();
        o.files.push(FileMetric {
            path: sf.path.display().to_string(),
            logical_loc: logical,
            physical_loc: physical,
            function_count: fs.len(),
            average_cyclomatic: avg(&cc),
            max_cyclomatic: cc.iter().copied().max().unwrap_or(0),
            average_cognitive: avg(&cg),
            max_cognitive: cg.iter().copied().max().unwrap_or(0),
        })
    }
    for x in &s.functions {
        let loc = crate::structural::metrics::logical_loc(&x.source);
        let cc = crate::structural::metrics::cyclomatic(&x.source);
        let cog = crate::structural::metrics::cognitive(&x.source);
        o.findings.extend(crate::structural::metrics::findings(x, c, loc, cc, cog));
        o.functions.push(FunctionMetric {
            path: x.path.display().to_string(),
            name: x.name.clone(),
            qualified_name: x.qualified_name.clone(),
            line: x.line,
            end_line: x.end_line,
            logical_loc: loc,
            statements: x.statements,
            parameters: x.parameters,
            generic_parameters: x.generic_parameters,
            cyclomatic: cc,
            cognitive: cog,
            coverage_percent: None,
            crap: None,
            is_test: x.is_test,
            is_public: x.is_public,
            is_async: x.is_async,
            attributes: x.attributes.clone(),
        })
    }
    let (t, i) = crate::structural::sprawl(s, c, &mut o.findings);
    o.types = t;
    o.interfaces = i;
    let (d, n) = crate::structural::duplicate(s, c, &mut o.findings);
    o.duplicates = d;
    o.duplicate_logical_loc = n;
    o.dead_items = crate::structural::dead(s, c, &mut o.findings);
    crate::structural::architecture(s, c, &mut o.findings);
    o.invalid_tests = crate::test_quality::analyze(s, c, &mut o.findings);
    let z = crate::fuzz::analyze(s, c, &mut o.findings);
    o.fuzz_targets = z.targets;
    o.critical_fuzz_missing = z.critical_missing;
    o.property_test_count = z.property_test_count;
    crate::core_safety::analyze(s, c, &mut o.findings);
    crate::state::analyze(s, c, &mut o.findings);
    crate::async_concurrency::analyze(s, c, &mut o.findings);
    crate::security_error::analyze(s, c, &mut o.findings);
    crate::platform::analyze(s, c, &mut o.findings);
    crate::hardware::analyze(s, c, &mut o.findings);
    crate::performance::analyze(s, c, &mut o.findings);
    crate::hardening::analyze(s, c, &mut o.findings);
    crate::release_engineering::analyze(s, c, &mut o.findings);
    o
}
fn avg(v: &[usize]) -> f64 {
    if v.is_empty() { 0.0 } else { v.iter().sum::<usize>() as f64 / v.len() as f64 }
}

#[cfg(test)]
mod tests;
