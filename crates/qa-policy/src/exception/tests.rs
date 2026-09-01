use super::*;
use crate::{QaConfig, QaException};
use std::path::PathBuf;

fn finding(rule: &str, path: Option<&str>) -> Finding {
    Finding {
        rule_id: rule.into(),
        severity: Severity::High,
        message: "finding".into(),
        path: path.map(str::to_string),
        line: Some(1),
        detail: None,
    }
}

fn active(rule: &str, path: &str) -> QaException {
    QaException {
        rule: rule.into(),
        path: path.into(),
        reason: "documented reason".into(),
        expires: "2999-12-31".into(),
        limit: None,
    }
}

#[test]
fn path_matching_supports_exact_suffix_global_and_wildcard_patterns() {
    let root = PathBuf::from("C:/repo");
    assert!(path_match(&root, "*", None));
    assert!(path_match(&root, "**/*", Some("anything")));
    assert!(!path_match(&root, "src/lib.rs", None));
    assert!(path_match(&root, "src/lib.rs", Some("C:/repo/src/lib.rs")));
    assert!(path_match(&root, "lib.rs", Some("C:/repo/src/lib.rs")));
    assert!(path_match(&root, "src/*/mod.rs", Some("C:/repo/src/deep/mod.rs")));
    assert!(!path_match(&root, "src/*/mod.rs", Some("C:/repo/tests/mod.rs")));
    assert!(wild("src/one/two/mod.rs", "src/*/mod.rs"));
    assert!(!wild("src/one/two/lib.rs", "src/*/mod.rs"));
}

#[test]
fn active_exception_suppresses_matching_finding_but_not_other_rules_or_paths() {
    let root = Path::new("workspace");
    let mut config = QaConfig::default();
    config.exception.push(active("QA-X-001", "src/*.rs"));
    let result = apply_exceptions(
        root,
        &config,
        vec![
            finding("QA-X-001", Some("workspace/src/lib.rs")),
            finding("QA-X-002", Some("workspace/src/lib.rs")),
            finding("QA-X-001", Some("workspace/tests/lib.rs")),
        ],
    );
    assert_eq!(result.suppressed, 1);
    assert_eq!(result.findings.len(), 2);
}

#[test]
fn governance_rejects_missing_reason_and_expired_or_missing_expiry() {
    let mut config = QaConfig {
        exception: vec![
            QaException {
                rule: "QA-A-001".into(),
                path: "*".into(),
                reason: "".into(),
                expires: "2999-01-01".into(),
                limit: None,
            },
            QaException {
                rule: "QA-B-001".into(),
                path: "*".into(),
                reason: "reason".into(),
                expires: "2000-01-01".into(),
                limit: None,
            },
        ],
        ..QaConfig::default()
    };
    let findings = exception_governance(&config, "2026-08-22");
    assert!(findings.iter().any(|finding| finding.rule_id == "QA-EXC-002"));
    assert!(findings.iter().any(|finding| finding.rule_id == "QA-EXC-001"));

    config.exceptions.require_reason = false;
    config.exceptions.require_expiry = false;
    assert!(exception_governance(&config, "2026-08-22").is_empty());
}

#[test]
fn today_is_an_iso_calendar_date() {
    let value = today();
    assert_eq!(value.len(), 10);
    assert_eq!(&value[4..5], "-");
    assert_eq!(&value[7..8], "-");
    assert!(value[..4].parse::<u32>().unwrap() >= 2020);
}

#[test]
fn suppression_requires_the_exact_rule_active_reason_and_matching_path() {
    let root = Path::new("workspace");
    let mut config = QaConfig::default();
    config.exception.push(active("QA-X-001", "src/*.rs"));
    let today = "2026-08-27";

    assert!(suppressed(root, &config, today, &finding("QA-X-001", Some("workspace/src/lib.rs"))));
    assert!(!suppressed(root, &config, today, &finding("QA-X-002", Some("workspace/src/lib.rs"))));
    assert!(!suppressed(
        root,
        &config,
        today,
        &finding("QA-X-001", Some("workspace/tests/lib.rs")),
    ));

    config.exception[0].reason.clear();
    assert!(!suppressed(root, &config, today, &finding("QA-X-001", Some("workspace/src/lib.rs"))));
    config.exception[0].reason = "reason".into();
    config.exception[0].expires = "2026-08-26".into();
    assert!(!suppressed(root, &config, today, &finding("QA-X-001", Some("workspace/src/lib.rs"))));
}

#[test]
fn wildcard_matching_is_ordered_and_position_sensitive() {
    assert!(wild("src/one/two/mod.rs", "src/*/mod.rs"));
    assert!(wild("prefix-middle-suffix", "prefix*suffix"));
    assert!(!wild("suffix-prefix", "prefix*suffix"));
    assert!(!wild("src/mod.rs/after", "src/*/missing.rs"));
    assert!(wild("abc", "abc"));
    assert!(!wild("ab", "abc"));
}

#[test]
fn unix_day_conversion_matches_epoch_leap_day_and_century_boundaries() {
    for (days, expected) in [
        (0, "1970-01-01"),
        (1, "1970-01-02"),
        (10_956, "1999-12-31"),
        (11_015, "2000-02-28"),
        (11_016, "2000-02-29"),
        (11_017, "2000-03-01"),
        (19_782, "2024-02-29"),
        (47_541, "2100-03-01"),
    ] {
        assert_eq!(iso_date_from_unix_days(days), expected);
    }
}

#[test]
fn non_wildcard_paths_do_not_degenerate_into_match_everything() {
    let root = Path::new("/workspace");
    assert!(path_match(root, "src/lib.rs", Some("/workspace/src/lib.rs")));
    assert!(path_match(root, "lib.rs", Some("/workspace/src/lib.rs")));
    assert!(!path_match(root, "other.rs", Some("/workspace/src/lib.rs")));
    assert!(!path_match(root, "src/lib.rs", Some("/workspace/tests/lib.rs")));
}

#[test]
fn unix_day_conversion_uses_whole_days_not_subday_remainder() {
    assert_eq!(unix_days(0), 0);
    assert_eq!(unix_days(86_399), 0);
    assert_eq!(unix_days(86_400), 1);
    assert_eq!(unix_days(172_799), 1);
    assert_eq!(unix_days(172_800), 2);
}
