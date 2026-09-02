use super::*;
use std::{
    io,
    process::{Command, ExitStatus, Output},
    thread,
    time::{Duration, Instant},
};

pub(super) fn execute<F>(
    command: Command,
    label: &str,
    complete: &F,
    grace: Duration,
) -> io::Result<Output>
where
    F: Fn() -> bool,
{
    if let Some(control) = crate::control::current() {
        execute_controlled(command, label, control, complete, grace)
    } else {
        execute_plain(command, complete, grace)
    }
}

fn execute_plain<F>(mut command: Command, complete: &F, grace: Duration) -> io::Result<Output>
where
    F: Fn() -> bool,
{
    configure_controlled_command(&mut command, false);
    let mut child = command.spawn()?;
    let readers = take_stream_readers(&mut child, None);
    let pid = child.id();
    let status = monitor_completion(&mut child, complete, grace, None)?;
    finish_output_bounded(status, readers, Duration::from_secs(10), pid)
}

fn execute_controlled<F>(
    mut command: Command,
    label: &str,
    control: crate::control::RunControl,
    complete: &F,
    grace: Duration,
) -> io::Result<Output>
where
    F: Fn() -> bool,
{
    reject_skipped_category(&control)?;
    control.set_item(label);
    configure_controlled_command(&mut command, false);
    let mut child = command.spawn()?;
    control.set_process_active(true);
    let readers = take_stream_readers(&mut child, Some(&control));
    let pid = child.id();
    let status =
        monitor_completion(&mut child, complete, grace, Some((&control, label, &readers)))?;
    control.set_process_active(false);
    finish_output_bounded(status, readers, Duration::from_secs(10), pid)
}

fn monitor_completion<F>(
    child: &mut std::process::Child,
    complete: &F,
    grace: Duration,
    controlled: Option<(&crate::control::RunControl, &str, &StreamReaders)>,
) -> io::Result<ExitStatus>
where
    F: Fn() -> bool,
{
    let pid = child.id();
    let mut finalized_at = None;
    let mut last_completion_check = None;
    let mut suspended = false;
    loop {
        if let Some(status) = child.try_wait()? {
            #[cfg(windows)]
            if finalized_at.is_some() {
                report_cleanup_error("terminate finalized descendants", terminate_descendants(pid));
            }
            return Ok(status);
        }
        if let Some((control, label, readers)) = controlled {
            if skip_requested(control) {
                return Err(interrupt_child(child, pid, suspended, control, readers));
            }
            suspended = sync_pause_state(child, pid, label, suspended, control, readers)?;
        }
        let since_last_check = last_completion_check.map(|checked: Instant| checked.elapsed());
        if completion_probe_allowed(finalized_at.is_some(), since_last_check) {
            last_completion_check = Some(Instant::now());
            if complete() {
                finalized_at = Some(Instant::now());
                #[cfg(windows)]
                report_cleanup_error("terminate mutation descendants", terminate_descendants(pid));
                if let Some((control, _, _)) = controlled {
                    control.set_item("mutation evidence finalized; waiting for process shutdown");
                }
            }
        }
        if finalized_at.is_some_and(|observed| grace_expired(observed.elapsed(), grace)) {
            terminate_finalized_process(child, pid, suspended, controlled);
            return child.wait();
        }
        thread::sleep(Duration::from_millis(80));
    }
}

pub(super) fn completion_probe_allowed(
    finalized: bool,
    since_last_check: Option<Duration>,
) -> bool {
    !finalized && since_last_check.is_none_or(completion_probe_due)
}

pub(super) fn completion_probe_due(elapsed: Duration) -> bool {
    elapsed >= Duration::from_secs(1)
}

pub(super) fn grace_expired(elapsed: Duration, grace: Duration) -> bool {
    elapsed >= grace
}

pub(super) fn terminate_finalized_process(
    child: &mut std::process::Child,
    pid: u32,
    suspended: bool,
    controlled: Option<(&crate::control::RunControl, &str, &StreamReaders)>,
) {
    if suspended {
        report_cleanup_error("resume finalized process tree", resume_process_tree(pid));
    }
    if let Some((control, _, _)) = controlled {
        control.set_item("mutation evidence finalized; terminating lingering process tree");
    }
    report_cleanup_error("terminate finalized process tree", terminate_process_tree(pid));
    if let Err(error) = child.kill() {
        if error.kind() != io::ErrorKind::InvalidInput {
            eprintln!("warning: failed to kill finalized child process {pid}: {error}");
        }
    }
}

fn report_cleanup_error(action: &str, result: io::Result<()>) {
    if let Err(error) = result {
        eprintln!("warning: {action} failed: {error}");
    }
}

fn finish_output_bounded(
    status: ExitStatus,
    readers: StreamReaders,
    timeout: Duration,
    pid: u32,
) -> io::Result<Output> {
    if streams_finished_within(&readers, timeout) {
        return Ok(finish_output(status, readers));
    }
    let cleanup = terminate_descendants(pid);
    if streams_finished_within(&readers, Duration::from_secs(2)) {
        return Ok(finish_output(status, readers));
    }
    let detail = cleanup.err().map_or(String::new(), |error| format!("; cleanup failed: {error}"));
    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        format!("command process ended but inherited output pipes remained open{detail}"),
    ))
}

pub(super) fn streams_finished_within(readers: &StreamReaders, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        let stdout_done = readers.stdout.as_ref().is_none_or(|reader| reader.is_finished());
        let stderr_done = readers.stderr.as_ref().is_none_or(|reader| reader.is_finished());
        if stdout_done && stderr_done {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(Duration::from_millis(10));
    }
}
