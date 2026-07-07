use super::*;
use ratatui::backend::TestBackend;
use ratatui::Terminal;

fn render_to_string(step: &mut LanguageStep, state: &InstallerState) -> String {
    let backend = TestBackend::new(60, 12);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| step.render(f, f.area(), state)).unwrap();
    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|c| c.symbol())
        .collect()
}

#[test]
fn renders_both_language_labels() {
    let mut step = LanguageStep::new();
    let state = InstallerState::default();
    let s = render_to_string(&mut step, &state);
    // CJK characters occupy two cells; the continuation cell's symbol is
    // a space, so strip spaces before checking the CJK label.
    let stripped = s.replace(' ', "");
    assert!(stripped.contains("English"), "missing English label");
    assert!(
        stripped.contains("简体中文"),
        "missing 简体中文 label; buffer was:\n{s:?}"
    );
}

#[test]
fn highlight_moves_on_down_without_wedging() {
    // Regression: the old `&self` + clone-state render lost ratatui's
    // offset bookkeeping and could wedge after a few Up/Down presses.
    // This test drives several highlight moves and re-renders each time,
    // asserting no panic and the expected label under the highlight.
    let mut step = LanguageStep::new();
    let mut state = InstallerState::default();

    // Initial: highlight on English (index 0).
    assert_eq!(step.highlighted(), UiLang::En);

    // Down → ZhCn.
    step.handle_key(
        crossterm::event::KeyEvent::new(KeyCode::Down, crossterm::event::KeyModifiers::NONE),
        &mut state,
    );
    assert_eq!(step.highlighted(), UiLang::ZhCn);
    let _ = render_to_string(&mut step, &state);

    // Down again → ZhTw.
    step.handle_key(
        crossterm::event::KeyEvent::new(KeyCode::Down, crossterm::event::KeyModifiers::NONE),
        &mut state,
    );
    assert_eq!(step.highlighted(), UiLang::ZhTw);
    let _ = render_to_string(&mut step, &state);

    // One more Down wraps back to En.
    step.handle_key(
        crossterm::event::KeyEvent::new(KeyCode::Down, crossterm::event::KeyModifiers::NONE),
        &mut state,
    );
    assert_eq!(step.highlighted(), UiLang::En);
    let _ = render_to_string(&mut step, &state);

    // Up wraps to ZhTw.
    step.handle_key(
        crossterm::event::KeyEvent::new(KeyCode::Up, crossterm::event::KeyModifiers::NONE),
        &mut state,
    );
    assert_eq!(step.highlighted(), UiLang::ZhTw);
    let _ = render_to_string(&mut step, &state);
}
