use qa_model::Finding;
use qa_syntax::WorkspaceSource;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(1);

pub fn workspace(files: &[(&str, &str)]) -> PathBuf {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("urqa-rule-unit-{}-{id}", std::process::id()));
    if root.exists() {
        fs::remove_dir_all(&root).unwrap();
    }
    fs::create_dir_all(&root).unwrap();
    for (name, text) in files {
        let path = root.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, text).unwrap();
    }
    root
}

pub fn discover(files: &[(&str, &str)]) -> (PathBuf, WorkspaceSource) {
    let root = workspace(files);
    let source = qa_syntax::discover(&root);
    (root, source)
}

pub fn ids(findings: &[Finding]) -> Vec<&str> {
    findings.iter().map(|finding| finding.rule_id.as_str()).collect()
}

pub fn cleanup(root: &Path) {
    fs::remove_dir_all(root).unwrap();
}
