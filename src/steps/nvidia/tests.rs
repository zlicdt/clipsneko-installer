use super::*;
use crossterm::event::KeyModifiers;
use ratatui::backend::TestBackend;
use ratatui::Terminal;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn state_with_kernel(kernel: KernelChoice) -> InstallerState {
    InstallerState {
        kernel: Some(kernel),
        ..InstallerState::default()
    }
}

fn render_to_string(step: &mut NvidiaStep, state: &InstallerState) -> String {
    let backend = TestBackend::new(96, 12);
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
fn default_is_nvidia_open_dkms() {
    assert_eq!(NvidiaChoice::default(), NvidiaChoice::NvidiaOpenDkms);
    assert_eq!(DEFAULT_NVIDIA, NvidiaChoice::NvidiaOpenDkms);
    assert_eq!(NvidiaStep::new().highlighted(), DEFAULT_NVIDIA);
}

#[test]
fn package_mapping_covers_every_choice() {
    let mappings = [
        (NvidiaChoice::None, None),
        (NvidiaChoice::NvidiaOpen, Some("nvidia-open")),
        (NvidiaChoice::NvidiaOpenLts, Some("nvidia-open-lts")),
        (NvidiaChoice::NvidiaOpenDkms, Some("nvidia-open-dkms")),
    ];

    for (choice, package) in mappings {
        assert_eq!(choice.package_name(), package);
    }
}

#[test]
fn compatibility_matrix_covers_all_kernels_and_choices() {
    for kernel in KernelChoice::ALL {
        assert!(NvidiaChoice::None.is_compatible_with(kernel));
        assert!(NvidiaChoice::NvidiaOpenDkms.is_compatible_with(kernel));
        assert_eq!(
            NvidiaChoice::NvidiaOpen.is_compatible_with(kernel),
            kernel == KernelChoice::Linux
        );
        assert_eq!(
            NvidiaChoice::NvidiaOpenLts.is_compatible_with(kernel),
            kernel == KernelChoice::LinuxLts
        );
    }
}

#[test]
fn activation_restores_compatible_choice_and_resets_incompatible_choice() {
    let mut step = NvidiaStep::new();
    let mut state = state_with_kernel(KernelChoice::Linux);
    state.nvidia = NvidiaChoice::NvidiaOpen;
    step.activate(&mut state).unwrap();
    assert_eq!(step.highlighted(), NvidiaChoice::NvidiaOpen);
    assert_eq!(state.nvidia, NvidiaChoice::NvidiaOpen);

    state.kernel = Some(KernelChoice::LinuxZen);
    step.activate(&mut state).unwrap();
    assert_eq!(step.highlighted(), DEFAULT_NVIDIA);
    assert_eq!(state.nvidia, DEFAULT_NVIDIA);
}

#[test]
fn navigation_skips_incompatible_choices_in_both_directions() {
    let mut step = NvidiaStep::new();
    let mut state = state_with_kernel(KernelChoice::LinuxZen);
    step.activate(&mut state).unwrap();

    step.handle_key(key(KeyCode::Up), &mut state).unwrap();
    assert_eq!(step.highlighted(), NvidiaChoice::None);
    step.handle_key(key(KeyCode::Down), &mut state).unwrap();
    assert_eq!(step.highlighted(), NvidiaChoice::NvidiaOpenDkms);

    let mut linux_state = state_with_kernel(KernelChoice::Linux);
    step.activate(&mut linux_state).unwrap();
    step.handle_key(key(KeyCode::Up), &mut linux_state).unwrap();
    assert_eq!(step.highlighted(), NvidiaChoice::NvidiaOpen);
}

#[test]
fn space_enter_and_next_button_commit_current_highlight() {
    let mut step = NvidiaStep::new();
    let mut state = state_with_kernel(KernelChoice::LinuxZen);
    step.activate(&mut state).unwrap();

    step.handle_key(key(KeyCode::Up), &mut state).unwrap();
    let action = step
        .handle_key(key(KeyCode::Char(' ')), &mut state)
        .unwrap();
    assert!(matches!(action, StepAction::None));
    assert_eq!(state.nvidia, NvidiaChoice::None);

    step.handle_key(key(KeyCode::Down), &mut state).unwrap();
    let action = step.handle_key(key(KeyCode::Enter), &mut state).unwrap();
    assert!(matches!(action, StepAction::Next));
    assert_eq!(state.nvidia, DEFAULT_NVIDIA);

    step.handle_key(key(KeyCode::Up), &mut state).unwrap();
    let action = step.on_next_button(&mut state).unwrap();
    assert!(matches!(action, StepAction::Next));
    assert_eq!(state.nvidia, NvidiaChoice::None);
}

#[test]
fn render_shows_all_choices_and_marks_incompatible_ones() {
    crate::i18n::set_language(crate::i18n::UiLang::En).unwrap();
    let mut step = NvidiaStep::new();
    let mut state = state_with_kernel(KernelChoice::LinuxZen);
    step.activate(&mut state).unwrap();

    let output = render_to_string(&mut step, &state);

    for text in [
        "No NVIDIA",
        "nvidia-open",
        "nvidia-open-lts",
        "nvidia-open-dkms",
    ] {
        assert!(output.contains(text), "missing {text:?} in {output:?}");
    }
    assert_eq!(
        output.matches("incompatible with selected kernel").count(),
        2
    );
}
