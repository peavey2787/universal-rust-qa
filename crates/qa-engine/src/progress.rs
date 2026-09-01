use qa_model::SummaryMetrics;
use std::{
    sync::{Arc, Mutex},
    time::Instant,
};

#[derive(Clone)]
pub struct RunControl {
    backend: qa_backends::control::RunControl,
    state: Arc<Mutex<ProgressState>>,
}

struct ProgressState {
    running: bool,
    completed: usize,
    total: usize,
    category: String,
    summary: Option<SummaryMetrics>,
    finding_count: usize,
    evidence_count: usize,
    started: Instant,
}

#[derive(Debug, Clone)]
pub struct ProgressSnapshot {
    pub running: bool,
    pub paused: bool,
    pub completed: usize,
    pub total: usize,
    pub category: String,
    pub item: String,
    pub process_active: bool,
    pub skip_category_pending: bool,
    pub summary: Option<SummaryMetrics>,
    pub finding_count: usize,
    pub evidence_count: usize,
    pub elapsed_seconds: u64,
}

impl RunControl {
    pub fn new(total: usize) -> Self {
        Self {
            backend: qa_backends::control::RunControl::default(),
            state: Arc::new(Mutex::new(ProgressState {
                running: true,
                completed: 0,
                total,
                category: "starting".into(),
                summary: None,
                finding_count: 0,
                evidence_count: 0,
                started: Instant::now(),
            })),
        }
    }

    pub fn snapshot(&self) -> ProgressSnapshot {
        let backend = self.backend.snapshot();
        let state = self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        ProgressSnapshot {
            running: state.running,
            paused: backend.paused,
            completed: state.completed,
            total: state.total,
            category: state.category.clone(),
            item: backend.current_item,
            process_active: backend.process_active,
            skip_category_pending: backend.skip_category,
            summary: state.summary.clone(),
            finding_count: state.finding_count,
            evidence_count: state.evidence_count,
            elapsed_seconds: state.started.elapsed().as_secs(),
        }
    }

    pub fn pause(&self) {
        self.backend.pause();
    }

    pub fn resume(&self) {
        self.backend.resume();
    }

    pub fn skip_current(&self) -> bool {
        if !self.backend.snapshot().process_active {
            return false;
        }
        self.backend.skip_current();
        true
    }

    pub fn skip_category(&self) {
        self.backend.skip_category();
    }

    pub(crate) fn category<T>(&self, name: &str, operation: impl FnOnce() -> T) -> T {
        self.begin_category(name);
        let result = qa_backends::control::with_control(&self.backend, operation);
        self.finish_category();
        result
    }

    pub(crate) fn update_summary(
        &self,
        summary: SummaryMetrics,
        finding_count: usize,
        evidence_count: usize,
    ) {
        let mut state = self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.summary = Some(summary);
        state.finding_count = finding_count;
        state.evidence_count = evidence_count;
    }

    pub(crate) fn finish(&self) {
        let mut state = self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.running = false;
        state.completed = state.total;
        state.category = "complete".into();
    }

    fn begin_category(&self, name: &str) {
        {
            let mut state = self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            name.clone_into(&mut state.category);
        }
        self.backend.begin_category(name);
    }

    fn finish_category(&self) {
        self.backend.finish_category();
        let mut state = self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.completed = (state.completed + 1).min(state.total);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_tracks_categories_pause_and_skip_without_faking_completion() {
        let control = RunControl::new(2);
        let initial = control.snapshot();
        assert!(initial.running);
        assert_eq!(initial.completed, 0);
        assert_eq!(initial.total, 2);
        assert_eq!(initial.category, "starting");
        assert_eq!(initial.item, "");
        assert!(!initial.paused);
        assert!(!initial.process_active);
        assert!(!initial.skip_category_pending);
        assert!(initial.summary.is_none());
        assert_eq!(initial.finding_count, 0);
        assert_eq!(initial.evidence_count, 0);

        control.category("inventory", || {});
        let snapshot = control.snapshot();
        assert_eq!(snapshot.completed, 1);
        assert_eq!(snapshot.total, 2);
        assert_eq!(snapshot.category, "inventory");
        assert_eq!(snapshot.item, "complete");
        control.pause();
        assert!(control.snapshot().paused);
        control.resume();
        assert!(!control.snapshot().paused);
        assert!(!control.skip_current());
        control.skip_category();
        assert!(control.snapshot().skip_category_pending);

        let summary = SummaryMetrics {
            health_score: 87.5,
            health_is_provisional: false,
            average_file_loc: 0.0,
            files_over_loc: 0,
            average_cc: 0.0,
            functions_over_cc: 0,
            average_crap: None,
            functions_over_crap: None,
            total_tests: 0,
            invalid_tests: 0,
            coverage: qa_model::CoverageSummary {
                percent: None,
                functions_below_threshold: None,
                source: None,
                status: qa_model::EvidenceStatus::Unknown,
            },
            mutation: qa_model::MutationSummary {
                status: qa_model::EvidenceStatus::Unknown,
                caught: 0,
                missed: 0,
                timeout: 0,
                unviable: 0,
                score_percent: None,
                source: None,
            },
            fuzz: qa_model::FuzzSummary {
                target_count: 0,
                critical_targets_missing: 0,
                regression_artifacts: 0,
                unpersisted_crashes: 0,
                property_test_count: 0,
                status: qa_model::EvidenceStatus::Unknown,
            },
            duplicate_percent: 0.0,
            dead_code_percent: 0.0,
            high_findings: 0,
            critical_findings: 0,
        };
        control.update_summary(summary, 7, 11);
        let updated = control.snapshot();
        assert_eq!(updated.summary.unwrap().health_score, 87.5);
        assert_eq!(updated.finding_count, 7);
        assert_eq!(updated.evidence_count, 11);

        control.finish();
        let finished = control.snapshot();
        assert!(!finished.running);
        assert_eq!(finished.completed, 2);
        assert_eq!(finished.category, "complete");
    }

    #[test]
    fn skip_current_reports_success_only_while_a_real_process_is_active() {
        let control = RunControl::new(1);
        let worker = control.clone();
        let handle = std::thread::spawn(move || {
            worker.category("process", || {
                #[cfg(windows)]
                let command = "ping -n 6 127.0.0.1 >NUL";
                #[cfg(not(windows))]
                let command = "sleep 5";
                qa_backends::process::run_shell(
                    std::path::Path::new(env!("CARGO_MANIFEST_DIR")),
                    command,
                    &[],
                )
            })
        });
        for _ in 0..50 {
            if control.snapshot().process_active {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(control.snapshot().process_active);
        assert!(control.skip_current());
        let _ = handle.join().unwrap();
        assert!(!control.skip_current());
    }

    #[test]
    fn category_completion_is_clamped_to_the_declared_total() {
        let control = RunControl::new(1);
        control.category("one", || {});
        control.category("two", || {});
        assert_eq!(control.snapshot().completed, 1);

        let zero = RunControl::new(0);
        zero.category("none", || {});
        assert_eq!(zero.snapshot().completed, 0);
        zero.finish();
        assert_eq!(zero.snapshot().completed, 0);
    }
}
