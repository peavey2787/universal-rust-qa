use crate::Finding;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum EvidenceStatus {
    Available,
    Partial,
    Unavailable,
    Disabled,
    Failed,
    #[default]
    Unknown,
    NotApplicable,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceRecord {
    pub family: String,
    pub check: String,
    pub status: EvidenceStatus,
    pub source: Option<String>,
    pub detail: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetric {
    pub path: String,
    pub logical_loc: usize,
    pub physical_loc: usize,
    pub function_count: usize,
    pub average_cyclomatic: f64,
    pub max_cyclomatic: usize,
    pub average_cognitive: f64,
    pub max_cognitive: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionMetric {
    pub path: String,
    pub name: String,
    pub qualified_name: String,
    pub line: usize,
    pub end_line: usize,
    pub logical_loc: usize,
    pub statements: usize,
    pub parameters: usize,
    pub generic_parameters: usize,
    pub cyclomatic: usize,
    pub cognitive: usize,
    pub coverage_percent: Option<f64>,
    pub crap: Option<f64>,
    pub is_test: bool,
    pub is_public: bool,
    pub is_async: bool,
    pub attributes: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeMetric {
    pub path: String,
    pub name: String,
    pub line: usize,
    pub kind: String,
    pub field_count: usize,
    pub variant_count: usize,
    pub is_public: bool,
    pub attributes: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceMetric {
    pub path: String,
    pub name: String,
    pub line: usize,
    pub kind: String,
    pub item_count: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceSpan {
    pub path: String,
    pub line: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateGroup {
    pub fingerprint: String,
    pub kind: String,
    pub similarity: f64,
    pub occurrences: Vec<SourceSpan>,
    pub logical_lines: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeadItem {
    pub path: String,
    pub line: usize,
    pub name: String,
    pub kind: String,
    pub confidence: String,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CoverageSummary {
    pub percent: Option<f64>,
    pub functions_below_threshold: Option<usize>,
    pub source: Option<String>,
    pub status: EvidenceStatus,
    #[serde(default)]
    pub scope_percent: Option<f64>,
    #[serde(default)]
    pub eligible_packages: usize,
    #[serde(default)]
    pub covered_packages: usize,
    #[serde(default)]
    pub failed_packages: usize,
    #[serde(default)]
    pub not_applicable_packages: usize,
    #[serde(default)]
    pub eligible_source_loc: usize,
    #[serde(default)]
    pub covered_source_loc: usize,
    #[serde(default)]
    pub profile_count: usize,
    #[serde(default)]
    pub failure_manifest: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutationSummary {
    pub status: EvidenceStatus,
    pub caught: usize,
    pub missed: usize,
    pub timeout: usize,
    pub unviable: usize,
    pub score_percent: Option<f64>,
    pub source: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutationItem {
    pub outcome: String,
    pub path: Option<String>,
    pub line: Option<usize>,
    pub mutation: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuzzTargetEvidence {
    pub name: String,
    pub path: String,
    pub line: usize,
    pub reaches_production: bool,
    pub critical_targets: Vec<String>,
    pub build_status: EvidenceStatus,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuzzSummary {
    pub target_count: usize,
    pub critical_targets_missing: usize,
    pub regression_artifacts: usize,
    pub unpersisted_crashes: usize,
    pub property_test_count: usize,
    pub status: EvidenceStatus,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryMetrics {
    pub health_score: f64,
    pub health_is_provisional: bool,
    pub average_file_loc: f64,
    pub files_over_loc: usize,
    pub average_cc: f64,
    pub functions_over_cc: usize,
    pub average_crap: Option<f64>,
    pub functions_over_crap: Option<usize>,
    pub total_tests: usize,
    pub invalid_tests: usize,
    pub coverage: CoverageSummary,
    pub mutation: MutationSummary,
    pub fuzz: FuzzSummary,
    pub duplicate_percent: f64,
    pub dead_code_percent: f64,
    pub high_findings: usize,
    pub critical_findings: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QaReport {
    pub schema: u32,
    pub generated_unix_seconds: u64,
    pub workspace: String,
    pub profile: String,
    pub summary: SummaryMetrics,
    pub files: Vec<FileMetric>,
    pub functions: Vec<FunctionMetric>,
    pub types: Vec<TypeMetric>,
    pub interfaces: Vec<InterfaceMetric>,
    pub mutations: Vec<MutationItem>,
    pub fuzz_targets: Vec<FuzzTargetEvidence>,
    pub duplicates: Vec<DuplicateGroup>,
    pub dead_items: Vec<DeadItem>,
    pub evidence: Vec<EvidenceRecord>,
    pub findings: Vec<Finding>,
}
