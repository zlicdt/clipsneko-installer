use super::*;

const SAMPLE: &str = "\
##
## Arch Linux repository mirrorlist
## Generated on 2026-04-06
##

## Worldwide
Server = https://geo.mirror.pkgbuild.com/$repo/os/$arch
Server = https://mirror.rackspace.com/archlinux/$repo/os/$arch

## China
Server = https://mirrors.tuna.tsinghua.edu.cn/archlinux/$repo/os/$arch
Server = https://mirrors.ustc.edu.cn/archlinux/$repo/os/$arch

## Japan
Server = https://ftp.jaist.ac.jp/pub/Linux/ArchLinux/$repo/os/$arch
";

#[test]
fn parse_regions_skips_file_header() {
    let regions = parse_mirrorlist_regions(SAMPLE);
    assert_eq!(regions, vec!["Worldwide", "China", "Japan"]);
}

#[test]
fn parse_regions_empty() {
    assert!(parse_mirrorlist_regions("").is_empty());
}

#[test]
fn parse_regions_only_header() {
    let text = "##\n## Arch Linux repository mirrorlist\n## Generated on 2026-04-06\n";
    assert!(parse_mirrorlist_regions(text).is_empty());
}

#[test]
fn reorder_moves_region_to_top() {
    let reordered = reorder_mirrorlist(SAMPLE, "China");
    let lines: Vec<&str> = reordered.lines().collect();
    // Header stays first.
    assert!(lines
        .iter()
        .any(|l| l.contains("Arch Linux repository mirrorlist")));
    // China block should appear before Worldwide and Japan.
    let china_idx = lines.iter().position(|l| l.trim() == "## China").unwrap();
    let worldwide_idx = lines
        .iter()
        .position(|l| l.trim() == "## Worldwide")
        .unwrap();
    let japan_idx = lines.iter().position(|l| l.trim() == "## Japan").unwrap();
    assert!(
        china_idx < worldwide_idx,
        "China should be before Worldwide"
    );
    assert!(china_idx < japan_idx, "China should be before Japan");
    // China's servers should be right after the China header.
    assert!(lines[china_idx + 1].starts_with("Server = "));
    assert!(lines[china_idx + 1].contains("tuna"));
}

#[test]
fn reorder_preserves_all_servers() {
    let reordered = reorder_mirrorlist(SAMPLE, "Japan");
    let server_count = reordered
        .lines()
        .filter(|l| l.starts_with("Server = "))
        .count();
    assert_eq!(server_count, 5, "all 5 servers should be preserved");
}

#[test]
fn reorder_unknown_region_unchanged_order() {
    let reordered = reorder_mirrorlist(SAMPLE, "Nonexistent");
    let regions = parse_mirrorlist_regions(&reordered);
    // No matching region → nothing moved to top, order preserved.
    assert_eq!(regions, vec!["Worldwide", "China", "Japan"]);
}

#[test]
fn reorder_header_stays_at_top() {
    let reordered = reorder_mirrorlist(SAMPLE, "China");
    let first_lines: Vec<&str> = reordered.lines().take(4).collect();
    assert!(first_lines[1].contains("Arch Linux repository mirrorlist"));
    assert!(first_lines[2].contains("Generated on"));
}

#[test]
fn extract_servers_for_region() {
    let servers = extract_region_servers(SAMPLE, "China");
    assert_eq!(servers.len(), 2);
    assert!(servers[0].contains("tuna"));
    assert!(servers[1].contains("ustc"));
}

#[test]
fn extract_servers_unknown_region_empty() {
    assert!(extract_region_servers(SAMPLE, "Nonexistent").is_empty());
}

#[test]
fn normalize_server_line_with_prefix() {
    let result = normalize_server_line("Server = https://example.com/archlinux/$repo/os/$arch");
    assert_eq!(
        result,
        Some("Server = https://example.com/archlinux/$repo/os/$arch".to_string())
    );
}

#[test]
fn normalize_server_line_without_prefix() {
    let result = normalize_server_line("https://example.com/archlinux/$repo/os/$arch");
    assert_eq!(
        result,
        Some("Server = https://example.com/archlinux/$repo/os/$arch".to_string())
    );
}

#[test]
fn normalize_server_line_no_equals_space() {
    let result = normalize_server_line("Server=https://example.com/$repo/os/$arch");
    assert_eq!(
        result,
        Some("Server = https://example.com/$repo/os/$arch".to_string())
    );
}

#[test]
fn normalize_server_line_empty() {
    assert_eq!(normalize_server_line(""), None);
    assert_eq!(normalize_server_line("   "), None);
}

#[test]
fn normalize_server_line_invalid_scheme() {
    assert_eq!(
        normalize_server_line("ftp://example.com/"),
        Some("Server = ftp://example.com/".to_string())
    );
    assert_eq!(
        normalize_server_line("rsync://example.com/"),
        Some("Server = rsync://example.com/".to_string())
    );
}

#[test]
fn normalize_server_line_bad_scheme() {
    assert_eq!(normalize_server_line("file:///etc/pacman"), None);
    assert_eq!(normalize_server_line("example.com"), None);
    assert_eq!(normalize_server_line("Server = "), None);
}

#[test]
fn split_header_separates_correctly() {
    let (header, body) = split_header(SAMPLE);
    assert!(header.contains("Arch Linux repository mirrorlist"));
    assert!(header.contains("Generated on"));
    assert!(body.starts_with("## Worldwide"));
}
