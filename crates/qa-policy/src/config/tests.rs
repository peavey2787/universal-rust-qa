use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(1);

fn temp_dir(name: &str) -> PathBuf {
    let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("urqa-config-{name}-{}-{id}", std::process::id()));
    if path.exists() {
        fs::remove_dir_all(&path).unwrap();
    }
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn defaults_cover_every_strict_subsystem_and_environment_allowlist() {
    let config = QaConfig::default();
    assert_eq!(config.schema, 1);
    assert_eq!(config.profile, "strict");
    assert_eq!(config.output_dir, "qa-out");
    assert_eq!(config.summary.health_weights.structure, 35);
    assert_eq!(config.metrics.coverage_percent, 90.0);
    assert_eq!(config.mutation.minimum_kill_percent, 90.0);
    assert!(config.safety.require_safety_comment);
    assert!(config.environment.allow_vars.iter().any(|value| value == "PATH"));
    assert!(config.state.enabled);
    assert!(config.async_rules.enabled);
    assert!(config.constant_time.enabled);
    assert_eq!(config.sanitizers.kinds, vec!["address", "leak", "thread", "memory"]);
    assert!(!config.differential.enabled);
    assert!(!config.fault.enabled);
    assert_eq!(config.mir.toolchain, "nightly");
    assert!(config.platform.check_msrv);
    assert!(config.hardening.enabled);
    assert!(config.reproducibility.enabled);
    assert!(config.self_hardening.enabled);
    assert_eq!(config.viewer.command, "code");
}

#[test]
fn save_load_roundtrip_and_absent_configuration_behave_predictably() {
    let root = temp_dir("roundtrip");
    let absent = QaConfig::load(&root).unwrap();
    assert_eq!(absent.profile, "strict");

    let mut config = QaConfig { profile: "custom".into(), ..QaConfig::default() };
    config.metrics.crap = 9.5;
    config.hardware.enabled = true;
    config.save(&root.join("qa.toml")).unwrap();
    let loaded = QaConfig::load(&root).unwrap();
    assert_eq!(loaded.profile, "custom");
    assert_eq!(loaded.metrics.crap, 9.5);
    assert!(loaded.hardware.enabled);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn load_and_save_errors_preserve_error_kind_and_path() {
    let root = temp_dir("errors");
    fs::write(root.join("qa.toml"), "not valid = [").unwrap();
    let error = QaConfig::load(&root).unwrap_err();
    assert!(matches!(error, ConfigError::Parse(_, _)));
    assert!(error.to_string().contains("could not parse"));

    fs::remove_file(root.join("qa.toml")).unwrap();
    fs::create_dir(root.join("qa.toml")).unwrap();
    let error = QaConfig::load(&root).unwrap_err();
    assert!(matches!(error, ConfigError::Read(_, _)));

    let error = QaConfig::default().save(&root.join("missing-parent/config.toml")).unwrap_err();
    assert!(matches!(error, ConfigError::Write(_, _)));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn serde_defaults_fill_omitted_nested_fields() {
    let text = r#"
schema = 1
profile = "strict"
output_dir = "out"

[metrics]
crap = 12.0
"#;
    let root = temp_dir("serde-defaults");
    fs::write(root.join("qa.toml"), text).unwrap();
    let config = QaConfig::load(&root).unwrap();
    assert_eq!(config.metrics.crap, 12.0);
    assert_eq!(config.metrics.file_loc, 400);
    assert_eq!(config.viewer.command, "code");
    assert_eq!(config.fault.max_fail_points, 16);
    fs::remove_dir_all(root).unwrap();
}
