use super::*;
use ratatui::backend::TestBackend;
use ratatui::Terminal;

fn render_to_string(step: &mut LanguageStep, state: &InstallerState) -> String {
    let backend = TestBackend::new(60, 12);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| step.render(f, f.area(), state, true))
        .unwrap();
    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|c| c.symbol())
        .collect()
}

#[test]
fn renders_all_language_labels() {
    set_language(UiLang::En).unwrap();
    let mut step = LanguageStep::new().unwrap();
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
    for label in ["繁體中文", "日本語", "Deutsch", "한국어", "Русский"] {
        assert!(stripped.contains(label), "missing {label} label");
    }
}

#[test]
fn highlight_moves_on_down_without_wedging() {
    // Regression: the old `&self` + clone-state render lost ratatui's
    // offset bookkeeping and could wedge after a few Up/Down presses.
    // This test drives several highlight moves and re-renders each time,
    // asserting no panic and the expected label under the highlight.
    let mut step = LanguageStep::new().unwrap();
    let mut state = InstallerState::default();

    // Initial: highlight on English (index 0).
    assert_eq!(step.highlighted_ui(), UiLang::En);

    // Down → ZhCn.
    step.handle_key(
        crossterm::event::KeyEvent::new(KeyCode::Down, crossterm::event::KeyModifiers::NONE),
        &mut state,
    )
    .unwrap();
    assert_eq!(step.highlighted_ui(), UiLang::ZhCn);
    let _ = render_to_string(&mut step, &state);

    // Up returns to English without relying on the number of supported languages.
    step.handle_key(
        crossterm::event::KeyEvent::new(KeyCode::Up, crossterm::event::KeyModifiers::NONE),
        &mut state,
    )
    .unwrap();
    assert_eq!(step.highlighted_ui(), UiLang::En);
    let _ = render_to_string(&mut step, &state);

    // Up from the first entry wraps to the last; Down wraps back to English.
    step.handle_key(
        crossterm::event::KeyEvent::new(KeyCode::Up, crossterm::event::KeyModifiers::NONE),
        &mut state,
    )
    .unwrap();
    assert_eq!(step.highlighted_ui(), UiLang::Ru);
    let _ = render_to_string(&mut step, &state);
    step.handle_key(
        crossterm::event::KeyEvent::new(KeyCode::Down, crossterm::event::KeyModifiers::NONE),
        &mut state,
    )
    .unwrap();
    assert_eq!(step.highlighted_ui(), UiLang::En);
    let _ = render_to_string(&mut step, &state);
}

#[test]
fn activation_populates_independent_defaults() {
    let mut step = LanguageStep::new().unwrap();
    let mut state = InstallerState::default();

    step.activate(&mut state).unwrap();

    assert_eq!(state.ui_lang, Some(UiLang::En));
    assert_eq!(state.target_locale.as_deref(), Some("en_US.UTF-8"));
}

#[test]
fn tab_cycles_ui_locale_and_footer_boundaries() {
    let mut step = LanguageStep::new().unwrap();
    assert_eq!(step.focus, LanguageFocus::UiLanguage);
    assert!(step.consume_tab(false));
    assert_eq!(step.focus, LanguageFocus::TargetLocale);
    assert!(!step.consume_tab(false));
    assert_eq!(step.focus, LanguageFocus::UiLanguage);
}
