use std::{
    cell::RefCell,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Output, Stdio},
    thread,
    time::Duration,
};

mod cargo;
mod watch;

thread_local! {
    static CARGO_TARGET_DIR_OVERRIDE: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

pub fn with_cargo_target_dir<T>(target: Option<&Path>, operation: impl FnOnce() -> T) -> T {
    let previous =
        CARGO_TARGET_DIR_OVERRIDE.with(|slot| slot.replace(target.map(Path::to_path_buf)));
    let _reset = CargoTargetReset(previous);
    operation()
}

struct CargoTargetReset(Option<PathBuf>);

impl Drop for CargoTargetReset {
    fn drop(&mut self) {
        CARGO_TARGET_DIR_OVERRIDE.with(|slot| {
            slot.replace(self.0.take());
        });
    }
}

fn apply_runtime_env(command: &mut Command) {
    CARGO_TARGET_DIR_OVERRIDE.with(|slot| {
        if let Some(path) = slot.borrow().as_ref() {
            command.env("CARGO_TARGET_DIR", path);
        }
    });
}

pub fn run_system_cargo(
    workspace: &Path,
    args: &[String],
    envs: &[(&str, String)],
) -> io::Result<Output> {
    let mut command = Command::new("cargo");
    command.current_dir(workspace).args(args);
    apply_runtime_env(&mut command);
    clear_conflicting_rustflags(&mut command, envs);
    apply_envs(&mut command, envs);
    execute(command, &command_label("cargo", args), None)
}

pub fn run(
    workspace: &Path,
    program: &str,
    args: &[String],
    envs: &[(&str, String)],
) -> io::Result<Output> {
    let mut command = workspace_program_command(workspace, program, args)?;
    apply_runtime_env(&mut command);
    clear_conflicting_rustflags(&mut command, envs);
    apply_envs(&mut command, envs);
    execute(command, &command_label(program, args), None)
}

pub fn run_with_completion_watch<F>(
    workspace: &Path,
    program: &str,
    args: &[String],
    envs: &[(&str, String)],
    complete: F,
    grace: Duration,
) -> io::Result<Output>
where
    F: Fn() -> bool,
{
    let mut command = workspace_program_command(workspace, program, args)?;
    apply_runtime_env(&mut command);
    clear_conflicting_rustflags(&mut command, envs);
    apply_envs(&mut command, envs);
    watch::execute(command, &command_label(program, args), &complete, grace)
}

fn workspace_program_command(
    workspace: &Path,
    program: &str,
    args: &[String],
) -> io::Result<Command> {
    if program == "cargo" {
        return cargo::workspace_command(workspace, args);
    }
    let mut command = Command::new(program);
    command.current_dir(workspace).args(args);
    Ok(command)
}

fn clear_conflicting_rustflags(command: &mut Command, envs: &[(&str, String)]) {
    if envs.iter().any(|(key, _)| *key == "CARGO_ENCODED_RUSTFLAGS") {
        command.env_remove("RUSTFLAGS");
    }
    if envs.iter().any(|(key, _)| *key == "RUSTFLAGS") {
        command.env_remove("CARGO_ENCODED_RUSTFLAGS");
    }
}

fn apply_envs(command: &mut Command, envs: &[(&str, String)]) {
    for (key, value) in envs {
        command.env(key, value);
    }
}

pub fn run_shell(workspace: &Path, command: &str, envs: &[(&str, String)]) -> io::Result<Output> {
    let mut child = shell_command(command);
    child.current_dir(workspace);
    apply_runtime_env(&mut child);
    apply_envs(&mut child, envs);
    execute(child, &format!("shell: {}", truncate(command, 96)), None)
}

pub fn run_shell_with_input(workspace: &Path, command: &str, input: &[u8]) -> io::Result<Output> {
    let mut child = shell_command(command);
    child.current_dir(workspace);
    apply_runtime_env(&mut child);
    execute(child, &format!("shell: {}", truncate(command, 96)), Some(input))
}

fn shell_command(command: &str) -> Command {
    #[cfg(windows)]
    let builder = {
        let mut builder = Command::new("cmd");
        builder.arg("/C").arg(command);
        builder
    };
    #[cfg(not(windows))]
    let builder = {
        let mut builder = Command::new("sh");
        builder.arg("-c").arg(command);
        builder
    };
    builder
}

fn execute(command: Command, label: &str, input: Option<&[u8]>) -> io::Result<Output> {
    if let Some(control) = super::control::current() {
        execute_controlled(command, label, input, control)
    } else {
        execute_plain(command, input)
    }
}

fn execute_plain(mut command: Command, input: Option<&[u8]>) -> io::Result<Output> {
    if let Some(input) = input {
        command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = command.spawn()?;
        let stdin =
            child.stdin.take().ok_or_else(|| io::Error::other("child stdin unavailable"))?;
        write_input(stdin, input)?;
        return child.wait_with_output();
    }
    command.output()
}

struct StreamReaders {
    stdout: Option<thread::JoinHandle<Vec<u8>>>,
    stderr: Option<thread::JoinHandle<Vec<u8>>>,
}

fn execute_controlled(
    mut command: Command,
    label: &str,
    input: Option<&[u8]>,
    control: super::control::RunControl,
) -> io::Result<Output> {
    reject_skipped_category(&control)?;
    control.set_item(label);
    configure_controlled_command(&mut command, input.is_some());
    let mut child = command.spawn()?;
    if let Some(input) = input {
        let stdin =
            child.stdin.take().ok_or_else(|| io::Error::other("child stdin unavailable"))?;
        write_input(stdin, input)?;
    }
    control.set_process_active(true);
    let readers = take_stream_readers(&mut child, Some(&control));
    monitor_child(&mut child, label, &control, &readers)?;
    let status = child.wait()?;
    control.set_process_active(false);
    Ok(finish_output(status, readers))
}

fn reject_skipped_category(control: &super::control::RunControl) -> io::Result<()> {
    if control.should_skip_category() {
        return Err(skipped_error("category skipped by user"));
    }
    Ok(())
}

fn configure_controlled_command(command: &mut Command, has_input: bool) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        command.creation_flags(CREATE_NEW_PROCESS_GROUP);
    }
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    if has_input {
        command.stdin(Stdio::piped());
    }
}

fn take_stream_readers(
    child: &mut std::process::Child,
    control: Option<&super::control::RunControl>,
) -> StreamReaders {
    let control = control.cloned();
    StreamReaders {
        stdout: child.stdout.take().map(|stream| read_stream(stream, control.clone())),
        stderr: child.stderr.take().map(|stream| read_stream(stream, control)),
    }
}

fn monitor_child(
    child: &mut std::process::Child,
    label: &str,
    control: &super::control::RunControl,
    readers: &StreamReaders,
) -> io::Result<()> {
    let pid = child.id();
    let mut suspended = false;
    loop {
        if skip_requested(control) {
            return Err(interrupt_child(child, pid, suspended, control, readers));
        }
        suspended = sync_pause_state(child, pid, label, suspended, control, readers)?;
        if child.try_wait()?.is_some() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(80));
    }
}

fn skip_requested(control: &super::control::RunControl) -> bool {
    control.take_skip_current() || control.should_skip_category()
}

fn sync_pause_state(
    child: &mut std::process::Child,
    pid: u32,
    label: &str,
    suspended: bool,
    control: &super::control::RunControl,
    readers: &StreamReaders,
) -> io::Result<bool> {
    let paused = control.is_paused();
    if paused == suspended {
        return Ok(suspended);
    }
    if paused {
        return pause_child(pid, label, control);
    }
    resume_child(child, pid, label, control, readers)
}

fn pause_child(pid: u32, label: &str, control: &super::control::RunControl) -> io::Result<bool> {
    if let Err(error) = suspend_process_tree(pid) {
        control.resume();
        control.set_item(&format!("pause unavailable: {}", truncate(&error.to_string(), 80)));
        return Ok(false);
    }
    control.set_item(&format!("paused: {label}"));
    Ok(true)
}

fn resume_child(
    child: &mut std::process::Child,
    pid: u32,
    label: &str,
    control: &super::control::RunControl,
    readers: &StreamReaders,
) -> io::Result<bool> {
    if let Err(error) = resume_process_tree(pid) {
        abort_child(child, pid, control, readers);
        return Err(io::Error::other(format!("failed to resume process tree: {error}")));
    }
    control.set_item(label);
    Ok(false)
}

fn interrupt_child(
    child: &mut std::process::Child,
    pid: u32,
    suspended: bool,
    control: &super::control::RunControl,
    readers: &StreamReaders,
) -> io::Error {
    if suspended {
        let _resume_result = resume_process_tree(pid);
    }
    abort_child(child, pid, control, readers);
    control.set_item("skipped by user");
    skipped_error(skip_message(control))
}

fn abort_child(
    child: &mut std::process::Child,
    pid: u32,
    control: &super::control::RunControl,
    readers: &StreamReaders,
) {
    let _terminate_result = terminate_process_tree(pid);
    let _wait_result = child.wait();
    control.set_process_active(false);
    drain_streams(readers);
}

fn skip_message(control: &super::control::RunControl) -> &'static str {
    if control.should_skip_category() {
        "category skipped by user"
    } else {
        "current test skipped by user"
    }
}

fn drain_streams(readers: &StreamReaders) {
    wait_stream(&readers.stdout);
    wait_stream(&readers.stderr);
}

fn finish_output(status: ExitStatus, readers: StreamReaders) -> Output {
    let stdout = join_stream(readers.stdout);
    let stderr = join_stream(readers.stderr);
    Output { status, stdout, stderr }
}

fn write_input(mut stdin: std::process::ChildStdin, input: &[u8]) -> io::Result<()> {
    stdin.write_all(input)
}

fn read_stream(
    stream: impl Read + Send + 'static,
    control: Option<super::control::RunControl>,
) -> thread::JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut reader = io::BufReader::new(stream);
        let mut bytes = Vec::new();
        let mut line = Vec::new();
        loop {
            line.clear();
            match io::BufRead::read_until(&mut reader, b'\n', &mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    bytes.extend_from_slice(&line);
                    if let Some(control) = &control {
                        update_stream_status(control, &line);
                    }
                }
            }
        }
        bytes
    })
}

fn update_stream_status(control: &super::control::RunControl, line: &[u8]) {
    let status = String::from_utf8_lossy(line).trim().to_string();
    if !status.is_empty() {
        control.set_item(&truncate(&status, 110));
    }
}

fn wait_stream(handle: &Option<thread::JoinHandle<Vec<u8>>>) {
    let Some(reader) = handle.as_ref() else { return };
    loop {
        if reader.is_finished() {
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn join_stream(handle: Option<thread::JoinHandle<Vec<u8>>>) -> Vec<u8> {
    handle.and_then(|handle| handle.join().ok()).unwrap_or_default()
}

fn skipped_error(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::Interrupted, message)
}

fn command_label(program: &str, args: &[String]) -> String {
    if args.is_empty() {
        program.to_string()
    } else {
        truncate(&format!("{program} {}", args.join(" ")), 110)
    }
}

fn truncate(value: &str, max: usize) -> String {
    let mut chars = value.chars();
    let prefix = chars.by_ref().take(max).collect::<String>();
    if chars.next().is_some() { format!("{prefix}…") } else { prefix }
}

fn suspend_process_tree(pid: u32) -> io::Result<()> {
    #[cfg(unix)]
    {
        signal_group(pid, "-STOP")
    }
    #[cfg(windows)]
    {
        run_windows_process_control(pid, "suspend")
    }
}

fn resume_process_tree(pid: u32) -> io::Result<()> {
    #[cfg(unix)]
    {
        signal_group(pid, "-CONT")
    }
    #[cfg(windows)]
    {
        run_windows_process_control(pid, "resume")
    }
}

fn terminate_process_tree(pid: u32) -> io::Result<()> {
    #[cfg(unix)]
    {
        signal_group(pid, "-KILL")
    }
    #[cfg(windows)]
    {
        terminate_windows_process_tree(pid)
    }
}

fn terminate_descendants(pid: u32) -> io::Result<()> {
    #[cfg(unix)]
    {
        signal_group(pid, "-KILL")
    }
    #[cfg(windows)]
    {
        run_windows_process_control(pid, "terminate-descendants")
    }
}

#[cfg(unix)]
fn signal_group(pid: u32, signal: &str) -> io::Result<()> {
    let status = Command::new("kill").arg(signal).arg(format!("-{pid}")).status()?;
    if status.success() { Ok(()) } else { Err(io::Error::other(format!("kill {signal} failed"))) }
}

#[cfg(windows)]
fn terminate_windows_process_tree(pid: u32) -> io::Result<()> {
    let pid_text = pid.to_string();
    let status = Command::new("taskkill")
        .args(["/PID", pid_text.as_str(), "/T", "/F"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    if status.success() {
        return Ok(());
    }
    run_windows_process_control(pid, "terminate")
}

#[cfg(windows)]
fn run_windows_process_control(pid: u32, mode: &str) -> io::Result<()> {
    const SCRIPT: &str = include_str!("windows_process_control.ps1");
    let status = Command::new("powershell.exe")
        .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command", SCRIPT])
        .env("QA_PROCESS_CONTROL_PID", pid.to_string())
        .env("QA_PROCESS_CONTROL_MODE", mode)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("unable to {mode} active process tree")))
    }
}

pub fn command_available(workspace: &Path, program: &str) -> bool {
    if program == "cargo" {
        return cargo::workspace_command(workspace, &["--version".into()])
            .and_then(|mut command| command.output())
            .is_ok();
    }
    Command::new(program).current_dir(workspace).arg("--version").output().is_ok()
}

pub(crate) fn diagnostics(stdout: &[u8], stderr: &[u8]) -> String {
    format!("stdout:\n{}\nstderr:\n{}", diagnostic_stream(stdout), diagnostic_stream(stderr))
}

fn diagnostic_stream(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let chars = text.chars().collect::<Vec<_>>();
    const HALF: usize = 1500;
    if chars.len() <= HALF * 2 {
        return chars.into_iter().collect();
    }
    let head = chars[..HALF].iter().collect::<String>();
    let tail = chars[chars.len() - HALF..].iter().collect::<String>();
    format!("{head}\n... command stream truncated ...\n{tail}")
}

#[cfg(test)]
mod tests;
