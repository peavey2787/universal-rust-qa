use crate::dashboard;
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, poll, read},
    execute,
    terminal::{
        BeginSynchronizedUpdate, Clear, ClearType, EndSynchronizedUpdate, EnterAlternateScreen,
        LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
    },
};
use qa_policy::QaConfig;
use qa_sdk::{QaRun, QaRunLayout, RUN_CATEGORY_COUNT, RunControl, RunOptions};
use std::{
    io::{self, Write},
    path::{Path, PathBuf},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

type Worker = JoinHandle<Result<QaRun, qa_sdk::QaSdkError>>;

pub fn run(
    workspace: &Path,
    options: RunOptions,
    layout: QaRunLayout,
) -> Result<QaRun, Box<dyn std::error::Error>> {
    let config = QaConfig::load(workspace)?;
    let control = RunControl::new(RUN_CATEGORY_COUNT);
    let worker = spawn_worker(workspace.to_path_buf(), options, layout, control.clone());
    drive_terminal(&config, &control, &worker)?;
    join_worker(worker)
}

fn spawn_worker(
    workspace: PathBuf,
    options: RunOptions,
    layout: QaRunLayout,
    control: RunControl,
) -> Worker {
    thread::spawn(move || {
        qa_sdk::run_workspace_with_progress_and_layout(&workspace, &options, &layout, &control)
    })
}

fn drive_terminal(config: &QaConfig, control: &RunControl, worker: &Worker) -> io::Result<()> {
    let mut terminal = TerminalGuard::enter()?;
    let mut next_render = Instant::now();
    while !worker.is_finished() {
        if Instant::now() >= next_render {
            terminal.render(config, control)?;
            next_render = Instant::now() + Duration::from_millis(250);
        }
        handle_input(control)?;
        thread::sleep(Duration::from_millis(50));
    }
    terminal.render(config, control)
}

fn join_worker(worker: Worker) -> Result<QaRun, Box<dyn std::error::Error>> {
    let result = worker.join().map_err(|_| io::Error::other("QA worker thread panicked"))?;
    result.map_err(Into::into)
}

fn handle_input(control: &RunControl) -> io::Result<()> {
    while poll(Duration::ZERO)? {
        handle_event(control, read()?);
    }
    Ok(())
}

fn handle_event(control: &RunControl, event: Event) {
    let Event::Key(key) = event else {
        return;
    };
    if key.kind == KeyEventKind::Press {
        apply_key(control, key);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyAction {
    TogglePause,
    SkipCurrent,
    SkipCategory,
}

const KEY_ACTIONS: &[(char, KeyAction)] = &[
    ('p', KeyAction::TogglePause),
    ('P', KeyAction::TogglePause),
    (' ', KeyAction::TogglePause),
    ('s', KeyAction::SkipCurrent),
    ('S', KeyAction::SkipCurrent),
    ('c', KeyAction::SkipCategory),
    ('C', KeyAction::SkipCategory),
];

fn apply_key(control: &RunControl, key: KeyEvent) {
    if let Some(action) = key_action(key) {
        apply_action(control, action);
    }
}

fn key_action(key: KeyEvent) -> Option<KeyAction> {
    if key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) {
        return None;
    }
    key_code_action(key.code)
}

fn key_code_action(code: KeyCode) -> Option<KeyAction> {
    match code {
        KeyCode::Char(value) => char_action(value),
        _ => None,
    }
}

fn char_action(value: char) -> Option<KeyAction> {
    KEY_ACTIONS.iter().find(|(key, _)| *key == value).map(|(_, action)| *action)
}

fn apply_action(control: &RunControl, action: KeyAction) {
    if action == KeyAction::TogglePause {
        apply_pause(control);
        return;
    }
    apply_skip(control, action);
}

fn apply_pause(control: &RunControl) {
    if control.snapshot().paused {
        control.resume();
    } else {
        control.pause();
    }
}

fn apply_skip(control: &RunControl, action: KeyAction) {
    if action == KeyAction::SkipCurrent {
        control.skip_current();
    } else {
        control.skip_category();
    }
}

struct TerminalGuard {
    stdout: io::Stdout,
}

impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        if let Err(error) = execute!(stdout, EnterAlternateScreen, Hide, Clear(ClearType::All)) {
            let _disable_result = disable_raw_mode();
            return Err(error);
        }
        Ok(Self { stdout })
    }

    fn render(&mut self, config: &QaConfig, control: &RunControl) -> io::Result<()> {
        disable_raw_mode()?;
        let render_result = self.render_cooked(config, control);
        let raw_result = enable_raw_mode();
        render_result?;
        raw_result
    }

    fn render_cooked(&mut self, config: &QaConfig, control: &RunControl) -> io::Result<()> {
        execute!(self.stdout, BeginSynchronizedUpdate, MoveTo(0, 0), Clear(ClearType::All))?;
        let output = dashboard::live_dashboard_text(config, &control.snapshot());
        self.stdout.write_all(output.as_bytes())?;
        self.stdout.flush()?;
        execute!(self.stdout, EndSynchronizedUpdate)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _screen_result = execute!(self.stdout, Show, LeaveAlternateScreen);
        let _raw_result = disable_raw_mode();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_controls_map_to_pause_and_skip_actions() {
        assert_eq!(
            key_action(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE)),
            Some(KeyAction::TogglePause)
        );
        assert_eq!(
            key_action(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE)),
            Some(KeyAction::TogglePause)
        );
        assert_eq!(
            key_action(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE)),
            Some(KeyAction::SkipCurrent)
        );
        assert_eq!(
            key_action(KeyEvent::new(KeyCode::Char('C'), KeyModifiers::SHIFT)),
            Some(KeyAction::SkipCategory)
        );
        assert_eq!(key_action(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)), None);
        assert_eq!(key_action(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)), None);
    }

    #[test]
    fn character_actions_cover_every_supported_key_and_reject_other_input() {
        for key in ['p', 'P', ' '] {
            assert_eq!(char_action(key), Some(KeyAction::TogglePause));
        }
        for key in ['s', 'S'] {
            assert_eq!(char_action(key), Some(KeyAction::SkipCurrent));
        }
        for key in ['c', 'C'] {
            assert_eq!(char_action(key), Some(KeyAction::SkipCategory));
        }
        assert_eq!(char_action('x'), None);
        assert_eq!(key_code_action(KeyCode::Esc), None);
    }

    #[test]
    fn control_actions_change_shared_run_state() {
        let control = RunControl::new(RUN_CATEGORY_COUNT);
        handle_event(&control, Event::Key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE)));
        assert!(control.snapshot().paused);
        apply_action(&control, KeyAction::TogglePause);
        assert!(!control.snapshot().paused);
        apply_action(&control, KeyAction::SkipCurrent);
        assert!(!control.skip_current());
        assert!(!control.snapshot().skip_category_pending);
        apply_action(&control, KeyAction::SkipCategory);
        assert!(control.snapshot().skip_category_pending);
        handle_event(&control, Event::Resize(80, 24));
    }
}
