use serde::{Deserialize, Serialize};
#[derive(Debug, Clone)]
pub(super) struct CoveragePackage {
    pub name: String,
    pub root: String,
    pub source_loc: usize,
    pub default_member: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct CoverageAttempt {
    pub package: Option<String>,
    pub target: Option<String>,
    pub configuration: String,
    pub features: Vec<String>,
    pub no_default_features: bool,
    pub all_features: bool,
    pub command: Vec<String>,
    pub exit_code: Option<i32>,
    pub stage: String,
    pub outcome: String,
    pub category: Option<String>,
    pub profiles_before: usize,
    pub profiles_after: usize,
    pub diagnostic: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct CoverageManifest {
    pub schema: u32,
    pub status: String,
    pub workspace_packages: usize,
    pub eligible_packages: usize,
    pub covered_packages: usize,
    pub failed_packages: usize,
    pub not_applicable_packages: usize,
    pub eligible_source_loc: usize,
    pub covered_source_loc: usize,
    pub profile_count: usize,
    pub eligible_package_names: Vec<String>,
    pub covered_package_names: Vec<String>,
    pub failed_package_names: Vec<String>,
    pub not_applicable_package_names: Vec<String>,
    pub covered_package_roots: Vec<String>,
    pub excluded_package_roots: Vec<String>,
    pub attempts: Vec<CoverageAttempt>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AttemptOutcome {
    Success,
    Failed,
    Unavailable,
}

impl AttemptOutcome {
    pub fn label(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failed => "failed",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct AttemptResult {
    pub outcome: AttemptOutcome,
    pub exit_code: Option<i32>,
    pub diagnostic: Option<String>,
}
