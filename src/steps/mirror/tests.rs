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
fn manual_mirrorlist_keeps_only_manual_server() {
    let manual = "Server = https://example.com/archlinux/$repo/os/$arch";
    let rewritten = manual_mirrorlist(SAMPLE, manual);
    let servers: Vec<&str> = rewritten
        .lines()
        .filter(|line| line.starts_with("Server = "))
        .collect();

    assert_eq!(servers, vec![manual]);
    assert!(rewritten.contains("Arch Linux repository mirrorlist"));
    assert!(!rewritten.contains("## China"));
}

#[test]
fn tab_cycle_prepares_correct_internal_endpoint() {
    let mut step = MirrorStep {
        regions: vec!["Worldwide".to_string()],
        raw: SAMPLE.to_string(),
        list_state: ListState::default(),
        selected: String::new(),
        input: String::new(),
        focus: MirrorFocus::List,
        error: ErrorDialog::default(),
        validating: false,
        spinner: 0,
        receiver: None,
        pending_lines: Vec::new(),
        pending_selected: String::new(),
    };

    assert!(step.consume_tab(false));
    assert_eq!(step.focus, MirrorFocus::Input);
    assert!(!step.consume_tab(false));
    assert_eq!(step.focus, MirrorFocus::List);

    assert!(!step.consume_tab(true));
    assert_eq!(step.focus, MirrorFocus::Input);
    assert!(step.consume_tab(true));
    assert_eq!(step.focus, MirrorFocus::List);
}

#[test]
fn long_manual_url_keeps_tail_and_cursor_visible() {
    let mut list_state = ListState::default();
    list_state.select(Some(0));
    let mut step = MirrorStep {
        regions: vec!["Worldwide".to_string()],
        raw: SAMPLE.to_string(),
        list_state,
        selected: String::new(),
        input: "https://example.com/a/very/long/path/to/archlinux/$repo/os/$arch".to_string(),
        focus: MirrorFocus::Input,
        error: ErrorDialog::default(),
        validating: false,
        spinner: 0,
        receiver: None,
        pending_lines: Vec::new(),
        pending_selected: String::new(),
    };
    let state = InstallerState::default();
    let backend = ratatui::backend::TestBackend::new(60, 12);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| step.render(frame, frame.area(), &state, true))
        .unwrap();
    let rendered: String = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect();

    assert!(rendered.contains("$repo/os/$arch█"));
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

// --- background validation / loading-dialog state machine ---

use crossterm::event::KeyModifiers;
use std::sync::mpsc;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

/// A bare step that never touches the real mirrorlist, with a fake worker
/// channel pre-loaded with the given messages installed as the in-flight
/// validation. The sender is returned so tests can keep the channel open
/// (dropping it would look like a dead worker to `tick`).
fn validating_step(
    messages: Vec<Result<bool, String>>,
) -> (MirrorStep, mpsc::Sender<Result<bool, String>>) {
    let (sender, receiver) = mpsc::channel();
    for msg in messages {
        sender.send(msg).unwrap();
    }
    let step = MirrorStep {
        regions: vec!["Worldwide".to_string()],
        raw: String::new(),
        list_state: ListState::default(),
        selected: String::new(),
        input: String::new(),
        focus: MirrorFocus::List,
        error: ErrorDialog::default(),
        validating: true,
        spinner: 0,
        receiver: Some(receiver),
        pending_lines: vec!["Server = https://example.com/$repo/os/$arch".to_string()],
        pending_selected: "Worldwide".to_string(),
    };
    (step, sender)
}

#[test]
fn validating_is_modal_and_swallows_all_keys() {
    let (mut step, _sender) = validating_step(Vec::new());
    let mut state = InstallerState::default();
    assert!(step.has_modal());
    for code in [
        KeyCode::Enter,
        KeyCode::Esc,
        KeyCode::Char('j'),
        KeyCode::Tab,
    ] {
        assert!(matches!(
            step.handle_key(key(code), &mut state).unwrap(),
            StepAction::None
        ));
    }
    // The swallowed keys must not have changed the in-flight validation.
    assert!(step.validating);
    assert!(state.mirror_lines.is_empty());
}

#[test]
fn tick_waits_quietly_while_worker_runs() {
    let (mut step, _sender) = validating_step(Vec::new());
    let mut state = InstallerState::default();
    assert!(matches!(step.tick(&mut state).unwrap(), StepAction::None));
    assert!(step.validating);
    assert_eq!(step.spinner, 1);
}

#[test]
fn tick_success_records_choice_and_advances() {
    let (mut step, _sender) = validating_step(vec![Ok(true)]);
    let mut state = InstallerState::default();
    assert!(matches!(step.tick(&mut state).unwrap(), StepAction::Next));
    assert!(!step.validating);
    assert!(step.receiver.is_none());
    assert_eq!(
        state.mirror_lines,
        vec!["Server = https://example.com/$repo/os/$arch".to_string()]
    );
    assert_eq!(step.selected, "Worldwide");
    assert!(!step.has_modal());
}

#[test]
fn tick_pacman_failure_shows_error_dialog_without_advancing() {
    let (mut step, _sender) = validating_step(vec![Ok(false)]);
    let mut state = InstallerState::default();
    assert!(matches!(step.tick(&mut state).unwrap(), StepAction::None));
    assert!(!step.validating);
    assert!(step.error.visible);
    assert!(step.has_modal());
    assert!(state.mirror_lines.is_empty());
    assert!(step.selected.is_empty());
}

#[test]
fn tick_propagates_fatal_worker_failure() {
    let (mut step, _sender) = validating_step(vec![Err("boom".to_string())]);
    let mut state = InstallerState::default();
    let err = step.tick(&mut state).err().unwrap();
    assert!(err.to_string().contains("boom"));
    assert!(!step.validating);
    assert!(step.receiver.is_none());
}
