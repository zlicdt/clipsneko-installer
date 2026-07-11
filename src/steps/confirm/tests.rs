use super::*;
use crate::state::{DiskState, KernelChoice, UserInfo};
use crate::util::password::SecretString;
use crossterm::event::KeyModifiers;
use ratatui::backend::TestBackend;
use ratatui::Terminal;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn complete_state() -> InstallerState {
    InstallerState {
        target_locale: Some("zh_CN.UTF-8".to_string()),
        target_locales: vec!["en_US.UTF-8".to_string(), "zh_CN.UTF-8".to_string()],
        keymap: Some("us".to_string()),
        disk: DiskState {
            esp_partition: Some("nvme0n1p1".to_string()),
            esp_needs_format: Some(false),
            target_partitions: vec!["nvme0n1p2".to_string(), "sda1".to_string()],
            affected_disks: vec!["nvme0n1".to_string(), "sda".to_string()],
            raid_mode: Some(BtrfsRaidMode::Raid1),
        },
        kernel: Some(KernelChoice::LinuxZen),
        nvidia: NvidiaChoice::NvidiaOpenDkms,
        timezone: Some("Asia/Shanghai".to_string()),
        user: Some(UserInfo {
            username: "neko".to_string(),
            password_set: true,
        }),
        user_password: Some(SecretString::new("do-not-render".to_string())),
        hostname: Some("ClipsNeko".to_string()),
        ..InstallerState::default()
    }
}

fn rendered(step: &mut ConfirmStep, state: &InstallerState) -> String {
    let backend = TestBackend::new(96, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| step.render(frame, frame.area(), state, true))
        .unwrap();
    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}

#[test]
fn summary_contains_every_requested_choice_and_no_password() {
    crate::i18n::set_language(crate::i18n::UiLang::En).unwrap();
    let state = complete_state();
    let text: String = summary_lines(&state)
        .iter()
        .map(Line::to_string)
        .collect::<Vec<_>>()
        .join("\n");

    for expected in [
        "en_US.UTF-8",
        "zh_CN.UTF-8",
        "us",
        "linux-zen",
        "nvidia-open-dkms",
        "ClipsNeko",
        "Asia/Shanghai",
        "neko",
        "/dev/nvme0n1",
        "/dev/sda",
        "/dev/nvme0n1p1",
        "/dev/nvme0n1p2",
        "/dev/sda1",
        "RAID1",
    ] {
        assert!(text.contains(expected), "missing {expected:?} in {text:?}");
    }
    assert!(!text.contains("do-not-render"));
    assert!(!text.to_ascii_lowercase().contains("password"));
}

#[test]
fn next_opens_a_cancel_first_dialog_and_install_requires_explicit_focus() {
    let mut step = ConfirmStep::new();
    let mut state = complete_state();

    assert!(matches!(
        step.on_next_button(&mut state).unwrap(),
        StepAction::None
    ));
    assert_eq!(step.dialog_focus, Some(DialogFocus::Cancel));
    assert!(step.has_modal());

    assert!(matches!(
        step.handle_key(key(KeyCode::Enter), &mut state).unwrap(),
        StepAction::None
    ));
    assert!(step.dialog_focus.is_none());

    step.handle_key(key(KeyCode::Enter), &mut state).unwrap();
    step.handle_key(key(KeyCode::Right), &mut state).unwrap();
    assert_eq!(step.dialog_focus, Some(DialogFocus::Install));
    assert!(matches!(
        step.handle_key(key(KeyCode::Enter), &mut state).unwrap(),
        StepAction::Next
    ));
}

#[test]
fn incomplete_summary_cannot_open_the_install_dialog() {
    let mut step = ConfirmStep::new();
    let mut state = complete_state();
    state.disk.affected_disks.clear();

    assert!(!step.is_complete(&state));
    assert!(matches!(
        step.handle_key(key(KeyCode::Enter), &mut state).unwrap(),
        StepAction::None
    ));
    assert!(step.dialog_focus.is_none());
}

#[test]
fn render_shows_summary_and_the_final_warning() {
    crate::i18n::set_language(crate::i18n::UiLang::En).unwrap();
    let mut step = ConfirmStep::new();
    let mut state = complete_state();

    let summary = rendered(&mut step, &state);
    assert!(summary.contains("Installation summary"));
    assert!(summary.contains("Affected disk"));

    step.on_next_button(&mut state).unwrap();
    let dialog = rendered(&mut step, &state);
    assert!(dialog.contains("listed Target partitions"));
    assert!(dialog.contains("Cancel"));
    assert!(dialog.contains("Install"));
}

#[test]
fn long_summary_scrolls_with_navigation_keys() {
    let mut step = ConfirmStep::new();
    let mut state = complete_state();
    state.disk.target_partitions = (1..=12).map(|index| format!("sda{index}")).collect();
    let backend = TestBackend::new(60, 10);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| step.render(frame, frame.area(), &state, true))
        .unwrap();

    assert!(step.max_scroll > 0);
    step.handle_key(key(KeyCode::End), &mut state).unwrap();
    assert_eq!(step.scroll, step.max_scroll);
    step.handle_key(key(KeyCode::Home), &mut state).unwrap();
    assert_eq!(step.scroll, 0);
    step.handle_key(key(KeyCode::PageDown), &mut state).unwrap();
    assert_eq!(step.scroll, 5.min(step.max_scroll));
}
