use super::*;
use crossterm::event::KeyModifiers;
use ratatui::backend::TestBackend;
use ratatui::style::Color;
use ratatui::Terminal;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn render_to_string(step: &mut SystemTypeStep, state: &InstallerState) -> String {
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
fn default_is_hyprland_without_dev_tools() {
    assert_eq!(SystemType::default(), SystemType::Hyprland);
    assert_eq!(DEFAULT_SYSTEM_TYPE, SystemType::Hyprland);
    assert_eq!(SystemTypeStep::new().highlighted(), SystemType::Hyprland);
    assert!(!InstallerState::default().dev_tools);
}

#[test]
fn activation_records_default_and_restores_saved_choice() {
    let mut step = SystemTypeStep::new();
    let mut state = InstallerState::default();

    step.activate(&mut state).unwrap();
    assert_eq!(state.system_type, Some(SystemType::Hyprland));

    state.system_type = Some(SystemType::Kde);
    step.activate(&mut state).unwrap();
    assert_eq!(step.highlighted(), SystemType::Kde);
}

#[test]
fn navigation_wraps_without_committing() {
    let mut step = SystemTypeStep::new();
    let mut state = InstallerState::default();
    step.activate(&mut state).unwrap();

    step.handle_key(key(KeyCode::Down), &mut state).unwrap();
    assert_eq!(step.highlighted(), SystemType::Server);
    assert_eq!(state.system_type, Some(SystemType::Hyprland));

    step.handle_key(key(KeyCode::Down), &mut state).unwrap();
    assert_eq!(step.highlighted(), SystemType::Kde);

    step.handle_key(key(KeyCode::Up), &mut state).unwrap();
    assert_eq!(step.highlighted(), SystemType::Server);
}

#[test]
fn space_commits_and_enter_commits_then_advances() {
    let mut step = SystemTypeStep::new();
    let mut state = InstallerState::default();
    step.activate(&mut state).unwrap();

    step.handle_key(key(KeyCode::Down), &mut state).unwrap();
    let action = step
        .handle_key(key(KeyCode::Char(' ')), &mut state)
        .unwrap();
    assert!(matches!(action, StepAction::None));
    assert_eq!(state.system_type, Some(SystemType::Server));

    step.handle_key(key(KeyCode::Down), &mut state).unwrap();
    let action = step.handle_key(key(KeyCode::Enter), &mut state).unwrap();
    assert!(matches!(action, StepAction::Next));
    assert_eq!(state.system_type, Some(SystemType::Kde));
}

#[test]
fn tab_moves_focus_and_space_toggles_dev_tools() {
    let mut step = SystemTypeStep::new();
    let mut state = InstallerState::default();
    step.activate(&mut state).unwrap();

    assert!(step.consume_tab(false));
    let action = step
        .handle_key(key(KeyCode::Char(' ')), &mut state)
        .unwrap();
    assert!(matches!(action, StepAction::None));
    assert!(state.dev_tools);

    assert!(step.consume_tab(true));
    let action = step
        .handle_key(key(KeyCode::Char(' ')), &mut state)
        .unwrap();
    assert!(matches!(action, StepAction::None));
    assert!(state.dev_tools);
    assert_eq!(state.system_type, Some(SystemType::Hyprland));
}

#[test]
fn tab_bubbles_out_at_the_ends_of_the_focus_chain() {
    let mut step = SystemTypeStep::new();
    assert!(!step.consume_tab(true));
    assert!(step.consume_tab(true));
    assert!(step.consume_tab(false));
    assert!(!step.consume_tab(false));
}

#[test]
fn navigation_keys_are_ignored_while_checkbox_is_focused() {
    let mut step = SystemTypeStep::new();
    let mut state = InstallerState::default();
    step.activate(&mut state).unwrap();
    step.consume_tab(false);

    step.handle_key(key(KeyCode::Down), &mut state).unwrap();
    assert_eq!(step.highlighted(), SystemType::Hyprland);
}

#[test]
fn next_button_commits_current_highlight() {
    let mut step = SystemTypeStep::new();
    let mut state = InstallerState::default();
    step.activate(&mut state).unwrap();
    step.handle_key(key(KeyCode::Down), &mut state).unwrap();

    let action = step.on_next_button(&mut state).unwrap();

    assert!(matches!(action, StepAction::Next));
    assert_eq!(state.system_type, Some(SystemType::Server));
}

#[test]
fn english_render_shows_all_choices_and_checkbox() {
    crate::i18n::set_language(crate::i18n::UiLang::En).unwrap();
    let mut step = SystemTypeStep::new();
    let mut state = InstallerState::default();
    step.activate(&mut state).unwrap();

    let output = render_to_string(&mut step, &state);

    for text in ["Server", "KDE", "Hyprland", "[ ]", "development tools"] {
        assert!(output.contains(text), "missing {text:?} in {output:?}");
    }
}

#[test]
fn selected_profile_and_checked_dev_tools_render_bold_white() {
    crate::i18n::set_language(crate::i18n::UiLang::En).unwrap();
    let backend = TestBackend::new(72, 12);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut step = SystemTypeStep::new();
    let mut state = InstallerState::default();
    step.activate(&mut state).unwrap();
    state.dev_tools = true;

    terminal
        .draw(|frame| step.render(frame, frame.area(), &state, true))
        .unwrap();
    let buffer = terminal.backend().buffer();
    let width = usize::from(buffer.area.width);
    let rows: Vec<&[ratatui::buffer::Cell]> = (0..buffer.area.height)
        .map(|y| &buffer.content()[usize::from(y) * width..(usize::from(y) + 1) * width])
        .collect();
    let row_cell = |y: usize, symbol: &str| {
        rows[y]
            .iter()
            .find(|cell| cell.symbol() == symbol)
            .unwrap_or_else(|| panic!("no {symbol:?} on row {y}"))
    };

    // List block: title row, then "Server", "KDE Plasma", "Hyprland" rows.
    let hyprland = row_cell(3, "H");
    assert!(
        hyprland.modifier.contains(Modifier::BOLD) && hyprland.fg == Color::White,
        "selected Hyprland entry must render bold white"
    );
    let server = row_cell(1, "S");
    assert!(
        !server.modifier.contains(Modifier::BOLD) && server.fg != Color::White,
        "unselected Server entry must not render bold white"
    );

    // Checkbox block: borders at y=8/y=10, text row y=9.
    let checkbox = row_cell(9, "I");
    assert!(
        checkbox.modifier.contains(Modifier::BOLD) && checkbox.fg == Color::White,
        "checked dev-tools label must render bold white"
    );
}
