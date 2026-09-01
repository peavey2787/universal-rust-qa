use super::super::*;
use std::time::Instant;

#[test]
fn input_writer_closes_owned_stdin_and_transmits_payload() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    #[cfg(windows)]
    let mut command = {
        let mut command = Command::new("cmd");
        command.args(["/C", "more"]);
        command
    };
    #[cfg(not(windows))]
    let mut command = {
        let mut command = Command::new("sh");
        command.args(["-c", "cat"]);
        command
    };
    command.current_dir(root).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
    let mut child = command.spawn().unwrap();
    let stdin = child.stdin.take().unwrap();
    let result = write_input(stdin, b"payload\n");
    let output = child.wait_with_output().unwrap();
    assert!(result.is_ok());
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("payload"));
}

#[test]
fn completion_watch_terminates_a_lingering_process_after_final_evidence() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let marker = done.clone();
    let setter = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(80));
        marker.store(true, std::sync::atomic::Ordering::SeqCst);
    });

    #[cfg(windows)]
    let mut command = {
        let mut command = Command::new("cmd");
        command.args(["/C", "ping -n 8 127.0.0.1 >NUL"]);
        command
    };
    #[cfg(not(windows))]
    let mut command = {
        let mut command = Command::new("sh");
        command.args(["-c", "sleep 5"]);
        command
    };
    command.current_dir(root);
    let started = Instant::now();
    let output = watch::execute(
        command,
        "watched-test",
        &|| done.load(std::sync::atomic::Ordering::SeqCst),
        Duration::from_millis(80),
    )
    .unwrap();
    setter.join().unwrap();
    assert!(!output.status.success());
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[test]
fn completion_grace_boundary_is_inclusive_only_at_the_deadline() {
    let grace = Duration::from_millis(100);
    assert!(!watch::grace_expired(Duration::from_millis(99), grace));
    assert!(watch::grace_expired(Duration::from_millis(100), grace));
    assert!(watch::grace_expired(Duration::from_millis(101), grace));
}

#[test]
fn bounded_stream_wait_distinguishes_finished_and_lingering_readers() {
    let finished = StreamReaders { stdout: Some(std::thread::spawn(Vec::<u8>::new)), stderr: None };
    assert!(watch::streams_finished_within(&finished, Duration::from_secs(1)));
    let _ = join_stream(finished.stdout);

    let lingering = StreamReaders {
        stdout: Some(std::thread::spawn(|| {
            std::thread::sleep(Duration::from_millis(80));
            Vec::new()
        })),
        stderr: None,
    };
    assert!(!watch::streams_finished_within(&lingering, Duration::from_millis(10)));
    let _ = join_stream(lingering.stdout);
}

#[test]
fn completion_probe_interval_is_inclusive_only_at_one_second() {
    assert!(!watch::completion_probe_due(Duration::from_millis(999)));
    assert!(watch::completion_probe_due(Duration::from_secs(1)));
    assert!(watch::completion_probe_due(Duration::from_millis(1_001)));
}

#[test]
fn completion_probe_requires_unfinalized_state_and_due_interval() {
    assert!(watch::completion_probe_allowed(false, None));
    assert!(!watch::completion_probe_allowed(true, None));
    assert!(!watch::completion_probe_allowed(false, Some(Duration::from_millis(999))));
    assert!(watch::completion_probe_allowed(false, Some(Duration::from_secs(1))));
    assert!(!watch::completion_probe_allowed(true, Some(Duration::from_secs(1))));
}

#[test]
fn finalized_process_termination_kills_the_root_process() {
    let mut command = long_running_command();
    command.stdout(Stdio::null()).stderr(Stdio::null());
    let mut child = command.spawn().unwrap();
    let pid = child.id();

    watch::terminate_finalized_process(&mut child, pid, false, None);
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut exited = child.try_wait().unwrap().is_some();
    while !exited && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(25));
        exited = child.try_wait().unwrap().is_some();
    }
    if !exited {
        let _ = child.kill();
        let _ = child.wait();
    }
    assert!(exited, "finalized-process cleanup must terminate the root process");
}

fn long_running_command() -> Command {
    #[cfg(windows)]
    {
        let mut command = Command::new("cmd");
        command.args(["/C", "ping -n 30 127.0.0.1 >NUL"]);
        command
    }
    #[cfg(not(windows))]
    {
        let mut command = Command::new("sh");
        command.args(["-c", "sleep 30"]);
        command
    }
}

#[cfg(windows)]
#[test]
fn windows_descendant_cleanup_kills_child_while_parent_remains_alive() {
    use std::io::BufRead;

    let script = concat!(
        "$p = Start-Process -FilePath 'cmd.exe' ",
        "-ArgumentList '/C','ping -n 30 127.0.0.1 >NUL' ",
        "-WindowStyle Hidden -PassThru; Write-Output $p.Id; ",
        "[Console]::Out.Flush(); Start-Sleep -Seconds 30"
    );
    let mut parent = Command::new("powershell.exe")
        .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command", script])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let parent_pid = parent.id();
    let mut stdout = std::io::BufReader::new(parent.stdout.take().unwrap());
    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    let child_pid = line.trim().parse::<u32>().unwrap();

    let terminated = terminate_descendants(parent_pid);
    let child_gone = wait_for_windows_process_exit(child_pid, Duration::from_secs(2));
    let parent_alive = parent.try_wait().unwrap().is_none();

    if !child_gone {
        let child_pid_text = child_pid.to_string();
        let _ = Command::new("taskkill")
            .args(["/PID", child_pid_text.as_str(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = parent.kill();
    let _ = parent.wait();

    assert!(terminated.is_ok());
    assert!(child_gone, "descendant process must be terminated");
    assert!(parent_alive, "descendant cleanup must not terminate the root process");
}

#[cfg(windows)]
fn wait_for_windows_process_exit(pid: u32, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if !windows_process_exists(pid) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    !windows_process_exists(pid)
}

#[cfg(windows)]
fn windows_process_exists(pid: u32) -> bool {
    let filter = format!("PID eq {pid}");
    let Ok(output) = Command::new("tasklist").args(["/FI", &filter, "/NH"]).output() else {
        return false;
    };
    let pid_text = pid.to_string();
    String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .any(|field| field == pid_text.as_str())
}
