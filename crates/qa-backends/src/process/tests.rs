use super::*;

#[test]
fn process_runner_reports_success_failure_and_missing_program() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let ok = run(root, "rustc", &["--version".into()], &[]).unwrap();
    assert!(ok.status.success());

    let bad = run(root, "rustc", &["--definitely-invalid-option".into()], &[]).unwrap();
    assert!(!bad.status.success());

    assert!(run(root, "urqa-program-that-does-not-exist", &[], &[]).is_err());
    assert!(command_available(root, "rustc"));
    assert!(!command_available(root, "urqa-program-that-does-not-exist"));
}

#[test]
fn workspace_program_builder_routes_only_cargo_through_workspace_toolchain_resolution() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let rustc = workspace_program_command(root, "rustc", &["--version".into()]).unwrap();
    assert_eq!(rustc.get_program(), std::ffi::OsStr::new("rustc"));

    let cargo = workspace_program_command(root, "cargo", &["--version".into()]).unwrap();
    if Command::new("rustup").arg("--version").output().is_ok() {
        assert_eq!(cargo.get_program(), std::ffi::OsStr::new("rustup"));
    } else {
        assert_eq!(cargo.get_program(), std::ffi::OsStr::new("cargo"));
    }
}

#[test]
fn shell_runner_and_rustflag_environment_paths_execute() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let ok = run_shell(root, "exit 0", &[("URQA_TEST_VALUE", "yes".into())]).unwrap();
    assert!(ok.status.success());
    let bad = run_shell(root, "exit 9", &[]).unwrap();
    assert!(!bad.status.success());

    let rustflags = run(root, "rustc", &["--version".into()], &[("RUSTFLAGS", "".into())]).unwrap();
    assert!(rustflags.status.success());
    let encoded =
        run(root, "rustc", &["--version".into()], &[("CARGO_ENCODED_RUSTFLAGS", "".into())])
            .unwrap();
    assert!(encoded.status.success());
}

#[test]
fn diagnostics_preserve_both_streams_and_failure_tails() {
    let stdout = format!("{}test-panic-at-tail", "test progress\n".repeat(500));
    let stderr = format!("{}failure-at-tail", "compiler-progress\n".repeat(500));
    let detail = diagnostics(stdout.as_bytes(), stderr.as_bytes());
    assert!(detail.contains("stdout:"));
    assert!(detail.contains("stderr:"));
    assert!(detail.contains("command stream truncated"));
    assert!(detail.contains("test-panic-at-tail"));
    assert!(detail.contains("failure-at-tail"));
}

#[test]
fn shell_input_runner_roundtrips_stdin() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    #[cfg(windows)]
    let command = "more";
    #[cfg(not(windows))]
    let command = "cat";
    let output = run_shell_with_input(root, command, b"alpha\nbeta\n").unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("alpha"));
    assert!(stdout.contains("beta"));
}

#[test]
fn controlled_runner_can_pause_resume_and_skip_current_process_tree() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let control = crate::control::RunControl::default();
    control.begin_category("control-test");
    let trigger = control.clone();
    let controller = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(180));
        trigger.pause();
        std::thread::sleep(Duration::from_millis(180));
        assert!(trigger.snapshot().paused);
        trigger.resume();
        std::thread::sleep(Duration::from_millis(180));
        trigger.skip_current();
    });

    #[cfg(windows)]
    let command = "ping -n 6 127.0.0.1 >NUL";
    #[cfg(not(windows))]
    let command = "sleep 5";
    let result = crate::control::with_control(&control, || run_shell(root, command, &[]));
    controller.join().unwrap();
    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::Interrupted);
    assert!(!control.snapshot().process_active);
}

#[test]
fn controlled_runner_preserves_success_output_and_clears_active_state() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let control = crate::control::RunControl::default();
    control.begin_category("controlled-success");

    #[cfg(windows)]
    let command = "echo controlled-success";
    #[cfg(not(windows))]
    let command = "printf controlled-success";
    let output = crate::control::with_control(&control, || run_shell(root, command, &[])).unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("controlled-success"));
    assert!(!control.snapshot().process_active);
}

#[test]
fn category_skip_prevents_new_child_processes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let control = crate::control::RunControl::default();
    control.begin_category("skip-category");
    control.skip_category();
    let result = crate::control::with_control(&control, || run_shell(root, "exit 0", &[]));
    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::Interrupted);
}

#[test]
fn controlled_runner_can_skip_an_active_category_fail_closed() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let control = crate::control::RunControl::default();
    control.begin_category("skip-active-category");
    let trigger = control.clone();
    let controller = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(180));
        trigger.skip_category();
    });

    #[cfg(windows)]
    let command = "ping -n 6 127.0.0.1 >NUL";
    #[cfg(not(windows))]
    let command = "sleep 5";
    let result = crate::control::with_control(&control, || run_shell(root, command, &[]));
    controller.join().unwrap();
    let error = result.unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    assert!(error.to_string().contains("category skipped"));
    assert!(!control.snapshot().process_active);
}

#[test]
fn command_labels_and_truncation_preserve_short_text_and_bound_long_text() {
    assert_eq!(command_label("cargo", &[]), "cargo");
    assert_eq!(command_label("cargo", &["test".into()]), "cargo test");
    assert_eq!(truncate("short", 10), "short");
    let long = truncate(&"x".repeat(20), 8);
    assert_eq!(long.chars().count(), 9);
    assert!(long.ends_with('…'));
}

#[test]
fn scoped_cargo_target_dir_is_applied_to_child_processes_and_explicit_env_wins() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let external = root.join("external-target-test");
    #[cfg(windows)]
    let echo_target = "echo %CARGO_TARGET_DIR%";
    #[cfg(not(windows))]
    let echo_target = "printf %s \"$CARGO_TARGET_DIR\"";

    let scoped =
        with_cargo_target_dir(Some(&external), || run_shell(root, echo_target, &[])).unwrap();
    assert!(scoped.status.success());
    assert_eq!(String::from_utf8_lossy(&scoped.stdout).trim(), external.display().to_string());

    let explicit = with_cargo_target_dir(Some(&external), || {
        run_shell(root, echo_target, &[("CARGO_TARGET_DIR", "explicit-target".into())])
    })
    .unwrap();
    assert_eq!(String::from_utf8_lossy(&explicit.stdout).trim(), "explicit-target");
}

#[test]
fn nested_target_overrides_restore_the_previous_thread_local_value() {
    let first = Path::new("first-target");
    let second = Path::new("second-target");
    assert!(CARGO_TARGET_DIR_OVERRIDE.with(|slot| slot.borrow().is_none()));
    with_cargo_target_dir(Some(first), || {
        assert_eq!(
            CARGO_TARGET_DIR_OVERRIDE.with(|slot| slot.borrow().clone()),
            Some(first.to_path_buf())
        );
        with_cargo_target_dir(Some(second), || {
            assert_eq!(
                CARGO_TARGET_DIR_OVERRIDE.with(|slot| slot.borrow().clone()),
                Some(second.to_path_buf())
            );
        });
        assert_eq!(
            CARGO_TARGET_DIR_OVERRIDE.with(|slot| slot.borrow().clone()),
            Some(first.to_path_buf())
        );
    });
    assert!(CARGO_TARGET_DIR_OVERRIDE.with(|slot| slot.borrow().is_none()));
}

#[test]
fn rustflag_conflict_cleanup_removes_only_the_competing_encoding() {
    let mut encoded = Command::new("rustc");
    encoded.env("RUSTFLAGS", "ambient-rustflags");
    clear_conflicting_rustflags(&mut encoded, &[("CARGO_ENCODED_RUSTFLAGS", "encoded".into())]);
    let encoded_env = encoded.get_envs().collect::<Vec<_>>();
    assert!(
        encoded_env
            .iter()
            .any(|(key, value)| *key == std::ffi::OsStr::new("RUSTFLAGS") && value.is_none())
    );

    let mut plain = Command::new("rustc");
    plain.env("CARGO_ENCODED_RUSTFLAGS", "ambient-encoded");
    clear_conflicting_rustflags(&mut plain, &[("RUSTFLAGS", "plain".into())]);
    let plain_env = plain.get_envs().collect::<Vec<_>>();
    assert!(plain_env.iter().any(|(key, value)| {
        *key == std::ffi::OsStr::new("CARGO_ENCODED_RUSTFLAGS") && value.is_none()
    }));

    let mut unrelated = Command::new("rustc");
    unrelated.env("RUSTFLAGS", "keep-me");
    clear_conflicting_rustflags(&mut unrelated, &[("OTHER", "value".into())]);
    assert!(unrelated.get_envs().any(|(key, value)| {
        key == std::ffi::OsStr::new("RUSTFLAGS") && value == Some(std::ffi::OsStr::new("keep-me"))
    }));
}

#[test]
fn explicit_environment_removals_override_ambient_values() {
    let mut command = Command::new("rustc");
    command.env("LIBCLANG_PATH", "ambient-clang");
    command.env("KEEP_ME", "yes");
    remove_envs_from_command(&mut command, &["LIBCLANG_PATH"]);
    let envs = command.get_envs().collect::<Vec<_>>();
    assert!(
        envs.iter().any(|(key, value)| {
            *key == std::ffi::OsStr::new("LIBCLANG_PATH") && value.is_none()
        })
    );
    assert!(envs.iter().any(|(key, value)| {
        *key == std::ffi::OsStr::new("KEEP_ME") && *value == Some(std::ffi::OsStr::new("yes"))
    }));
}

#[test]
fn diagnostic_stream_preserves_exact_boundary_and_truncates_one_character_over() {
    let boundary = "a".repeat(3000);
    assert_eq!(diagnostic_stream(boundary.as_bytes()), boundary);

    let over = format!("{}X{}", "a".repeat(1500), "z".repeat(1500));
    let truncated = diagnostic_stream(over.as_bytes());
    assert!(truncated.starts_with(&"a".repeat(1500)));
    assert!(truncated.ends_with(&"z".repeat(1500)));
    assert!(truncated.contains("... command stream truncated ..."));
    assert!(!truncated.contains('X'));
}

#[test]
fn stream_status_updates_only_for_nonempty_text() {
    let control = crate::control::RunControl::default();
    control.set_item("before");
    update_stream_status(&control, b"   \r\n");
    assert_eq!(control.snapshot().current_item, "before");
    update_stream_status(&control, b"new status\n");
    assert_eq!(control.snapshot().current_item, "new status");
}

#[test]
fn shell_builder_and_controlled_input_are_observable() {
    let command = shell_command("echo qa-shell");
    #[cfg(windows)]
    {
        assert_eq!(command.get_program(), "cmd");
        assert_eq!(
            command.get_args().map(|arg| arg.to_string_lossy().into_owned()).collect::<Vec<_>>(),
            vec!["/C", "echo qa-shell"]
        );
    }
    #[cfg(not(windows))]
    {
        assert_eq!(command.get_program(), "sh");
        assert_eq!(
            command.get_args().map(|arg| arg.to_string_lossy().into_owned()).collect::<Vec<_>>(),
            vec!["-c", "echo qa-shell"]
        );
    }

    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let control = crate::control::RunControl::default();
    control.begin_category("controlled-input");
    #[cfg(windows)]
    let echo_input = "more";
    #[cfg(not(windows))]
    let echo_input = "cat";
    let output = crate::control::with_control(&control, || {
        run_shell_with_input(root, echo_input, b"controlled-input-marker\n")
    })
    .unwrap();
    assert!(String::from_utf8_lossy(&output.stdout).contains("controlled-input-marker"));
}

#[test]
fn skipped_category_and_pause_state_helpers_have_exact_results() {
    let control = crate::control::RunControl::default();
    control.begin_category("helper-state");
    assert!(reject_skipped_category(&control).is_ok());
    control.skip_category();
    assert_eq!(reject_skipped_category(&control).unwrap_err().kind(), io::ErrorKind::Interrupted);

    let control = crate::control::RunControl::default();
    let mut child = shell_command("exit 0").spawn().unwrap();
    let readers = StreamReaders { stdout: None, stderr: None };
    let pid = child.id();
    assert!(!sync_pause_state(&mut child, pid, "state", false, &control, &readers).unwrap());
    control.pause();
    assert!(sync_pause_state(&mut child, pid, "state", true, &control, &readers).unwrap());
    let _ = child.wait();
}

#[test]
fn stream_wait_helpers_really_wait_for_reader_completion() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    let completed = Arc::new(AtomicUsize::new(0));
    let one = completed.clone();
    let stdout = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(60));
        one.fetch_add(1, Ordering::SeqCst);
        Vec::new()
    });
    wait_stream(&Some(stdout));
    assert_eq!(completed.load(Ordering::SeqCst), 1);

    let left = completed.clone();
    let right = completed.clone();
    let readers = StreamReaders {
        stdout: Some(std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(60));
            left.fetch_add(1, Ordering::SeqCst);
            Vec::new()
        })),
        stderr: Some(std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(60));
            right.fetch_add(1, Ordering::SeqCst);
            Vec::new()
        })),
    };
    drain_streams(&readers);
    assert_eq!(completed.load(Ordering::SeqCst), 3);
}

#[test]
fn process_tree_controls_fail_closed_for_a_missing_process() {
    let missing = u32::MAX;
    assert!(suspend_process_tree(missing).is_err());
    assert!(resume_process_tree(missing).is_err());
    assert!(terminate_process_tree(missing).is_err());

    #[cfg(unix)]
    assert!(signal_group(missing, "-STOP").is_err());
    #[cfg(windows)]
    {
        assert!(windows::suspend_process_tree(missing).is_err());
        assert!(windows::terminate_process_tree(missing).is_err());
        assert!(windows::terminate_descendants(missing).is_ok());
    }
}

#[test]
fn pause_and_resume_helpers_change_a_live_process_and_control_state() {
    let control = crate::control::RunControl::default();
    control.begin_category("pause-resume-helper");
    #[cfg(windows)]
    let command_text = "ping -n 4 127.0.0.1 >NUL";
    #[cfg(not(windows))]
    let command_text = "sleep 3";

    let mut command = shell_command(command_text);
    configure_controlled_command(&mut command, false);
    let mut child = command.spawn().unwrap();
    let pid = child.id();
    let readers = StreamReaders { stdout: None, stderr: None };

    control.pause();
    assert!(pause_child(pid, "active", &control).unwrap());
    assert_eq!(control.snapshot().current_item, "paused: active");

    control.resume();
    assert!(!resume_child(&mut child, pid, "active", &control, &readers).unwrap());
    assert_eq!(control.snapshot().current_item, "active");

    terminate_process_tree(pid).unwrap();
    let _ = child.wait();
}

#[test]
fn pause_and_resume_helpers_fail_closed_when_process_control_is_unavailable() {
    let control = crate::control::RunControl::default();
    control.pause();
    assert!(!pause_child(u32::MAX, "missing", &control).unwrap());
    assert!(!control.snapshot().paused);

    let mut child = shell_command("exit 0").spawn().unwrap();
    let readers = StreamReaders { stdout: None, stderr: None };
    assert!(resume_child(&mut child, u32::MAX, "missing", &control, &readers).is_err());
    let _ = child.wait();
}

mod watchdog;
