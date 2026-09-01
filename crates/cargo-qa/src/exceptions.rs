use qa_policy::{QaConfig, QaException};
use std::{
    io::{self, Write},
    path::Path,
};

pub fn menu(workspace: &Path) -> Result<(), Box<dyn std::error::Error>> {
    menu_filtered(workspace, None)
}
pub fn menu_filtered(
    workspace: &Path,
    filter: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        let mut cfg = QaConfig::load(workspace)?;
        println!("\nExceptions{}", filter.map(|f| format!(" — {f}")).unwrap_or_default());
        let visible = visible_exception_indices(&cfg, filter);
        if visible.is_empty() {
            println!("  none");
        }
        for (n, idx) in visible.iter().copied().enumerate() {
            let e = &cfg.exception[idx];
            println!("  {}. [{}] {} — {} (expires {})", n + 1, e.rule, e.path, e.reason, e.expires);
            println!("     internal index {}", idx + 1);
        }
        println!("  A Add\n  R Remove\n  B Back");
        match prompt("exceptions> ")?.to_ascii_lowercase().as_str() {
            "a" => {
                let rule = filter.map(str::to_string).unwrap_or(prompt("Rule ID: ")?);
                let path = prompt("Path/glob: ")?;
                let reason = prompt("Reason: ")?;
                let expires = prompt("Expires (YYYY-MM-DD): ")?;
                if !has_required_details(&reason, &expires) {
                    println!("Reason and expiry are required.");
                    continue;
                }
                cfg.exception.push(QaException { rule, path, reason, expires, limit: None });
                cfg.save(&workspace.join("qa.toml"))?;
            }
            "r" => {
                let n = prompt("Visible exception # to remove: ")?.parse::<usize>();
                if let Ok(n) = n {
                    let selected = visible.get(n.saturating_sub(1)).copied();
                    if let Some(idx) = selected {
                        cfg.exception.remove(idx);
                        cfg.save(&workspace.join("qa.toml"))?;
                    }
                }
            }
            "b" | "" => break,
            _ => {}
        }
    }
    Ok(())
}
fn has_required_details(reason: &str, expires: &str) -> bool {
    !reason.trim().is_empty() && !expires.trim().is_empty()
}

fn visible_exception_indices(config: &QaConfig, filter: Option<&str>) -> Vec<usize> {
    config
        .exception
        .iter()
        .enumerate()
        .filter_map(|(index, exception)| {
            filter.is_none_or(|rule| exception.rule == rule).then_some(index)
        })
        .collect()
}

fn prompt(label: &str) -> io::Result<String> {
    print!("{label}");
    io::stdout().flush()?;
    let mut s = String::new();
    io::stdin().read_line(&mut s)?;
    Ok(s.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exception(rule: &str) -> QaException {
        QaException {
            rule: rule.into(),
            path: "src/*.rs".into(),
            reason: "test".into(),
            expires: "2999-01-01".into(),
            limit: None,
        }
    }

    #[test]
    fn visible_exception_indices_preserve_order_and_apply_exact_rule_filter() {
        let config = QaConfig {
            exception: vec![exception("QA-A"), exception("QA-B"), exception("QA-A")],
            ..Default::default()
        };

        assert_eq!(visible_exception_indices(&config, None), vec![0, 1, 2]);
        assert_eq!(visible_exception_indices(&config, Some("QA-A")), vec![0, 2]);
        assert_eq!(visible_exception_indices(&config, Some("QA-B")), vec![1]);
        assert!(visible_exception_indices(&config, Some("QA-C")).is_empty());
    }

    #[test]
    fn exception_details_require_both_reason_and_expiry() {
        assert!(has_required_details("reason", "2999-01-01"));
        assert!(!has_required_details("", "2999-01-01"));
        assert!(!has_required_details("reason", ""));
        assert!(!has_required_details("   ", "   "));
    }
}
