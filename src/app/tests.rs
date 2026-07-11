use super::*;
use crate::steps::StepId;
use crossterm::event::{KeyEvent, KeyModifiers};

struct TestStep {
    modal: bool,
    commit_on_next: bool,
}

struct LockedStep;

impl Step for LockedStep {
    fn id(&self) -> StepId {
        StepId::Install
    }

    fn render(
        &mut self,
        _frame: &mut Frame,
        _area: Rect,
        _state: &InstallerState,
        _body_focused: bool,
    ) {
    }

    fn handle_key(
        &mut self,
        _key: crossterm::event::KeyEvent,
        _state: &mut InstallerState,
    ) -> anyhow::Result<StepAction> {
        Ok(StepAction::None)
    }

    fn allows_back(&self) -> bool {
        false
    }

    fn blocks_global_quit(&self) -> bool {
        true
    }

    fn on_back_button(&mut self, _state: &mut InstallerState) -> anyhow::Result<StepAction> {
        Ok(StepAction::None)
    }
}

impl Step for TestStep {
    fn id(&self) -> StepId {
        StepId::Language
    }

    fn render(
        &mut self,
        _frame: &mut Frame,
        _area: Rect,
        _state: &InstallerState,
        _body_focused: bool,
    ) {
    }

    fn handle_key(
        &mut self,
        _key: crossterm::event::KeyEvent,
        _state: &mut InstallerState,
    ) -> anyhow::Result<StepAction> {
        Ok(StepAction::None)
    }

    fn has_modal(&self) -> bool {
        self.modal
    }

    fn on_next_button(&mut self, state: &mut InstallerState) -> anyhow::Result<StepAction> {
        if self.commit_on_next {
            state.hostname = Some("committed".to_string());
        }
        Ok(StepAction::Next)
    }
}

fn app_with_first_step(step: TestStep) -> App {
    App {
        steps: vec![
            Box::new(step),
            Box::new(TestStep {
                modal: false,
                commit_on_next: false,
            }),
        ],
        current: 0,
        state: InstallerState::default(),
        quit_confirm: None,
        focus: Focus::StepBody,
    }
}

fn key(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn modified_key(code: KeyCode, modifiers: KeyModifiers) -> Event {
    Event::Key(KeyEvent::new(code, modifiers))
}

#[test]
fn modal_receives_escape_before_global_quit() {
    let mut app = app_with_first_step(TestStep {
        modal: true,
        commit_on_next: false,
    });

    let _ = app.handle_event(key(KeyCode::Esc)).unwrap();

    assert!(app.quit_confirm.is_none());
    assert_eq!(app.current, 0);
    assert_eq!(app.focus, Focus::StepBody);
}

#[test]
fn modal_prevents_tab_from_reaching_footer() {
    let mut app = app_with_first_step(TestStep {
        modal: true,
        commit_on_next: false,
    });

    let _ = app.handle_event(key(KeyCode::Tab)).unwrap();

    assert_eq!(app.focus, Focus::StepBody);
    assert_eq!(app.current, 0);
}

#[test]
fn next_button_commits_through_step_hook_before_advancing() {
    let mut app = app_with_first_step(TestStep {
        modal: false,
        commit_on_next: true,
    });
    app.focus = Focus::NextButton;

    let _ = app.handle_event(key(KeyCode::Enter)).unwrap();

    assert_eq!(app.state.hostname.as_deref(), Some("committed"));
    assert_eq!(app.current, 1);
    assert_eq!(app.focus, Focus::StepBody);
}

#[test]
fn escape_uses_back_path_outside_modals() {
    let mut app = app_with_first_step(TestStep {
        modal: false,
        commit_on_next: false,
    });
    app.current = 1;

    let _ = app.handle_event(key(KeyCode::Esc)).unwrap();

    assert_eq!(app.current, 0);
    assert!(app.quit_confirm.is_none());
}

#[test]
fn quit_confirmation_defaults_to_cancel() {
    let mut app = app_with_first_step(TestStep {
        modal: false,
        commit_on_next: false,
    });

    let _ = app
        .handle_event(modified_key(KeyCode::Char('c'), KeyModifiers::CONTROL))
        .unwrap();
    assert_eq!(app.quit_confirm, Some(QuitFocus::Cancel));

    let action = app.handle_event(key(KeyCode::Enter)).unwrap();
    assert!(matches!(action, Action::Continue));
    assert!(app.quit_confirm.is_none());
}

#[test]
fn quit_requires_selecting_quit_button() {
    let mut app = app_with_first_step(TestStep {
        modal: false,
        commit_on_next: false,
    });
    app.quit_confirm = Some(QuitFocus::Cancel);

    let _ = app.handle_event(key(KeyCode::Right)).unwrap();
    let action = app.handle_event(key(KeyCode::Enter)).unwrap();

    assert!(matches!(action, Action::Quit));
}

#[test]
fn destructive_step_disables_back_and_global_ctrl_c() {
    let mut app = App {
        steps: vec![
            Box::new(TestStep {
                modal: false,
                commit_on_next: false,
            }),
            Box::new(LockedStep),
        ],
        current: 1,
        state: InstallerState::default(),
        quit_confirm: None,
        focus: Focus::StepBody,
    };

    assert!(!app.back_enabled());
    let action = app
        .handle_event(modified_key(KeyCode::Char('c'), KeyModifiers::CONTROL))
        .unwrap();
    assert!(matches!(action, Action::Continue));
    assert!(app.quit_confirm.is_none());
    let _ = app.handle_event(key(KeyCode::Esc)).unwrap();
    assert_eq!(app.current, 1);
}
