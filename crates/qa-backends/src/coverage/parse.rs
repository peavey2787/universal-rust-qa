use super::CoverageEvidence;
use qa_model::EvidenceStatus;
use serde_json::Value;
use std::{collections::BTreeMap, fs, path::Path};

pub(super) fn parse(path: &Path) -> CoverageEvidence {
    let text = match fs::read_to_string(path) {
        Ok(value) => value,
        Err(error) => return failed(error.to_string()),
    };
    let value: Value = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(error) => return failed(error.to_string()),
    };
    parse_value(path, &value)
}

pub(super) fn parse_value(path: &Path, value: &Value) -> CoverageEvidence {
    let Some(percent) = value.pointer("/data/0/totals/lines/percent").and_then(Value::as_f64) else {
        return failed("coverage JSON omitted data[0].totals.lines.percent".into());
    };
    let mut files = BTreeMap::new();
    for file in value.pointer("/data/0/files").and_then(Value::as_array).into_iter().flatten() {
        let Some(name) = file.get("filename").and_then(Value::as_str) else {
            continue;
        };
        let lines = files.entry(normalize(name)).or_insert_with(BTreeMap::<usize, u64>::new);
        merge_segments(lines, file.get("segments").and_then(Value::as_array));
    }
    CoverageEvidence {
        status: EvidenceStatus::Available,
        percent: Some(percent),
        source: Some(path.display().to_string()),
        files,
        ..CoverageEvidence::default()
    }
}

pub(super) fn retain_package_scope(
    evidence: &mut CoverageEvidence,
    covered_roots: &[String],
    excluded_roots: &[String],
) {
    evidence.files.retain(|path, _| {
        let covered = longest_matching_root(path, covered_roots);
        let excluded = longest_matching_root(path, excluded_roots);
        match (covered, excluded) {
            (Some(covered), Some(excluded)) => covered > excluded,
            (Some(_), None) => true,
            _ => false,
        }
    });
    let total = evidence.files.values().map(BTreeMap::len).sum::<usize>();
    if total == 0 {
        evidence.status = EvidenceStatus::Failed;
        evidence.percent = None;
        evidence.error = Some(
            "merged coverage report contained no executable lines from successfully measured \
             packages"
                .into(),
        );
        return;
    }
    let covered = evidence
        .files
        .values()
        .flat_map(|lines| lines.values())
        .filter(|count| **count > 0)
        .count();
    evidence.percent = Some(100.0 * covered as f64 / total as f64);
}

fn longest_matching_root(path: &str, roots: &[String]) -> Option<usize> {
    roots
        .iter()
        .filter(|root| path_within_root(path, root))
        .map(|root| normalize(root).trim_end_matches('/').len())
        .max()
}

fn path_within_root(path: &str, root: &str) -> bool {
    let path = normalize(path);
    let root = normalize(root).trim_end_matches('/').to_string();
    if cfg!(windows) {
        let path = path.to_ascii_lowercase();
        let root = root.to_ascii_lowercase();
        path == root || path.starts_with(&format!("{root}/"))
    } else {
        path == root || path.starts_with(&format!("{root}/"))
    }
}

fn merge_segments(lines: &mut BTreeMap<usize, u64>, segments: Option<&Vec<Value>>) {
    for segment in segments.into_iter().flatten() {
        let Some(parts) = segment.as_array() else {
            continue;
        };
        let Some(line) = parts.first().and_then(Value::as_u64) else {
            continue;
        };
        let count = parts.get(2).and_then(Value::as_u64).unwrap_or(0);
        let Ok(line) = usize::try_from(line) else {
            continue;
        };
        lines.entry(line).and_modify(|value| *value = (*value).max(count)).or_insert(count);
    }
}

fn failed(error: String) -> CoverageEvidence {
    CoverageEvidence {
        status: EvidenceStatus::Failed,
        error: Some(error),
        ..CoverageEvidence::default()
    }
}

pub(super) fn normalize(path: &str) -> String {
    let path = path.replace('\\', "/");
    if let Some(path) = path.strip_prefix("//?/UNC/") {
        format!("//{path}")
    } else {
        path.strip_prefix("//?/").unwrap_or(&path).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_verbatim_paths_normalize_to_the_same_package_root_shape() {
        assert_eq!(normalize(r"\\?\C:\work\crate\src\lib.rs"), "C:/work/crate/src/lib.rs");
        assert_eq!(
            normalize(r"\\?\UNC\server\share\crate\src\lib.rs"),
            "//server/share/crate/src/lib.rs"
        );
    }

    #[test]
    fn malformed_partial_llvm_output_is_failed_instead_of_becoming_zero_or_complete() {
        let value = serde_json::json!({"data":[{"files":[]}]});
        let evidence = parse_value(Path::new("partial.json"), &value);
        assert_eq!(evidence.status, EvidenceStatus::Failed);
        assert!(
            evidence
                .error
                .as_deref()
                .is_some_and(|error| error.contains("totals.lines.percent"))
        );
    }

    #[test]
    fn duplicate_file_records_merge_lines_without_double_counting_or_overwriting_hits() {
        let value = serde_json::json!({
            "data": [{
                "totals": {"lines": {"percent": 50.0}},
                "files": [
                    {"filename":"src/lib.rs","segments":[[10,1,1],[11,1,0]]},
                    {"filename":"src/lib.rs","segments":[[10,1,0],[11,1,4],[12,1,1]]}
                ]
            }]
        });
        let evidence = parse_value(Path::new("merged.json"), &value);
        let lines = evidence.files.get("src/lib.rs").unwrap();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines.get(&10), Some(&1));
        assert_eq!(lines.get(&11), Some(&4));
        assert_eq!(lines.get(&12), Some(&1));
    }
    #[test]
    fn failed_package_files_are_removed_before_measured_percent_and_crap_use() {
        let value = serde_json::json!({
            "data": [{
                "totals": {"lines": {"percent": 99.0}},
                "files": [
                    {"filename":"/ws/good/src/lib.rs","segments":[[1,1,1],[2,1,0]]},
                    {"filename":"/ws/failed/src/lib.rs","segments":[[1,1,1],[2,1,1]]}
                ]
            }]
        });
        let mut evidence = parse_value(Path::new("merged.json"), &value);
        retain_package_scope(&mut evidence, &["/ws/good".into()], &[]);
        assert_eq!(evidence.status, EvidenceStatus::Available);
        assert_eq!(evidence.percent, Some(50.0));
        assert!(evidence.files.contains_key("/ws/good/src/lib.rs"));
        assert!(!evidence.files.contains_key("/ws/failed/src/lib.rs"));
    }

    #[test]
    fn empty_successful_scope_fails_instead_of_reusing_failed_package_totals() {
        let value = serde_json::json!({
            "data": [{
                "totals": {"lines": {"percent": 100.0}},
                "files": [
                    {"filename":"/ws/failed/src/lib.rs","segments":[[1,1,1]]}
                ]
            }]
        });
        let mut evidence = parse_value(Path::new("merged.json"), &value);
        retain_package_scope(&mut evidence, &["/ws/good".into()], &[]);
        assert_eq!(evidence.status, EvidenceStatus::Failed);
        assert!(evidence.percent.is_none());
    }


    #[test]
    fn nested_failed_package_is_not_credited_to_a_covered_parent_package() {
        let value = serde_json::json!({
            "data": [{
                "totals": {"lines": {"percent": 100.0}},
                "files": [
                    {"filename":"/ws/src/lib.rs","segments":[[1,1,1]]},
                    {"filename":"/ws/crates/failed/src/lib.rs","segments":[[1,1,1]]}
                ]
            }]
        });
        let mut evidence = parse_value(Path::new("merged.json"), &value);
        retain_package_scope(
            &mut evidence,
            &["/ws".into()],
            &["/ws/crates/failed".into()],
        );
        assert!(evidence.files.contains_key("/ws/src/lib.rs"));
        assert!(!evidence.files.contains_key("/ws/crates/failed/src/lib.rs"));
    }

}
