use super::prompt;
use qa_model::{FunctionMetric, QaReport, Severity};
use qa_policy::QaConfig;
use std::path::Path;

type DetailResult = Result<(), Box<dyn std::error::Error>>;
type DetailAction = fn(&Path, &QaReport, &QaConfig) -> DetailResult;
type MetricAction =
    fn(&Path, &QaConfig, &[&FunctionMetric], MetricKind, Option<&str>) -> DetailResult;

const LOC_ACTIONS: [DetailAction; 4] = [loc_top, loc_all, loc_smallest, loc_exceptions];
const METRIC_ACTIONS: [MetricAction; 3] = [metric_top, metric_all, metric_exceptions];

#[derive(Clone, Copy)]
enum MetricKind {
    Cyclomatic,
    Crap,
}

impl MetricKind {
    fn value(self, function: &FunctionMetric) -> f64 {
        match self {
            Self::Cyclomatic => function.cyclomatic as f64,
            Self::Crap => function.crap.unwrap_or(-1.0),
        }
    }
}

pub fn loc_menu(w: &Path, r: &QaReport, c: &QaConfig) -> DetailResult {
    loop {
        println!(
            "\nLOC details\n 1 Top 10 biggest files\n 2 All files: biggest → smallest\n 3 Top 10 smallest files\n 4 Manage LOC exceptions\n B Back"
        );
        let choice = prompt("loc> ")?;
        if let Some(action) = numbered_action(&choice, &LOC_ACTIONS) {
            action(w, r, c)?;
            continue;
        }
        if is_back(&choice) {
            break;
        }
    }
    Ok(())
}

pub fn cc_menu(w: &Path, r: &QaReport, c: &QaConfig) -> DetailResult {
    function_metric_menu(
        w,
        r,
        c,
        "Cyclomatic complexity",
        MetricKind::Cyclomatic,
        Some("QA-METRIC-001"),
    )
}

pub fn crap_menu(w: &Path, r: &QaReport, c: &QaConfig) -> DetailResult {
    function_metric_menu(w, r, c, "CRAP", MetricKind::Crap, Some("QA-METRIC-004"))
}

pub fn tests_menu(w: &Path, r: &QaReport, c: &QaConfig) -> DetailResult {
    let mut rows = r.functions.iter().filter(|f| f.is_test).collect::<Vec<_>>();
    rows.sort_by(|a, b| {
        a.coverage_percent.partial_cmp(&b.coverage_percent).unwrap_or(std::cmp::Ordering::Equal)
    });
    println!("\nTests (lowest known coverage first)");
    choose_function(w, c, &rows)?;
    Ok(())
}

pub fn duplicate_menu(w: &Path, r: &QaReport, c: &QaConfig) -> DetailResult {
    println!("\nDuplicate groups");
    print!("{}", duplicate_groups_text(r));
    let choice = prompt("group # (B back)> ")?;
    if let Some(group_index) = one_based_index(&choice, r.duplicates.len()) {
        let group = &r.duplicates[group_index];
        print!("{}", duplicate_occurrences_text(group));
        let choice = prompt("file #> ")?;
        if let Some(file_index) = one_based_index(&choice, group.occurrences.len()) {
            let span = &group.occurrences[file_index];
            crate::editor::open(&c.viewer, &resolve(w, &span.path), span.line)?;
        }
    }
    Ok(())
}

fn duplicate_groups_text(report: &QaReport) -> String {
    let mut output = String::new();
    for (i, group) in report.duplicates.iter().enumerate() {
        output.push_str(&format!(
            " {:>2}. {} | {:.0}% similar | {} LOC | {} occurrences\n",
            i + 1,
            group.kind,
            group.similarity * 100.0,
            group.logical_lines,
            group.occurrences.len()
        ));
    }
    output
}

fn duplicate_occurrences_text(group: &qa_model::DuplicateGroup) -> String {
    let mut output = String::new();
    for (i, span) in group.occurrences.iter().enumerate() {
        output.push_str(&format!(" {:>2}. {}:{}\n", i + 1, span.path, span.line));
    }
    output
}

pub fn dead_menu(w: &Path, r: &QaReport, c: &QaConfig) -> DetailResult {
    println!("\nDead/unreachable items");
    print!("{}", dead_items_text(r));
    let choice = prompt("item # (B back)> ")?;
    if let Some(index) = one_based_index(&choice, r.dead_items.len()) {
        let item = &r.dead_items[index];
        crate::editor::open(&c.viewer, &resolve(w, &item.path), item.line)?;
    }
    Ok(())
}

fn dead_items_text(report: &QaReport) -> String {
    let mut output = String::new();
    for (i, item) in report.dead_items.iter().enumerate() {
        output.push_str(&format!(
            " {:>2}. [{}] {} — {}:{}\n",
            i + 1,
            item.confidence,
            item.name,
            item.path,
            item.line
        ));
    }
    output
}

pub fn findings_menu(w: &Path, r: &QaReport, c: &QaConfig) -> DetailResult {
    let mut rows = r.findings.iter().collect::<Vec<_>>();
    rows.sort_by_key(|finding| std::cmp::Reverse(rank(finding.severity)));
    println!("\nFindings");
    print!("{}", finding_rows_text(&rows));
    let choice = prompt("finding # (B back)> ")?;
    if let Some(index) = one_based_index(&choice, rows.len()) {
        let finding = rows[index];
        if let Some(path) = &finding.path {
            crate::editor::open(&c.viewer, &resolve(w, path), finding.line.unwrap_or(1))?;
        }
    }
    Ok(())
}

fn finding_rows_text(rows: &[&qa_model::Finding]) -> String {
    let mut output = String::new();
    for (i, finding) in rows.iter().enumerate() {
        output.push_str(&format!(
            " {:>3}. [{:?}] {} — {}\n",
            i + 1,
            finding.severity,
            finding.rule_id,
            finding.message
        ));
    }
    output
}

pub fn evidence_menu(r: &QaReport) -> DetailResult {
    println!("\nEvidence");
    print!("{}", evidence_rows_text(r));
    let _ = prompt("Enter to return> ")?;
    Ok(())
}

fn evidence_rows_text(report: &QaReport) -> String {
    let mut output = String::new();
    for (i, evidence) in report.evidence.iter().enumerate() {
        output.push_str(&format!(
            " {:>3}. [{:?}] {:<6} {:<24} {}\n",
            i + 1,
            evidence.status,
            evidence.family,
            evidence.check,
            evidence.detail.as_deref().unwrap_or("")
        ));
    }
    output
}

fn loc_top(w: &Path, r: &QaReport, c: &QaConfig) -> DetailResult {
    file_list(w, r, c, false, Some(10))
}

fn loc_all(w: &Path, r: &QaReport, c: &QaConfig) -> DetailResult {
    file_list(w, r, c, false, None)
}

fn loc_smallest(w: &Path, r: &QaReport, c: &QaConfig) -> DetailResult {
    file_list(w, r, c, true, Some(10))
}

fn loc_exceptions(w: &Path, _: &QaReport, _: &QaConfig) -> DetailResult {
    crate::exceptions::menu_filtered(w, Some("QA-SPRAWL-001"))
}

fn file_list(
    w: &Path,
    r: &QaReport,
    c: &QaConfig,
    ascending: bool,
    limit: Option<usize>,
) -> DetailResult {
    let rows = file_rows(r, ascending, limit);
    println!("\nFiles by LOC");
    print!("{}", file_rows_text(&rows));
    let choice = prompt("file # (B back)> ")?;
    if let Some(index) = one_based_index(&choice, rows.len()) {
        crate::editor::open(&c.viewer, &resolve(w, &rows[index].path), 1)?;
    }
    Ok(())
}

fn file_rows_text(rows: &[&qa_model::FileMetric]) -> String {
    let mut output = String::new();
    for (i, file) in rows.iter().enumerate() {
        output.push_str(&format!(" {:>3}. {:>5} LOC  {}\n", i + 1, file.logical_loc, file.path));
    }
    output
}

fn file_rows(
    report: &QaReport,
    ascending: bool,
    limit: Option<usize>,
) -> Vec<&qa_model::FileMetric> {
    let mut rows = report.files.iter().collect::<Vec<_>>();
    rows.sort_by_key(|file| file.logical_loc);
    if !ascending {
        rows.reverse();
    }
    if let Some(limit) = limit {
        rows.truncate(limit);
    }
    rows
}

fn metric_rows(report: &QaReport, kind: MetricKind) -> Vec<&FunctionMetric> {
    let mut rows = report.functions.iter().filter(|function| !function.is_test).collect::<Vec<_>>();
    rows.sort_by(|a, b| {
        kind.value(b).partial_cmp(&kind.value(a)).unwrap_or(std::cmp::Ordering::Equal)
    });
    rows
}

fn function_metric_menu(
    w: &Path,
    r: &QaReport,
    c: &QaConfig,
    label: &str,
    kind: MetricKind,
    exception: Option<&str>,
) -> DetailResult {
    let rows = metric_rows(r, kind);
    loop {
        println!("\n{label}\n 1 Top 10\n 2 All high → low\n 3 Manage exceptions\n B Back");
        let choice = prompt("metric> ")?;
        if let Some(action) = numbered_action(&choice, &METRIC_ACTIONS) {
            action(w, c, &rows, kind, exception)?;
            continue;
        }
        if is_back(&choice) {
            break;
        }
    }
    Ok(())
}

fn metric_top(
    w: &Path,
    c: &QaConfig,
    rows: &[&FunctionMetric],
    kind: MetricKind,
    _: Option<&str>,
) -> DetailResult {
    choose_metric(w, c, &rows[..rows.len().min(10)], kind)
}

fn metric_all(
    w: &Path,
    c: &QaConfig,
    rows: &[&FunctionMetric],
    kind: MetricKind,
    _: Option<&str>,
) -> DetailResult {
    choose_metric(w, c, rows, kind)
}

fn metric_exceptions(
    w: &Path,
    _: &QaConfig,
    _: &[&FunctionMetric],
    _: MetricKind,
    exception: Option<&str>,
) -> DetailResult {
    if let Some(rule) = exception {
        crate::exceptions::menu_filtered(w, Some(rule))?;
    }
    Ok(())
}

fn choose_metric(
    w: &Path,
    c: &QaConfig,
    rows: &[&FunctionMetric],
    kind: MetricKind,
) -> DetailResult {
    print!("{}", metric_rows_text(rows, kind));
    let choice = prompt("function #> ")?;
    if let Some(index) = one_based_index(&choice, rows.len()) {
        let function = rows[index];
        crate::editor::open(&c.viewer, &resolve(w, &function.path), function.line)?;
    }
    Ok(())
}

fn metric_rows_text(rows: &[&FunctionMetric], kind: MetricKind) -> String {
    let mut output = String::new();
    for (i, function) in rows.iter().enumerate() {
        output.push_str(&format!(
            " {:>3}. {:>8.2}  {} — {}:{}\n",
            i + 1,
            kind.value(function),
            function.qualified_name,
            function.path,
            function.line
        ));
    }
    output
}

fn choose_function(w: &Path, c: &QaConfig, rows: &[&FunctionMetric]) -> DetailResult {
    print!("{}", function_rows_text(rows));
    let choice = prompt("test #> ")?;
    if let Some(index) = one_based_index(&choice, rows.len()) {
        let function = rows[index];
        crate::editor::open(&c.viewer, &resolve(w, &function.path), function.line)?;
    }
    Ok(())
}

fn function_rows_text(rows: &[&FunctionMetric]) -> String {
    let mut output = String::new();
    for (i, function) in rows.iter().enumerate() {
        output.push_str(&format!(
            " {:>3}. {:>7}  {} — {}:{}\n",
            i + 1,
            coverage_label(function.coverage_percent),
            function.qualified_name,
            function.path,
            function.line
        ));
    }
    output
}

fn coverage_label(value: Option<f64>) -> String {
    match value {
        Some(value) => format!("{value:.1}%"),
        None => "N/A".into(),
    }
}

fn numbered_action<T: Copy>(input: &str, actions: &[T]) -> Option<T> {
    one_based_index(input, actions.len()).map(|index| actions[index])
}

fn one_based_index(input: &str, len: usize) -> Option<usize> {
    input.parse::<usize>().ok()?.checked_sub(1).filter(|index| *index < len)
}

fn is_back(input: &str) -> bool {
    input.is_empty() || input.eq_ignore_ascii_case("b")
}

fn resolve(w: &Path, p: &str) -> std::path::PathBuf {
    let path = Path::new(p);
    if path.is_absolute() { path.to_path_buf() } else { w.join(path) }
}

fn rank(s: Severity) -> u8 {
    match s {
        Severity::Critical => 5,
        Severity::High => 4,
        Severity::Medium => 3,
        Severity::Low => 2,
        Severity::Info => 1,
    }
}

#[cfg(test)]
mod tests;
