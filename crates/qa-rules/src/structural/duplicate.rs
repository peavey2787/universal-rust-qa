use qa_model::{DuplicateGroup, Finding, Severity, SourceSpan};
use qa_policy::QaConfig;
use qa_syntax::WorkspaceSource;
use std::collections::{HashMap, HashSet};
pub fn analyze(
    s: &WorkspaceSource,
    c: &QaConfig,
    f: &mut Vec<Finding>,
) -> (Vec<DuplicateGroup>, usize) {
    let w = c.duplicates.minimum_loc.max(4);
    let mut m: HashMap<u64, Vec<SourceSpan>> = HashMap::new();
    for sf in &s.files {
        let l: Vec<_> = sf.text.lines().map(norm).collect();
        if l.len() < w {
            continue;
        }
        for i in 0..=l.len() - w {
            let ch = l[i..i + w].join("\n");
            if ch.len() < 80 || ch.split_whitespace().count() < c.duplicates.minimum_nodes {
                continue;
            }
            m.entry(hash(ch.as_bytes()))
                .or_default()
                .push(SourceSpan { path: sf.path.display().to_string(), line: i + 1 })
        }
    }
    let mut o = vec![];
    let mut cov = HashSet::new();
    for (h, v) in m {
        let u: HashSet<_> = v.iter().map(|x| (&x.path, x.line)).collect();
        if u.len() < 2 {
            continue;
        }
        for x in &v {
            for n in x.line..x.line + w {
                cov.insert((x.path.clone(), n));
            }
        }
        f.push(Finding {
            rule_id: "QA-DUP-002".into(),
            severity: Severity::Low,
            message: format!("Duplicate structural block with {} occurrences", v.len()),
            path: v.first().map(|x| x.path.clone()),
            line: v.first().map(|x| x.line),
            detail: None,
        });
        o.push(DuplicateGroup {
            fingerprint: format!("{h:016x}"),
            kind: "subtree-exact".into(),
            similarity: 1.0,
            occurrences: v,
            logical_lines: w,
        })
    }
    (o, cov.len())
}
fn norm(l: &str) -> String {
    l.split("//").next().unwrap_or("").split_whitespace().collect::<Vec<_>>().join(" ")
}
fn hash(b: &[u8]) -> u64 {
    let mut h = 14695981039346656037;
    for x in b {
        h ^= *x as u64;
        h = h.wrapping_mul(1099511628211)
    }
    h
}

#[cfg(test)]
mod tests;
