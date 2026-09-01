use qa_policy::QaConfig;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(1);

fn workspace(name: &str) -> PathBuf {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("urqa-cli-{name}-{}-{id}", std::process::id()));
    if root.exists() {
        fs::remove_dir_all(&root).unwrap();
    }
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("src/lib.rs"),
        "pub fn value() -> u8 { 1 }\n#[test] fn value_works(){ assert_eq!(value(),1); }\n",
    )
    .unwrap();
    root
}

fn run(root: &Path, args: &[&str], input: &str) -> Output {
    run_from(root, args, input)
}

fn run_from(cwd: &Path, args: &[&str], input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_cargo-qa"))
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.as_mut().unwrap().write_all(input.as_bytes()).unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn help_doctor_export_import_and_default_scan_are_executable() {
    let root = workspace("commands");
    let help = run(&root, &["--help"], "");
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("self-hardening"));

    let doctor = run(&root, &["doctor"], "");
    assert!(doctor.status.success());
    assert!(String::from_utf8_lossy(&doctor.stdout).contains("Universal Rust QA toolchain doctor"));

    let exported = root.join("export.toml");
    let export = run(&root, &["export-config", exported.to_str().unwrap()], "");
    assert!(export.status.success());
    assert!(exported.is_file());
    let mut text = fs::read_to_string(&exported).unwrap();
    text = text.replace("profile = \"strict\"", "profile = \"imported\"");
    fs::write(&exported, text).unwrap();
    let import = run(&root, &["import-config", exported.to_str().unwrap()], "");
    assert!(import.status.success());
    assert_eq!(QaConfig::load(&root).unwrap().profile, "imported");

    let scan = run(&root, &[], "");
    assert!(scan.status.success());
    assert!(String::from_utf8_lossy(&scan.stdout).contains("UNIVERSAL RUST QA"));
    assert!(root.join("qa-out/report.json").is_file());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn settings_menu_exercises_every_configuration_family() {
    let root = workspace("settings");
    let before = QaConfig::default();
    let input = concat!(
        "1\n1\n401\n2\n51\n3\n13\n4\n16\n5\n16.5\n6\n91\n7\n6\n8\n3\nb\n",
        "2\n1\n26\n2\n6\n3\n5\n4\n9\n5\n0.91\n6\n7\nb\n",
        "3\n1\n2\n3\n4\n5\n6\nwarn\n7\nwarn\n8\ndeny\nb\n",
        "4\n1\nwarn\n2\nwarn\n3\nwarn\n4\n5\n6\nb\n",
        "5\n1\noff\n2\nstable\n3\n4\n5\n7\n6\n7\n9\n8\n8\n9\noff\n10\nbeta\nb\n",
        "6\n1\n2\n3\n4\n5\n6\n7\nallow\n8\n9\n10\nb\n",
        "7\n1\n2\n1024\n3\n4\n5\ndeny\n6\n11\n7\n26\n8\n6\n9\n300000\n10\n11\n12\n13\nb\n",
        "8\n1\nallow\n2\nallow\n3\n4\n5\n6\n7\n8\n9\n10\n11\n12\n3\n13\nb\n",
        "9\nrustc\n--version|{path}\n",
        "b\n",
    );
    let output = run(&root, &["settings"], input);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let config = QaConfig::load(&root).unwrap();
    assert_eq!(config.metrics.file_loc, 401);
    assert_eq!(config.metrics.function_loc, 51);
    assert_eq!(config.metrics.cyclomatic, 13);
    assert_eq!(config.metrics.cognitive, 16);
    assert_eq!(config.metrics.crap, 16.5);
    assert_eq!(config.metrics.coverage_percent, 91.0);
    assert_eq!(config.metrics.duplicate_percent, 6.0);
    assert_eq!(config.metrics.dead_code_percent, 3.0);

    assert_eq!(config.sprawl.function_statements, 26);
    assert_eq!(config.sprawl.parameters, 6);
    assert_eq!(config.sprawl.generic_parameters, 5);
    assert_eq!(config.duplicates.minimum_loc, 9);
    assert_eq!(config.duplicates.near_clone_similarity, 0.91);
    assert_ne!(config.dead_code.closed_world, before.dead_code.closed_world);
    assert_ne!(
        config.tests.require_production_reachability,
        before.tests.require_production_reachability
    );

    assert_ne!(config.state.enabled, before.state.enabled);
    assert_ne!(config.state.require_roundtrip_contract, before.state.require_roundtrip_contract);
    assert_ne!(config.state.require_restart_contract, before.state.require_restart_contract);
    assert_ne!(config.async_rules.enabled, before.async_rules.enabled);
    assert_ne!(
        config.async_rules.critical_requires_cancellation_contract,
        before.async_rules.critical_requires_cancellation_contract
    );
    assert_eq!(config.async_rules.blocking_calls, "warn");
    assert_eq!(config.async_rules.detached_tasks, "warn");
    assert_eq!(config.async_rules.relaxed_atomics, "deny");

    assert_eq!(config.errors.discarded_results, "warn");
    assert_eq!(config.errors.secret_logging, "warn");
    assert_eq!(config.errors.broken_sources, "warn");
    assert_ne!(config.secrets.require_zeroize, before.secrets.require_zeroize);
    assert_ne!(config.secrets.deny_debug_display, before.secrets.deny_debug_display);
    assert_ne!(config.constant_time.enabled, before.constant_time.enabled);

    assert_eq!(config.sanitizers.mode, "off");
    assert_eq!(config.sanitizers.toolchain, "stable");
    assert_ne!(
        config.sanitizers.msan_complete_instrumentation,
        before.sanitizers.msan_complete_instrumentation
    );
    assert_ne!(config.differential.enabled, before.differential.enabled);
    assert_eq!(config.differential.seed, 7);
    assert_ne!(config.fault.enabled, before.fault.enabled);
    assert_eq!(config.fault.seed, 9);
    assert_eq!(config.fault.max_fail_points, 8);
    assert_eq!(config.mir.mode, "off");
    assert_eq!(config.mir.toolchain, "beta");

    assert_ne!(config.platform.check_default, before.platform.check_default);
    assert_ne!(config.platform.check_no_default, before.platform.check_no_default);
    assert_ne!(config.platform.check_all_features, before.platform.check_all_features);
    assert_ne!(config.platform.check_each_feature, before.platform.check_each_feature);
    assert_ne!(config.platform.check_msrv, before.platform.check_msrv);
    assert_ne!(config.build.deny_network, before.build.deny_network);
    assert_eq!(config.build.process_spawn, "allow");
    assert_ne!(config.layout.critical_requires_repr, before.layout.critical_requires_repr);
    assert_ne!(config.ffi.require_safety_docs, before.ffi.require_safety_docs);
    assert_ne!(config.ffi.deny_panic_across_boundary, before.ffi.deny_panic_across_boundary);

    assert_ne!(config.hardware.enabled, before.hardware.enabled);
    assert_eq!(config.hardware.interrupt_stack_budget_bytes, 1024);
    assert_ne!(config.hardware.deny_heap_in_interrupts, before.hardware.deny_heap_in_interrupts);
    assert_ne!(config.performance.enabled, before.performance.enabled);
    assert_eq!(config.performance.false_sharing, "deny");
    assert_eq!(config.performance.instruction_warn_percent, 11.0);
    assert_eq!(config.performance.instruction_deny_percent, 26.0);
    assert_eq!(config.bloat.max_percent_growth, 6.0);
    assert_eq!(config.bloat.max_absolute_growth_bytes, 300_000);
    assert_ne!(config.hardening.enabled, before.hardening.enabled);
    assert_ne!(config.hardening.release_overflow_checks, before.hardening.release_overflow_checks);
    assert_ne!(config.hardening.require_pie, before.hardening.require_pie);
    assert_ne!(config.hardening.require_full_relro, before.hardening.require_full_relro);

    assert_eq!(config.snapshots.ci_updates, "allow");
    assert_eq!(config.snapshots.pending, "allow");
    assert_ne!(
        config.documentation.critical_requires_example,
        before.documentation.critical_requires_example
    );
    assert_ne!(config.documentation.run_doctests, before.documentation.run_doctests);
    assert_ne!(config.documentation.check_examples, before.documentation.check_examples);
    assert_ne!(config.dependencies.run_cargo_deny, before.dependencies.run_cargo_deny);
    assert_ne!(config.dependencies.run_unused, before.dependencies.run_unused);
    assert_ne!(config.dependencies.deny_wildcards, before.dependencies.deny_wildcards);
    assert_ne!(config.api.run_semver_checks, before.api.run_semver_checks);
    assert_ne!(config.generated.verify, before.generated.verify);
    assert_ne!(config.reproducibility.enabled, before.reproducibility.enabled);
    assert_eq!(config.reproducibility.runs, 3);
    assert_ne!(config.self_hardening.enabled, before.self_hardening.enabled);

    assert_eq!(config.viewer.command, "rustc");
    assert_eq!(config.viewer.args, vec!["--version", "{path}"]);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn exceptions_menu_persists_valid_addition_then_removes_it() {
    let root = workspace("exceptions");
    let add =
        run(&root, &["exceptions"], "a\nQA-X-001\nsrc/*.rs\ndocumented reason\n2999-01-01\nb\n");
    assert!(add.status.success(), "{}", String::from_utf8_lossy(&add.stderr));
    let stdout = String::from_utf8_lossy(&add.stdout);
    for expected in ["Exceptions", "Rule ID:", "Path/glob:", "Reason:", "Expires (YYYY-MM-DD):"] {
        assert!(stdout.contains(expected), "missing exception interaction: {expected}");
    }
    let config = QaConfig::load(&root).unwrap();
    assert_eq!(config.exception.len(), 1);
    let saved = &config.exception[0];
    assert_eq!(saved.rule, "QA-X-001");
    assert_eq!(saved.path, "src/*.rs");
    assert_eq!(saved.reason, "documented reason");
    assert_eq!(saved.expires, "2999-01-01");

    let remove = run(&root, &["exceptions"], "r\n1\nb\n");
    assert!(remove.status.success(), "{}", String::from_utf8_lossy(&remove.stderr));
    assert!(String::from_utf8_lossy(&remove.stdout).contains("Visible exception # to remove:"));
    assert!(QaConfig::load(&root).unwrap().exception.is_empty());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn interactive_dashboard_routes_every_top_level_menu_through_the_real_binary() {
    let root = workspace("dashboard");
    let input = concat!(
        "1\nb\n", "2\nb\n", "3\nb\n", "4\nb\n", "5\nb\n", "6\nb\n", "7\nb\n", "8\n\n", "r\nb\n",
        "s\nb\n", "e\nb\n", "q\n",
    );
    let output = run(&root, &["--interactive"], input);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    for heading in [
        "LOC details",
        "Cyclomatic complexity",
        "CRAP",
        "Tests (lowest known coverage first)",
        "Duplicate groups",
        "Dead/unreachable items",
        "Findings",
        "Evidence",
        "Generated reports",
        "Settings",
        "Exceptions",
    ] {
        assert!(stdout.contains(heading), "missing dashboard route: {heading}");
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn interactive_dashboard_opens_real_file_function_test_and_report_rows() {
    let root = workspace("dashboard-select");
    let repeated = concat!(
        "fn repeated_fixture() {\n",
        "    let a = 1;\n",
        "    let b = 2;\n",
        "    let c = a + b;\n",
        "    let d = c + 1;\n",
        "    let e = d + 1;\n",
        "    let f = e + 1;\n",
        "    let _ = f;\n",
        "}\n",
    );
    fs::write(root.join("src/a.rs"), repeated).unwrap();
    fs::write(root.join("src/b.rs"), repeated).unwrap();
    fs::write(root.join("src/risky.rs"), "fn unused_risky() { panic!(\"dashboard finding\"); }\n")
        .unwrap();

    let mut config = QaConfig::default();
    config.viewer.command = "rustc".into();
    config.viewer.args = vec!["--version".into()];
    config.save(&root.join("qa.toml")).unwrap();

    let input = concat!(
        "1\n1\n1\nb\n",
        "2\n1\n1\nb\n",
        "3\n1\n1\nb\n",
        "4\n1\n",
        "5\n1\n1\n",
        "6\n1\n",
        "7\n1\n",
        "r\n1\nb\n",
        "q\n",
    );
    let output = run(&root, &["--interactive"], input);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Files by LOC"));
    assert!(stdout.contains("Cyclomatic complexity"));
    assert!(stdout.contains("CRAP"));
    assert!(stdout.contains("Tests (lowest known coverage first)"));
    assert!(stdout.contains("Duplicate groups"));
    assert!(stdout.contains("Dead/unreachable items"));
    assert!(stdout.contains("Findings"));
    assert!(stdout.contains("Generated reports"));
    assert!(stdout.contains("% similar |"), "duplicate group rows were not rendered");
    assert!(stdout.contains("QA-SAFE-003"), "finding rows were not rendered");
    assert!(stdout.contains("unused_risky"), "dead-code rows were not rendered");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn reports_command_routes_to_the_reports_menu() {
    let root = workspace("reports-command");
    fs::create_dir_all(root.join("qa-out")).unwrap();
    fs::write(root.join("qa-out/report.json"), "{}\n").unwrap();
    let output = run(&root, &["reports"], "b\n");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Generated reports"));
    assert!(stdout.contains("report.json"));
    assert!(stdout.contains("A. Open full report.json"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn external_project_mode_keeps_transient_state_out_of_the_project() {
    let project = workspace("external-project");
    let launcher = workspace("external-launcher");
    let output = launcher.join("custom-reports");
    let project_arg = project.to_str().unwrap();
    let output_arg = output.to_str().unwrap();
    let result =
        run_from(&launcher, &["--project-dir", project_arg, "--output-dir", output_arg], "");
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    assert!(output.join("report.json").is_file());
    assert!(output.join("summary.txt").is_file());
    assert!(output.join("state/coverage").is_dir());
    assert!(output.join("state/build/target").is_dir());
    assert!(!project.join("qa-out").exists());
    assert!(!project.join("mutants.out").exists());
    assert!(!project.join("target").exists());
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(stdout.contains(output_arg));
    fs::remove_dir_all(project).unwrap();
    fs::remove_dir_all(launcher).unwrap();
}
