use std::{
    cell::RefCell,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

#[derive(Debug, Clone, Default)]
pub struct RunControl {
    inner: Arc<ControlState>,
}

#[derive(Debug, Default)]
struct ControlState {
    paused: AtomicBool,
    skip_current: AtomicBool,
    skip_category: AtomicBool,
    current_category: Mutex<String>,
    current_item: Mutex<String>,
    process_active: AtomicBool,
}

#[derive(Debug, Clone, Default)]
pub struct ControlSnapshot {
    pub paused: bool,
    pub skip_category: bool,
    pub current_category: String,
    pub current_item: String,
    pub process_active: bool,
}

thread_local! {
    static ACTIVE: RefCell<Option<RunControl>> = const { RefCell::new(None) };
}

impl RunControl {
    pub fn pause(&self) {
        self.inner.paused.store(true, Ordering::Release);
    }

    pub fn resume(&self) {
        self.inner.paused.store(false, Ordering::Release);
    }

    pub fn skip_current(&self) {
        self.inner.skip_current.store(true, Ordering::Release);
        set_locked(&self.inner.current_item, "skip current requested");
    }

    pub fn skip_category(&self) {
        self.inner.skip_category.store(true, Ordering::Release);
        set_locked(&self.inner.current_item, "skip category requested");
    }

    pub fn snapshot(&self) -> ControlSnapshot {
        ControlSnapshot {
            paused: self.inner.paused.load(Ordering::Acquire),
            skip_category: self.inner.skip_category.load(Ordering::Acquire),
            current_category: lock_clone(&self.inner.current_category),
            current_item: lock_clone(&self.inner.current_item),
            process_active: self.inner.process_active.load(Ordering::Acquire),
        }
    }

    pub fn begin_category(&self, category: &str) {
        self.inner.skip_current.store(false, Ordering::Release);
        self.inner.skip_category.store(false, Ordering::Release);
        set_locked(&self.inner.current_category, category);
        set_locked(&self.inner.current_item, "preparing");
        self.wait_if_paused();
    }

    pub fn finish_category(&self) {
        self.inner.skip_current.store(false, Ordering::Release);
        self.inner.skip_category.store(false, Ordering::Release);
        set_locked(&self.inner.current_item, "complete");
    }

    pub(crate) fn take_skip_current(&self) -> bool {
        self.inner.skip_current.swap(false, Ordering::AcqRel)
    }

    pub(crate) fn should_skip_category(&self) -> bool {
        self.inner.skip_category.load(Ordering::Acquire)
    }

    pub(crate) fn is_paused(&self) -> bool {
        self.inner.paused.load(Ordering::Acquire)
    }

    pub(crate) fn set_item(&self, item: &str) {
        set_locked(&self.inner.current_item, item);
    }

    pub(crate) fn set_process_active(&self, active: bool) {
        self.inner.process_active.store(active, Ordering::Release);
    }

    fn wait_if_paused(&self) {
        while self.is_paused() {
            thread::sleep(Duration::from_millis(80));
        }
    }
}

pub fn with_control<T>(control: &RunControl, operation: impl FnOnce() -> T) -> T {
    ACTIVE.with(|slot| {
        let previous = slot.replace(Some(control.clone()));
        let result = operation();
        slot.replace(previous);
        result
    })
}

pub(crate) fn current() -> Option<RunControl> {
    ACTIVE.with(|slot| slot.borrow().clone())
}

fn set_locked(target: &Mutex<String>, value: &str) {
    if let Ok(mut guard) = target.lock() {
        guard.clear();
        guard.push_str(value);
    }
}

fn lock_clone(target: &Mutex<String>) -> String {
    target.lock().map(|guard| guard.clone()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controls_toggle_pause_and_scope_skip_requests() {
        let control = RunControl::default();
        control.begin_category("coverage");
        assert_eq!(control.snapshot().current_category, "coverage");
        control.pause();
        assert!(control.snapshot().paused);
        control.resume();
        control.skip_current();
        assert!(control.take_skip_current());
        assert!(!control.take_skip_current());
        control.skip_category();
        assert!(control.snapshot().skip_category);
        control.finish_category();
        assert!(!control.snapshot().skip_category);
    }

    #[test]
    fn active_control_is_thread_local_and_restored() {
        let first = RunControl::default();
        let second = RunControl::default();
        first.begin_category("first");
        second.begin_category("second");
        with_control(&first, || {
            assert_eq!(current().unwrap().snapshot().current_category, "first");
            with_control(&second, || {
                assert_eq!(current().unwrap().snapshot().current_category, "second");
            });
            assert_eq!(current().unwrap().snapshot().current_category, "first");
        });
        assert!(current().is_none());
    }

    #[test]
    fn snapshots_track_active_items_and_paused_category_boundaries() {
        let control = RunControl::default();
        control.set_item("starting");
        control.set_process_active(true);
        let snapshot = control.snapshot();
        assert_eq!(snapshot.current_item, "starting");
        assert!(snapshot.process_active);
        control.set_process_active(false);

        control.pause();
        let worker = control.clone();
        let handle = std::thread::spawn(move || {
            worker.begin_category("paused-boundary");
            worker.snapshot().current_category
        });
        std::thread::sleep(Duration::from_millis(120));
        assert!(!handle.is_finished());
        control.resume();
        assert_eq!(handle.join().unwrap(), "paused-boundary");
    }
}
