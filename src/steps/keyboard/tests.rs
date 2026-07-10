use super::*;

#[test]
fn parse_keymap_list_handles_lines() {
    let input = "us\nuk\nde-latin1\n\n  \nru\n";
    let out = parse_keymap_list(input);
    assert_eq!(out, vec!["us", "uk", "de-latin1", "ru"]);
}

#[test]
fn parse_keymap_list_empty() {
    assert!(parse_keymap_list("").is_empty());
    assert!(parse_keymap_list("\n\n").is_empty());
}

#[test]
fn parse_keymap_list_trims_whitespace() {
    let out = parse_keymap_list("  us  \n\tcolemak\t\n");
    assert_eq!(out, vec!["us", "colemak"]);
}

#[test]
fn parse_current_keymap_extracts_vc_keymap() {
    let input = "   VC Keymap: us\n   X11 KEYMAP: us\n   Layout: us\n";
    assert_eq!(parse_current_keymap(input).as_deref(), Some("us"));
}

#[test]
fn parse_current_keymap_missing_returns_none() {
    let input = "   X11 KEYMAP: us\n";
    assert_eq!(parse_current_keymap(input), None);
}

#[test]
fn parse_current_keymap_na_returns_none() {
    let input = "   VC Keymap: n/a\n";
    assert_eq!(parse_current_keymap(input), None);
}

#[test]
fn parse_current_keymap_na_case_insensitive() {
    let input = "   VC Keymap: N/A\n";
    assert_eq!(parse_current_keymap(input), None);
}

#[test]
fn parse_current_keymap_empty_returns_none() {
    assert_eq!(parse_current_keymap(""), None);
}

#[test]
fn parse_current_keymap_trims_value() {
    let input = "   VC Keymap:   de-latin1  \n";
    assert_eq!(parse_current_keymap(input).as_deref(), Some("de-latin1"));
}

#[test]
fn parse_current_keymap_does_not_match_x11_line() {
    // "X11 KEYMAP:" must not be confused with "VC Keymap:".
    let input = "  X11 KEYMAP: us\n";
    assert_eq!(parse_current_keymap(input), None);
}
