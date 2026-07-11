use super::*;
use crossterm::event::KeyModifiers;
use ratatui::backend::TestBackend;
use ratatui::Terminal;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn render_to_string(step: &mut KernelStep, state: &InstallerState) -> String {
    let backend = TestBackend::new(72, 12);
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
fn default_is_linux_zen() {
    assert_eq!(KernelChoice::default(), KernelChoice::LinuxZen);
    assert_eq!(DEFAULT_KERNEL, KernelChoice::LinuxZen);
    assert_eq!(KernelStep::new().highlighted(), KernelChoice::LinuxZen);
}

#[test]
fn package_and_headers_mapping_covers_every_choice() {
    let mappings = [
        (KernelChoice::Linux, "linux", "linux-headers"),
        (KernelChoice::LinuxLts, "linux-lts", "linux-lts-headers"),
        (KernelChoice::LinuxZen, "linux-zen", "linux-zen-headers"),
        (
            KernelChoice::LinuxHardened,
            "linux-hardened",
            "linux-hardened-headers",
        ),
    ];

    for (choice, package, headers) in mappings {
        assert_eq!(choice.package_name(), package);
        assert_eq!(choice.headers_package_name(), headers);
    }
}

#[test]
fn activation_records_default_and_restores_saved_choice() {
    let mut step = KernelStep::new();
    let mut state = InstallerState::default();

    step.activate(&mut state).unwrap();
    assert_eq!(state.kernel, Some(KernelChoice::LinuxZen));

    state.kernel = Some(KernelChoice::LinuxLts);
    step.activate(&mut state).unwrap();
    assert_eq!(step.highlighted(), KernelChoice::LinuxLts);
}

#[test]
fn navigation_wraps_without_committing() {
    let mut step = KernelStep::new();
    let mut state = InstallerState::default();
    step.activate(&mut state).unwrap();

    step.handle_key(key(KeyCode::Down), &mut state).unwrap();
    assert_eq!(step.highlighted(), KernelChoice::LinuxHardened);
    assert_eq!(state.kernel, Some(KernelChoice::LinuxZen));

    step.handle_key(key(KeyCode::Down), &mut state).unwrap();
    assert_eq!(step.highlighted(), KernelChoice::Linux);

    step.handle_key(key(KeyCode::Up), &mut state).unwrap();
    assert_eq!(step.highlighted(), KernelChoice::LinuxHardened);
}

#[test]
fn space_commits_and_enter_commits_then_advances() {
    let mut step = KernelStep::new();
    let mut state = InstallerState::default();
    step.activate(&mut state).unwrap();

    step.handle_key(key(KeyCode::Up), &mut state).unwrap();
    let action = step
        .handle_key(key(KeyCode::Char(' ')), &mut state)
        .unwrap();
    assert!(matches!(action, StepAction::None));
    assert_eq!(state.kernel, Some(KernelChoice::LinuxLts));

    step.handle_key(key(KeyCode::Up), &mut state).unwrap();
    let action = step.handle_key(key(KeyCode::Enter), &mut state).unwrap();
    assert!(matches!(action, StepAction::Next));
    assert_eq!(state.kernel, Some(KernelChoice::Linux));
}

#[test]
fn next_button_commits_current_highlight() {
    let mut step = KernelStep::new();
    let mut state = InstallerState::default();
    step.activate(&mut state).unwrap();
    step.handle_key(key(KeyCode::Down), &mut state).unwrap();

    let action = step.on_next_button(&mut state).unwrap();

    assert!(matches!(action, StepAction::Next));
    assert_eq!(state.kernel, Some(KernelChoice::LinuxHardened));
}

#[test]
fn english_render_explains_all_choices_and_headers() {
    crate::i18n::set_language(crate::i18n::UiLang::En).unwrap();
    let mut step = KernelStep::new();
    let mut state = InstallerState::default();
    step.activate(&mut state).unwrap();

    let output = render_to_string(&mut step, &state);

    for text in ["linux", "linux-lts", "linux-zen", "linux-hardened"] {
        assert!(output.contains(text), "missing {text:?} in {output:?}");
    }
    assert!(output.contains("headers"), "missing headers hint");
}
