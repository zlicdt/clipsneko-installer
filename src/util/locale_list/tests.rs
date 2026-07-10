use super::*;

#[test]
fn parses_enabled_and_commented_utf8_entries() {
    let fixture = r#"
# Configuration file for locale-gen
#en_US.UTF-8 UTF-8
zh_CN.UTF-8 UTF-8
#de_DE ISO-8859-1
# de_DE.UTF-8 UTF-8
"#;
    assert_eq!(
        parse_utf8_locales(fixture),
        vec!["en_US.UTF-8", "zh_CN.UTF-8", "de_DE.UTF-8"]
    );
}

#[test]
fn ignores_headers_invalid_lines_and_duplicates() {
    let fixture = r#"
# Each line is of the form:
# <locale> <charset>
#en_US.UTF-8 UTF-8
en_US.UTF-8 UTF-8

"#;
    assert_eq!(parse_utf8_locales(fixture), vec!["en_US.UTF-8"]);
}
