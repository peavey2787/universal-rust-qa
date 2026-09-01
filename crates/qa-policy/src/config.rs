use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};
use thiserror::Error;
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("could not read configuration {0}: {1}")]
    Read(PathBuf, std::io::Error),
    #[error("could not parse configuration {0}: {1}")]
    Parse(PathBuf, Box<toml::de::Error>),
    #[error("could not serialize configuration: {0}")]
    Serialize(toml::ser::Error),
    #[error("could not write configuration {0}: {1}")]
    Write(PathBuf, std::io::Error),
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QaConfig {
    pub schema: u32,
    pub profile: String,
    pub output_dir: String,
    #[serde(default)]
    pub summary: SummaryConfig,
    #[serde(default)]
    pub metrics: MetricsConfig,
    #[serde(default)]
    pub sprawl: SprawlConfig,
    #[serde(default)]
    pub duplicates: DuplicateConfig,
    #[serde(default)]
    pub dead_code: DeadCodeConfig,
    #[serde(default)]
    pub architecture: ArchitectureConfig,
    #[serde(default)]
    pub tests: TestConfig,
    #[serde(default)]
    pub coverage: CoverageConfig,
    #[serde(default)]
    pub mutation: MutationConfig,
    #[serde(default)]
    pub fuzz: FuzzConfig,
    #[serde(default)]
    pub safety: SafetyConfig,
    #[serde(default)]
    pub resources: ResourceConfig,
    #[serde(default)]
    pub alloc: AllocationConfig,
    #[serde(default)]
    pub environment: EnvironmentConfig,
    #[serde(default)]
    pub state: StateConfig,
    #[serde(default)]
    pub async_rules: AsyncConfig,
    #[serde(default)]
    pub concurrency: ConcurrencyConfig,
    #[serde(default)]
    pub errors: ErrorConfig,
    #[serde(default)]
    pub secrets: SecretConfig,
    #[serde(default)]
    pub constant_time: ConstantTimeConfig,
    #[serde(default)]
    pub sanitizers: SanitizerConfig,
    #[serde(default)]
    pub differential: DifferentialConfig,
    #[serde(default)]
    pub fault: FaultConfig,
    #[serde(default)]
    pub mir: MirConfig,
    #[serde(default)]
    pub platform: PlatformConfig,
    #[serde(default)]
    pub build: BuildConfig,
    #[serde(default)]
    pub layout: LayoutConfig,
    #[serde(default)]
    pub ffi: FfiConfig,
    #[serde(default)]
    pub hardware: HardwareConfig,
    #[serde(default)]
    pub performance: PerformanceConfig,
    #[serde(default)]
    pub bloat: BloatConfig,
    #[serde(default)]
    pub hardening: HardeningConfig,
    #[serde(default)]
    pub snapshots: SnapshotConfig,
    #[serde(default)]
    pub documentation: DocumentationConfig,
    #[serde(default)]
    pub dependencies: DependencyConfig,
    #[serde(default)]
    pub api: ApiConfig,
    #[serde(default)]
    pub generated: GeneratedConfig,
    #[serde(default)]
    pub reproducibility: ReproConfig,
    #[serde(default)]
    pub self_hardening: SelfHardeningConfig,
    #[serde(default)]
    pub viewer: ViewerConfig,
    #[serde(default)]
    pub exceptions: ExceptionPolicy,
    #[serde(default)]
    pub exception: Vec<crate::QaException>,
}
mod types;
pub use types::*;

impl Default for QaConfig {
    fn default() -> Self {
        Self {
            schema: 1,
            profile: "strict".into(),
            output_dir: "qa-out".into(),
            summary: SummaryConfig::default(),
            metrics: MetricsConfig::default(),
            sprawl: SprawlConfig::default(),
            duplicates: DuplicateConfig::default(),
            dead_code: DeadCodeConfig::default(),
            architecture: ArchitectureConfig::default(),
            tests: TestConfig::default(),
            coverage: CoverageConfig::default(),
            mutation: MutationConfig::default(),
            fuzz: FuzzConfig::default(),
            safety: SafetyConfig::default(),
            resources: ResourceConfig::default(),
            alloc: AllocationConfig::default(),
            environment: EnvironmentConfig::default(),
            state: StateConfig::default(),
            async_rules: AsyncConfig::default(),
            concurrency: ConcurrencyConfig::default(),
            errors: ErrorConfig::default(),
            secrets: SecretConfig::default(),
            constant_time: ConstantTimeConfig::default(),
            sanitizers: SanitizerConfig::default(),
            differential: DifferentialConfig::default(),
            fault: FaultConfig::default(),
            mir: MirConfig::default(),
            platform: PlatformConfig::default(),
            build: BuildConfig::default(),
            layout: LayoutConfig::default(),
            ffi: FfiConfig::default(),
            hardware: HardwareConfig::default(),
            performance: PerformanceConfig::default(),
            bloat: BloatConfig::default(),
            hardening: HardeningConfig::default(),
            snapshots: SnapshotConfig::default(),
            documentation: DocumentationConfig::default(),
            dependencies: DependencyConfig::default(),
            api: ApiConfig::default(),
            generated: GeneratedConfig::default(),
            reproducibility: ReproConfig::default(),
            self_hardening: SelfHardeningConfig::default(),
            viewer: ViewerConfig::default(),
            exceptions: ExceptionPolicy::default(),
            exception: vec![],
        }
    }
}
impl QaConfig {
    pub fn load(workspace: &Path) -> Result<Self, ConfigError> {
        let path = workspace.join("qa.toml");
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = fs::read_to_string(&path).map_err(|e| ConfigError::Read(path.clone(), e))?;
        toml::from_str(&text).map_err(|e| ConfigError::Parse(path, Box::new(e)))
    }
    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        let text = toml::to_string_pretty(self).map_err(ConfigError::Serialize)?;
        fs::write(path, text).map_err(|e| ConfigError::Write(path.to_path_buf(), e))
    }
}

#[cfg(test)]
mod tests;
