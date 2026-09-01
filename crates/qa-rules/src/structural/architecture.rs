use qa_model::{Finding, Severity};
use qa_policy::QaConfig;
use qa_syntax::WorkspaceSource;
pub fn analyze(s: &WorkspaceSource, c: &QaConfig, f: &mut Vec<Finding>) {
    for layer in &c.architecture.layer {
        for sf in &s.files {
            if !layer.paths.iter().any(|path| path_matches(&sf.path, path)) {
                continue;
            }
            for other in &c.architecture.layer {
                if other.name == layer.name || layer.may_depend_on.contains(&other.name) {
                    continue;
                }
                if other
                    .paths
                    .iter()
                    .any(|q| sf.text.contains(q.trim_matches('*').trim_matches('/')))
                {
                    f.push(Finding {
                        rule_id: "QA-ARCH-001".into(),
                        severity: Severity::High,
                        message: format!(
                            "Layer `{}` appears to depend on forbidden `{}`",
                            layer.name, other.name
                        ),
                        path: Some(sf.path.display().to_string()),
                        line: Some(1),
                        detail: None,
                    })
                }
            }
        }
    }
}

fn path_matches(path: &std::path::Path, configured: &str) -> bool {
    let native = path.to_string_lossy().replace('\\', "/");
    let configured = configured.replace('\\', "/");
    native.contains(configured.as_str())
}

#[cfg(test)]
mod tests;
