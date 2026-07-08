use super::*;

#[test]
fn parse_hostname_i_single_ip() {
    assert_eq!(parse_hostname_i("192.168.1.100\n"), vec!["192.168.1.100"]);
}

#[test]
fn parse_hostname_i_multiple_ips() {
    assert_eq!(
        parse_hostname_i("192.168.1.100 10.0.0.1\n"),
        vec!["192.168.1.100", "10.0.0.1"]
    );
}

#[test]
fn parse_hostname_i_empty() {
    assert!(parse_hostname_i("").is_empty());
    assert!(parse_hostname_i("\n").is_empty());
    assert!(parse_hostname_i("   \n").is_empty());
}

#[test]
fn parse_hostname_i_trims_whitespace() {
    assert_eq!(
        parse_hostname_i("  192.168.1.100   10.0.0.1  \n"),
        vec!["192.168.1.100", "10.0.0.1"]
    );
}

#[test]
fn parse_default_route_normal() {
    let input = "default via 192.168.1.1 dev eth0 proto dhcp metric 100\n";
    assert_eq!(
        parse_default_route(input),
        Some(("192.168.1.1".to_string(), "eth0".to_string()))
    );
}

#[test]
fn parse_default_route_no_route() {
    assert_eq!(parse_default_route(""), None);
    assert_eq!(parse_default_route("\n"), None);
}

#[test]
fn parse_default_route_missing_dev() {
    assert_eq!(parse_default_route("default via 192.168.1.1\n"), None);
}

#[test]
fn parse_default_route_missing_via() {
    assert_eq!(parse_default_route("default dev eth0\n"), None);
}

#[test]
fn parse_default_route_first_line_only() {
    let input = "default via 192.168.1.1 dev eth0\ndefault via 10.0.0.1 dev wlan0\n";
    assert_eq!(
        parse_default_route(input),
        Some(("192.168.1.1".to_string(), "eth0".to_string()))
    );
}

#[test]
fn parse_default_route_extra_fields() {
    let input = "default via fe80::1 dev eth0 proto static metric 100 pref medium\n";
    assert_eq!(
        parse_default_route(input),
        Some(("fe80::1".to_string(), "eth0".to_string()))
    );
}
