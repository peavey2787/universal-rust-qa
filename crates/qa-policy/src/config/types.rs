use serde::{Deserialize, Serialize};

macro_rules! cfg {($n:ident{$($f:ident:$t:ty=$v:expr),*$(,)?})=>{#[derive(Debug,Clone,Serialize,Deserialize)]#[serde(default)]pub struct $n{$(pub $f:$t),*} impl Default for $n{fn default()->Self{Self{$($f:$v),*}}}}}
cfg!(HealthWeights {
    structure: u32 = 35,
    tests: u32 = 25,
    duplication: u32 = 15,
    dead_code: u32 = 15,
    findings: u32 = 10
});
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SummaryConfig {
    pub health_weights: HealthWeights,
}
cfg!(MetricsConfig {
    file_loc: usize = 400,
    function_loc: usize = 50,
    cyclomatic: usize = 12,
    cognitive: usize = 15,
    crap: f64 = 15.0,
    coverage_percent: f64 = 90.0,
    duplicate_percent: f64 = 5.0,
    dead_code_percent: f64 = 2.0
});
cfg!(SprawlConfig {
    function_statements: usize = 25,
    parameters: usize = 5,
    generic_parameters: usize = 4,
    struct_fields_warn: usize = 12,
    struct_fields_deny: usize = 24,
    enum_variants_warn: usize = 16,
    module_depth_warn: usize = 4,
    module_depth_deny: usize = 6,
    impl_methods_warn: usize = 25,
    trait_methods_warn: usize = 15
});
cfg!(DuplicateConfig {
    minimum_nodes: usize = 15,
    minimum_loc: usize = 8,
    near_clone_similarity: f64 = 0.90
});
cfg!(DeadCodeConfig { closed_world: bool = false, exported_unreferenced: String = "warn".into() });
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ArchitectureConfig {
    #[serde(default)]
    pub layer: Vec<ArchitectureLayer>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchitectureLayer {
    pub name: String,
    pub paths: Vec<String>,
    #[serde(default)]
    pub may_depend_on: Vec<String>,
}
cfg!(TestConfig {
    require_production_reachability: bool = true,
    reject_tautological_assertions: bool = true,
    reject_unseeded_randomness: bool = true,
    reject_anonymous_ignore: bool = true
});
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CoverageConfig {
    pub mode: String,
    pub all_features: bool,
    pub include_packages: Vec<String>,
    pub exclude_packages: Vec<String>,
    pub features: Vec<String>,
    pub no_default_features: bool,
    pub targets: Vec<String>,
    pub adaptive: bool,
}

impl Default for CoverageConfig {
    fn default() -> Self {
        Self {
            mode: "auto".into(),
            all_features: false,
            include_packages: vec![],
            exclude_packages: vec![],
            features: vec![],
            no_default_features: false,
            targets: vec![],
            adaptive: true,
        }
    }
}
cfg!(MutationConfig {
    mode: String = "existing".into(),
    minimum_kill_percent: f64 = 90.0,
    timeout_seconds: u64 = 120
});
cfg!(FuzzConfig {
    build_targets: bool = false,
    require_critical_parser_target: bool = true,
    reject_vacuous_targets: bool = true
});
cfg!(SafetyConfig {
    unwrap: String = "deny".into(),
    expect: String = "deny".into(),
    panic: String = "deny".into(),
    indexing: String = "deny".into(),
    require_safety_comment: bool = true,
    critical_checked_arithmetic: bool = true
});
cfg!(ResourceConfig {
    unbounded_channels: String = "deny".into(),
    untrusted_allocation: String = "deny".into(),
    unbounded_accumulation: String = "deny".into()
});
cfg!(AllocationConfig {
    explicit_leaks: String = "deny".into(),
    hot_path_allocation: String = "warn".into()
});
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentConfig {
    pub detect_absolute_host_paths: bool,
    pub detect_undeclared_env: bool,
    #[serde(default = "env_allow")]
    pub allow_vars: Vec<String>,
}
fn env_allow() -> Vec<String> {
    ["PATH", "RUST_BACKTRACE", "CARGO_MANIFEST_DIR", "OUT_DIR"]
        .into_iter()
        .map(str::to_string)
        .collect()
}
impl Default for EnvironmentConfig {
    fn default() -> Self {
        Self {
            detect_absolute_host_paths: true,
            detect_undeclared_env: true,
            allow_vars: env_allow(),
        }
    }
}
// phases 8-15
cfg!(StateConfig {
    enabled: bool = true,
    require_explicit_invalid_transition: bool = true,
    require_roundtrip_contract: bool = true,
    reject_terminal_exit: bool = true,
    require_restart_contract: bool = true
});
cfg!(AsyncConfig {
    enabled: bool = true,
    blocking_calls: String = "deny".into(),
    detached_tasks: String = "deny".into(),
    await_holding_lock: String = "deny".into(),
    critical_requires_cancellation_contract: bool = true,
    relaxed_atomics: String = "warn".into(),
    static_mut: String = "deny".into()
});
cfg!(ConcurrencyConfig { loom_enabled: bool = false, loom_feature: String = "loom".into() });
cfg!(ErrorConfig {
    discarded_results: String = "deny".into(),
    secret_logging: String = "deny".into(),
    broken_sources: String = "deny".into(),
    lost_context: String = "warn".into()
});
cfg!(SecretConfig { deny_debug_display: bool = true, require_zeroize: bool = true });
cfg!(ConstantTimeConfig {
    enabled: bool = true,
    secret_branch: String = "deny".into(),
    secret_index: String = "deny".into(),
    mode: String = "explicit".into(),
    command: Option<String> = None
});
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SanitizerConfig {
    pub mode: String,
    #[serde(default = "san_list")]
    pub kinds: Vec<String>,
    pub toolchain: String,
    pub target: Option<String>,
    pub msan_complete_instrumentation: bool,
}
fn san_list() -> Vec<String> {
    ["address", "leak", "thread", "memory"].into_iter().map(str::to_string).collect()
}
impl Default for SanitizerConfig {
    fn default() -> Self {
        Self {
            mode: "explicit".into(),
            kinds: san_list(),
            toolchain: "nightly".into(),
            target: None,
            msan_complete_instrumentation: false,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DifferentialConfig {
    pub enabled: bool,
    pub seed: u64,
    #[serde(default)]
    pub target: Vec<DifferentialTarget>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DifferentialTarget {
    pub name: String,
    pub reference_command: String,
    pub candidate_command: String,
    pub corpus: String,
    pub equivalence: String,
}
impl Default for DifferentialConfig {
    fn default() -> Self {
        Self { enabled: false, seed: 1, target: vec![] }
    }
}
cfg!(FaultConfig {
    enabled: bool = false,
    seed: u64 = 1,
    run_tests: bool = false,
    max_fail_points: usize = 16,
    kinds: Vec<String> = vec![
        "io".into(),
        "allocation".into(),
        "partial_io".into(),
        "latency".into(),
        "clock".into(),
    ],
    feature: String = "qa-fault-injection".into()
});
cfg!(MirConfig {
    mode: String = "explicit".into(),
    toolchain: String = "nightly".into(),
    check_drop_cleanup: bool = true,
    check_panic_edges: bool = true,
    check_no_alloc: bool = true,
    check_zeroization: bool = true,
    check_async_retention: bool = true
});
cfg!(PlatformConfig {
    check_default: bool = true,
    check_no_default: bool = true,
    check_all_features: bool = true,
    check_msrv: bool = true,
    check_each_feature: bool = false,
    targets: Vec<String> = vec![]
});
cfg!(BuildConfig {
    deny_network: bool = true,
    writes_outside_out_dir: String = "deny".into(),
    process_spawn: String = "warn".into(),
    require_rerun_directives: bool = true
});
cfg!(LayoutConfig {
    critical_requires_repr: bool = true,
    deny_raw_padded_byte_casts: bool = true,
    deny_packed_references: bool = true
});
cfg!(FfiConfig { require_safety_docs: bool = true, deny_panic_across_boundary: bool = true });
// phases 16-20
cfg!(HardwareConfig {
    enabled: bool = false,
    target: Option<String> = None,
    interrupt_stack_budget_bytes: usize = 2048,
    deny_heap_in_interrupts: bool = true,
    deny_blocking_in_interrupts: bool = true,
    deny_panic_in_interrupts: bool = true,
    linker_map: Option<String> = None
});
cfg!(PerformanceConfig {
    enabled: bool = false,
    false_sharing: String = "warn".into(),
    vectorization_contract: String = "deny".into(),
    instruction_warn_percent: f64 = 10.0,
    instruction_deny_percent: f64 = 25.0,
    baseline_path: String = "qa/performance-baseline.json".into()
});
cfg!(BloatConfig {
    max_percent_growth: f64 = 5.0,
    max_absolute_growth_bytes: u64 = 262144,
    baseline_path: String = "qa/bloat-baseline.json".into()
});
cfg!(HardeningConfig {
    enabled: bool = true,
    release_overflow_checks: bool = true,
    deny_executable_stack: bool = true,
    deny_rwx_segments: bool = true,
    deny_host_paths: bool = true,
    require_pie: bool = true,
    require_full_relro: bool = true
});
cfg!(SnapshotConfig {
    ci_updates: String = "deny".into(),
    pending: String = "deny".into(),
    unreferenced: String = "deny".into(),
    secret_scan: bool = true,
    unstable_content: String = "warn".into()
});
cfg!(DocumentationConfig {
    critical_missing_docs: String = "deny".into(),
    critical_requires_example: bool = true,
    run_doctests: bool = true,
    check_examples: bool = true
});
cfg!(DependencyConfig {
    run_cargo_deny: bool = true,
    run_unused: bool = true,
    deny_wildcards: bool = true,
    deny_git_dependencies: bool = false
});
cfg!(ApiConfig {
    run_semver_checks: bool = false,
    baseline: Option<String> = None,
    unsafe_requires_safety_docs: bool = true,
    public_missing_docs: String = "warn".into(),
    must_use_results: String = "warn".into()
});
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratorTarget {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub outputs: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneratedConfig {
    pub verify: bool,
    #[serde(default)]
    pub target: Vec<GeneratorTarget>,
}
impl Default for GeneratedConfig {
    fn default() -> Self {
        Self { verify: true, target: vec![] }
    }
}
cfg!(ReproConfig {
    enabled: bool = true,
    runs: usize = 2,
    release: bool = true,
    locked: bool = true,
    artifacts: Vec<String> = vec![]
});
cfg!(SelfHardeningConfig {
    enabled: bool = true,
    require_clean_tree: bool = true,
    require_rule_registry_integrity: bool = true,
    require_report_schema: bool = true,
    max_source_file_loc: usize = 600
});
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewerConfig {
    pub command: String,
    pub args: Vec<String>,
}
impl Default for ViewerConfig {
    fn default() -> Self {
        Self { command: "code".into(), args: vec!["--goto".into(), "{path}:{line}".into()] }
    }
}
cfg!(ExceptionPolicy {
    require_reason: bool = true,
    require_expiry: bool = true,
    max_days: u32 = 365
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_list_helpers_preserve_strict_values() {
        assert_eq!(env_allow(), vec!["PATH", "RUST_BACKTRACE", "CARGO_MANIFEST_DIR", "OUT_DIR"]);
        assert_eq!(san_list(), vec!["address", "leak", "thread", "memory"]);
    }
}
