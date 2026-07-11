use super::*;
use crossterm::event::KeyModifiers;
use ratatui::backend::TestBackend;
use ratatui::Terminal;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn render_to_string(step: &mut HostnameStep, state: &InstallerState, focused: bool) -> String {
    let backend = TestBackend::new(90, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| step.render(frame, frame.area(), state, focused))
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
fn validation_matches_the_locked_single_label_pattern() {
    for valid in [
        "a",
        "0",
        "clipsneko",
        "ClipsNeko",
        "CN-LiveCD-1",
        "clipsneko-linux",
        "a23456789012345678901234567890123456789012345678901234567890123",
    ] {
        assert!(hostname_is_valid(valid), "expected valid: {valid}");
    }

    for invalid in [
        "",
        "-clipsneko",
        "clipsneko-",
        "clipsneko_linux",
        "clipsneko.local",
        "clips neko",
        "主机",
        "a234567890123456789012345678901234567890123456789012345678901234",
    ] {
        assert!(!hostname_is_valid(invalid), "expected invalid: {invalid}");
    }
}

#[test]
fn enter_and_next_button_commit_only_valid_input() {
    let mut step = HostnameStep::new();
    let mut state = InstallerState::default();

    step.hostname = "invalid.name".to_string();
    assert!(!step.is_complete(&state));
    assert!(matches!(
        step.handle_key(key(KeyCode::Enter), &mut state).unwrap(),
        StepAction::None
    ));
    assert!(matches!(
        step.on_next_button(&mut state).unwrap(),
        StepAction::None
    ));
    assert!(state.hostname.is_none());

    step.hostname = "clipsneko".to_string();
    assert!(step.is_complete(&state));
    assert!(matches!(
        step.handle_key(key(KeyCode::Enter), &mut state).unwrap(),
        StepAction::Next
    ));
    assert_eq!(state.hostname.as_deref(), Some("clipsneko"));
}

#[test]
fn activation_restores_the_committed_hostname() {
    let mut step = HostnameStep::new();
    let mut state = InstallerState {
        hostname: Some("saved-host".to_string()),
        ..InstallerState::default()
    };

    step.activate(&mut state).unwrap();

    assert_eq!(step.hostname, "saved-host");
    assert!(step.is_complete(&state));
}

#[test]
fn editing_and_backspace_update_the_input() {
    let mut step = HostnameStep::new();
    let mut state = InstallerState::default();

    for character in "host-1".chars() {
        step.handle_key(key(KeyCode::Char(character)), &mut state)
            .unwrap();
    }
    assert_eq!(step.hostname, "host-1");

    step.handle_key(key(KeyCode::Backspace), &mut state)
        .unwrap();
    assert_eq!(step.hostname, "host-");
    assert!(!step.is_complete(&state));
}

#[test]
fn english_render_shows_value_rule_and_focus_cursor() {
    crate::i18n::set_language(crate::i18n::UiLang::En).unwrap();
    let mut step = HostnameStep::new();
    let state = InstallerState::default();
    step.hostname = "clipsneko".to_string();

    let output = render_to_string(&mut step, &state, true);

    assert!(output.contains("clipsneko"));
    assert!(output.contains("Hostname is valid"));
    assert!(output.contains("ASCII letters"));
    assert!(output.contains('█'));
}
