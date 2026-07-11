use super::*;
use crossterm::event::KeyModifiers;
use ratatui::backend::TestBackend;
use ratatui::Terminal;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn fill(step: &mut UserStep, username: &str, password: &str, confirmation: &str) {
    step.username = username.to_string();
    step.password = SecretString::new(password.to_string());
    step.confirm_password = SecretString::new(confirmation.to_string());
}

fn render_to_string(step: &mut UserStep, state: &InstallerState) -> String {
    let backend = TestBackend::new(90, 24);
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
fn username_validation_matches_the_locked_pattern() {
    for valid in ["a", "_", "alice", "user_1", "a-b"] {
        assert!(username_is_valid(valid), "expected valid: {valid}");
    }
    for invalid in ["", "1alice", "Alice", "a.b", "a b", "用户"] {
        assert!(!username_is_valid(invalid), "expected invalid: {invalid}");
    }
}

#[test]
fn empty_password_is_blocked_but_nonempty_weak_password_is_allowed() {
    let mut step = UserStep::new();
    let mut state = InstallerState::default();
    fill(&mut step, "alice", "", "");
    assert!(!step.is_complete(&state));

    fill(&mut step, "alice", "x", "x");
    assert!(step.is_complete(&state));
    assert!(matches!(
        step.on_next_button(&mut state).unwrap(),
        StepAction::Next
    ));
    assert_eq!(state.user.as_ref().unwrap().username, "alice");
    assert_eq!(state.user_password.as_ref().unwrap().expose_secret(), "x");
}

#[test]
fn confirmation_mismatch_blocks_enter_and_next_button() {
    let mut step = UserStep::new();
    let mut state = InstallerState::default();
    fill(&mut step, "alice", "secret", "different");
    step.focus = UserFocus::ConfirmPassword;

    assert!(matches!(
        step.handle_key(key(KeyCode::Enter), &mut state).unwrap(),
        StepAction::None
    ));
    assert!(matches!(
        step.on_next_button(&mut state).unwrap(),
        StepAction::None
    ));
    assert!(state.user_password.is_none());
}

#[test]
fn enter_and_tab_follow_the_form_focus_order() {
    let mut step = UserStep::new();
    let mut state = InstallerState::default();

    step.handle_key(key(KeyCode::Enter), &mut state).unwrap();
    assert_eq!(step.focus, UserFocus::Password);
    step.handle_key(key(KeyCode::Enter), &mut state).unwrap();
    assert_eq!(step.focus, UserFocus::ConfirmPassword);

    assert!(!step.consume_tab(false));
    assert_eq!(step.focus, UserFocus::Username);
    assert!(!step.consume_tab(true));
    assert_eq!(step.focus, UserFocus::ConfirmPassword);
    assert!(step.consume_tab(true));
    assert_eq!(step.focus, UserFocus::Password);
}

#[test]
fn committed_password_moves_to_state_and_restores_on_reentry() {
    let mut step = UserStep::new();
    let mut state = InstallerState::default();
    fill(&mut step, "alice", "secret", "secret");

    assert!(matches!(
        step.on_next_button(&mut state).unwrap(),
        StepAction::Next
    ));
    assert!(step.password.is_empty());
    assert!(step.confirm_password.is_empty());
    assert!(state.user_password.is_some());
    assert!(step.is_complete(&state));

    step.activate(&mut state).unwrap();
    assert!(state.user_password.is_none());
    assert_eq!(step.password.expose_secret(), "secret");
    assert_eq!(step.confirm_password.expose_secret(), "secret");
}

#[test]
fn rendering_masks_password_and_shows_strength() {
    crate::i18n::set_language(crate::i18n::UiLang::En).unwrap();
    let mut step = UserStep::new();
    let state = InstallerState::default();
    fill(&mut step, "alice", "UniqueSecret7!", "UniqueSecret7!");

    let output = render_to_string(&mut step, &state);

    assert!(output.contains("alice"));
    assert!(output.contains("Password strength"));
    assert!(!output.contains("UniqueSecret7!"));
    assert!(output.contains('•'));
}
