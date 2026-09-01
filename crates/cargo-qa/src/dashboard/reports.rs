use super::prompt;
use qa_policy::QaConfig;
use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn reports_menu(workspace: &Path, config: &QaConfig) -> Result<(), Box<dyn std::error::Error>> {
    reports_menu_at(&workspace.join(&config.output_dir), config)
}

pub fn reports_menu_at(dir: &Path, config: &QaConfig) -> Result<(), Box<dyn std::error::Error>> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect();
    files.sort();
    loop {
        println!("\nGenerated reports");
        print!("{}", report_files_text(&files));
        println!("  A. Open full report.json\n  B. Back");
        let choice = prompt("reports> ")?;
        if is_back(&choice) {
            break;
        }
        if let Some(path) = report_target(&choice, dir, &files) {
            crate::editor::open(&config.viewer, &path, 1)?;
        }
    }
    Ok(())
}

fn report_files_text(files: &[PathBuf]) -> String {
    let mut output = String::new();
    for (i, path) in files.iter().enumerate() {
        output.push_str(&format!(
            " {:>2}. {}\n",
            i + 1,
            path.file_name().and_then(|name| name.to_str()).unwrap_or("report")
        ));
    }
    output
}

fn report_target(choice: &str, dir: &Path, files: &[PathBuf]) -> Option<PathBuf> {
    if choice.eq_ignore_ascii_case("a") {
        let full = dir.join("report.json");
        return full.is_file().then_some(full);
    }
    one_based_index(choice, files.len()).map(|index| files[index].clone())
}

fn one_based_index(input: &str, len: usize) -> Option<usize> {
    input.parse::<usize>().ok()?.checked_sub(1).filter(|index| *index < len)
}

fn is_back(input: &str) -> bool {
    input.is_empty() || input.eq_ignore_ascii_case("b")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_selection_handles_full_report_indices_and_back_navigation() {
        let root = std::env::temp_dir().join(format!("urqa-report-menu-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let first = root.join("a.json");
        let second = root.join("b.json");
        let full = root.join("report.json");
        fs::write(&first, b"{}").unwrap();
        fs::write(&second, b"{}").unwrap();
        fs::write(&full, b"{}").unwrap();
        let files = vec![first.clone(), second.clone()];

        assert_eq!(report_files_text(&files), "  1. a.json\n  2. b.json\n");
        assert_eq!(report_files_text(&[]), "");
        assert_eq!(report_target("A", &root, &files), Some(full.clone()));
        assert_eq!(report_target("1", &root, &files), Some(first));
        assert_eq!(report_target("2", &root, &files), Some(second));
        assert_eq!(report_target("3", &root, &files), None);
        assert_eq!(report_target("0", &root, &files), None);
        assert_eq!(one_based_index("3", 2), None);
        assert_eq!(report_target("9", &root, &files), None);
        assert_eq!(report_target("x", &root, &files), None);
        assert!(is_back(""));
        assert!(is_back("B"));
        assert!(!is_back("1"));

        fs::remove_file(full).unwrap();
        assert_eq!(report_target("a", &root, &files), None);
        fs::remove_dir_all(root).unwrap();
    }
}
