use super::*;

#[test]
fn toggle_is_reversible() {
    let mut value = false;
    toggle(&mut value);
    assert!(value);
    toggle(&mut value);
    assert!(!value);
}

#[test]
fn settings_action_table_and_back_navigation_cover_every_menu_entry() {
    for choice in ["1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "a"] {
        assert!(settings_action(choice).is_some(), "missing action {choice}");
    }
    assert!(settings_action("11").is_none());
    assert!(settings_action("x").is_none());
    assert!(is_back(""));
    assert!(is_back("b"));
    assert!(is_back("B"));
    assert!(!is_back("1"));
}
