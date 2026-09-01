pub use qa_engine::{ProgressSnapshot, RUN_CATEGORY_COUNT, RunControl, RunOptions};
use qa_model::QaReport;
use qa_policy::{ConfigError, QaConfig};
use std::{
    io,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QaRunLayout {
    pub state_dir: PathBuf,
    pub artifact_root: PathBuf,
    pub reports_dir: PathBuf,
    pub coverage_dir: PathBuf,
    pub mutation_dir: PathBuf,
    pub cargo_target_dir: Option<PathBuf>,
}

impl QaRunLayout {
    pub fn local(workspace: &Path, config: &QaConfig) -> Self {
        let artifact_root = workspace.join(&config.output_dir);
        Self {
            state_dir: workspace.to_path_buf(),
            reports_dir: artifact_root.clone(),
            coverage_dir: artifact_root.clone(),
            mutation_dir: workspace.join("mutants.out"),
            artifact_root,
            cargo_target_dir: None,
        }
    }

    fn engine_paths(&self) -> qa_engine::RunPaths {
        qa_engine::RunPaths {
            artifact_root: self.artifact_root.clone(),
            coverage_dir: self.coverage_dir.clone(),
            mutation_dir: self.mutation_dir.clone(),
            cargo_target_dir: self.cargo_target_dir.clone(),
        }
    }
}

pub struct QaRun {
    pub config: QaConfig,
    pub report: QaReport,
    pub output_dir: PathBuf,
    pub layout: QaRunLayout,
}

pub fn run_workspace(w: &Path) -> Result<QaRun, QaSdkError> {
    run_workspace_with_options(w, &RunOptions::default())
}

pub fn run_workspace_with_options(w: &Path, o: &RunOptions) -> Result<QaRun, QaSdkError> {
    let c = QaConfig::load(w)?;
    let layout = QaRunLayout::local(w, &c);
    run_with_config_and_layout(w, c, o, &layout, None)
}

pub fn run_workspace_with_options_and_layout(
    w: &Path,
    o: &RunOptions,
    layout: &QaRunLayout,
) -> Result<QaRun, QaSdkError> {
    let c = QaConfig::load(w)?;
    run_with_config_and_layout(w, c, o, layout, None)
}

pub fn run_workspace_with_progress(
    w: &Path,
    o: &RunOptions,
    control: &RunControl,
) -> Result<QaRun, QaSdkError> {
    let c = QaConfig::load(w)?;
    let layout = QaRunLayout::local(w, &c);
    run_with_config_and_layout(w, c, o, &layout, Some(control))
}

pub fn run_workspace_with_progress_and_layout(
    w: &Path,
    o: &RunOptions,
    layout: &QaRunLayout,
    control: &RunControl,
) -> Result<QaRun, QaSdkError> {
    let c = QaConfig::load(w)?;
    run_with_config_and_layout(w, c, o, layout, Some(control))
}

fn run_with_config_and_layout(
    workspace: &Path,
    config: QaConfig,
    options: &RunOptions,
    layout: &QaRunLayout,
    control: Option<&RunControl>,
) -> Result<QaRun, QaSdkError> {
    create_layout_dirs(layout)?;
    let paths = layout.engine_paths();
    let report = match control {
        Some(control) => {
            qa_engine::run_with_progress_and_paths(workspace, &config, options, &paths, control)
        }
        None => qa_engine::run_with_options_and_paths(workspace, &config, options, &paths),
    };
    let output_dir = qa_report::write_reports_to(&layout.reports_dir, &config, &report)?;
    Ok(QaRun { config, report, output_dir, layout: layout.clone() })
}

fn create_layout_dirs(layout: &QaRunLayout) -> io::Result<()> {
    for path in
        [&layout.state_dir, &layout.artifact_root, &layout.reports_dir, &layout.coverage_dir]
    {
        std::fs::create_dir_all(path)?;
    }
    if let Some(target) = &layout.cargo_target_dir {
        std::fs::create_dir_all(target)?;
    }
    Ok(())
}

#[derive(Debug)]
pub enum QaSdkError {
    Config(ConfigError),
    Io(io::Error),
}

impl From<ConfigError> for QaSdkError {
    fn from(v: ConfigError) -> Self {
        Self::Config(v)
    }
}

impl From<io::Error> for QaSdkError {
    fn from(v: io::Error) -> Self {
        Self::Io(v)
    }
}

impl std::fmt::Display for QaSdkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Config(e) => write!(f, "{e}"),
            Self::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for QaSdkError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Config(error) => Some(error),
            Self::Io(error) => Some(error),
        }
    }
}
