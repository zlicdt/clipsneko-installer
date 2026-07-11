use super::*;
use crossterm::event::KeyModifiers;
use ratatui::backend::TestBackend;
use ratatui::Terminal;

const TIMEZONES: &str = "\
Africa/Cairo
America/New_York
Antarctica/Casey
Arctic/Longyearbyen
Asia/Shanghai
Asia/Tokyo
Atlantic/Reykjavik
Australia/Sydney
Europe/Berlin
Indian/Maldives
Pacific/Auckland
Etc/UTC
US/Eastern
CET
UTC
";

fn step() -> TimezoneStep {
    TimezoneStep::from_timezone_output(TIMEZONES).unwrap()
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn render_to_string(step: &mut TimezoneStep, state: &InstallerState) -> String {
    let backend = TestBackend::new(96, 16);
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
fn parser_keeps_geographic_groups_and_direct_utc_only() {
    let step = step();
    let names: Vec<_> = step.regions.iter().map(|region| region.name).collect();

    assert_eq!(
        names,
        [
            "Africa",
            "America",
            "Antarctica",
            "Arctic",
            "Asia",
            "Atlantic",
            "Australia",
            "Europe",
            "Indian",
            "Pacific",
            "UTC",
        ]
    );
    assert_eq!(step.regions[4].zones, ["Asia/Shanghai", "Asia/Tokyo"]);
    assert!(step
        .regions
        .iter()
        .flat_map(|region| &region.zones)
        .all(|zone| !zone.starts_with("Etc/") && !zone.starts_with("US/")));
}

#[test]
fn detected_timezone_is_selected_and_invalid_detection_falls_back_to_utc() {
    let mut step = step();
    let mut state = InstallerState::default();

    step.activate_with_detected(&mut state, Some("Asia/Tokyo".to_string()));
    assert_eq!(state.timezone.as_deref(), Some("Asia/Tokyo"));
    assert_eq!(step.region().name, "Asia");
    assert_eq!(step.highlighted_timezone(), "Asia/Tokyo");

    state.timezone = None;
    step.activate_with_detected(&mut state, Some("Etc/UTC".to_string()));
    assert_eq!(state.timezone.as_deref(), Some("UTC"));
    assert!(step.is_utc_region());
}

#[test]
fn activation_restores_a_saved_timezone_without_geoip() {
    let mut step = step();
    let mut state = InstallerState {
        timezone: Some("Europe/Berlin".to_string()),
        ..InstallerState::default()
    };

    step.activate(&mut state).unwrap();

    assert_eq!(step.region().name, "Europe");
    assert_eq!(step.highlighted_timezone(), "Europe/Berlin");
}

#[test]
fn region_then_zone_enter_commits_and_advances() {
    let mut step = step();
    let mut state = InstallerState::default();
    step.activate_with_detected(&mut state, Some("Asia/Shanghai".to_string()));

    let action = step.handle_key(key(KeyCode::Enter), &mut state).unwrap();
    assert!(matches!(action, StepAction::None));
    assert_eq!(step.focus, TimezoneFocus::Zone);

    step.handle_key(key(KeyCode::Down), &mut state).unwrap();
    let action = step.handle_key(key(KeyCode::Enter), &mut state).unwrap();
    assert!(matches!(action, StepAction::Next));
    assert_eq!(state.timezone.as_deref(), Some("Asia/Tokyo"));
}

#[test]
fn arrows_switch_panels_and_zone_navigation_wraps() {
    let mut step = step();
    let mut state = InstallerState::default();
    step.activate_with_detected(&mut state, Some("Asia/Shanghai".to_string()));

    step.handle_key(key(KeyCode::Right), &mut state).unwrap();
    assert_eq!(step.focus, TimezoneFocus::Zone);
    step.handle_key(key(KeyCode::Up), &mut state).unwrap();
    assert_eq!(step.highlighted_timezone(), "Asia/Tokyo");
    step.handle_key(key(KeyCode::Left), &mut state).unwrap();
    assert_eq!(step.focus, TimezoneFocus::Region);
}

#[test]
fn utc_disables_zone_panel_and_enter_advances_directly() {
    let mut step = step();
    let mut state = InstallerState::default();
    step.activate_with_detected(&mut state, None);

    step.handle_key(key(KeyCode::Right), &mut state).unwrap();
    assert_eq!(step.focus, TimezoneFocus::Region);
    assert!(!step.consume_tab(false));

    let action = step.handle_key(key(KeyCode::Enter), &mut state).unwrap();
    assert!(matches!(action, StepAction::Next));
    assert_eq!(state.timezone.as_deref(), Some("UTC"));
}

#[test]
fn tab_cycles_between_both_tables_and_the_footer() {
    let mut step = step();
    step.select_timezone("Asia/Shanghai");

    assert!(step.consume_tab(false));
    assert_eq!(step.focus, TimezoneFocus::Zone);
    assert!(!step.consume_tab(false));
    assert_eq!(step.focus, TimezoneFocus::Region);
    assert!(!step.consume_tab(true));
    assert_eq!(step.focus, TimezoneFocus::Zone);
    assert!(step.consume_tab(true));
    assert_eq!(step.focus, TimezoneFocus::Region);
}

#[test]
fn next_button_commits_the_highlighted_timezone() {
    let mut step = step();
    let mut state = InstallerState::default();
    step.select_timezone("Asia/Tokyo");

    let action = step.on_next_button(&mut state).unwrap();

    assert!(matches!(action, StepAction::Next));
    assert_eq!(state.timezone.as_deref(), Some("Asia/Tokyo"));
}

#[test]
fn english_render_shows_two_tables_and_utc_disabled_message() {
    crate::i18n::set_language(crate::i18n::UiLang::En).unwrap();
    let mut step = step();
    let mut state = InstallerState::default();
    step.activate_with_detected(&mut state, None);

    let output = render_to_string(&mut step, &state);

    for text in ["Region", "Timezone", "Africa", "UTC", "not used"] {
        assert!(output.contains(text), "missing {text:?} in {output:?}");
    }
}
