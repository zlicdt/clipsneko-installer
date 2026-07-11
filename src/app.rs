//! The linear wizard state machine and main render loop. Holds the ordered
//! list of steps, the current step index, the shared `InstallerState`, a
//! focus pointer (step body vs. Back/Next buttons), and a quit-confirmation
//! flag. Esc follows the Back path; Ctrl+C opens quit confirmation.

use crate::state::InstallerState;
use crate::steps::{build_steps, Step, StepAction};
use crate::t;
use crate::util::process::run_fullscreen;
use crate::util::ui::centered_rect;
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;
use ratatui::Terminal;
use std::io::Stdout;
use std::time::Duration;

/// Which widget currently has keyboard focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    StepBody,
    BackButton,
    NextButton,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QuitFocus {
    Cancel,
    Quit,
}

impl QuitFocus {
    fn toggle(self) -> Self {
        match self {
            Self::Cancel => Self::Quit,
            Self::Quit => Self::Cancel,
        }
    }
}

impl Focus {
    /// Cycle to the next focusable widget, skipping disabled buttons.
    fn next(self, back_enabled: bool, next_enabled: bool) -> Focus {
        match self {
            Focus::StepBody => {
                if back_enabled {
                    Focus::BackButton
                } else if next_enabled {
                    Focus::NextButton
                } else {
                    Focus::StepBody
                }
            }
            Focus::BackButton => {
                if next_enabled {
                    Focus::NextButton
                } else {
                    Focus::StepBody
                }
            }
            Focus::NextButton => Focus::StepBody,
        }
    }

    /// Cycle to the previous focusable widget, skipping disabled buttons.
    fn prev(self, back_enabled: bool, next_enabled: bool) -> Focus {
        match self {
            Focus::StepBody => {
                if next_enabled {
                    Focus::NextButton
                } else if back_enabled {
                    Focus::BackButton
                } else {
                    Focus::StepBody
                }
            }
            Focus::BackButton => Focus::StepBody,
            Focus::NextButton => {
                if back_enabled {
                    Focus::BackButton
                } else {
                    Focus::StepBody
                }
            }
        }
    }
}

/// Outcome of app-level event handling.
enum Action {
    Continue,
    Quit,
    /// Suspend ratatui, run the given program full-screen, then resume and
    /// notify the current step via `on_subprocess_done`.
    RunSubprocess(String, Vec<String>),
}

pub struct App {
    steps: Vec<Box<dyn Step>>,
    current: usize,
    state: InstallerState,
    quit_confirm: Option<QuitFocus>,
    focus: Focus,
}

impl App {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            steps: build_steps()?,
            current: 0,
            state: InstallerState::default(),
            quit_confirm: None,
            focus: Focus::StepBody,
        })
    }

    fn back_enabled(&self) -> bool {
        self.current > 0 && self.steps[self.current].allows_back()
    }

    fn next_enabled(&self) -> bool {
        self.current + 1 < self.steps.len() && self.steps[self.current].is_complete(&self.state)
    }

    fn go_next(&mut self) -> anyhow::Result<()> {
        if self.next_enabled() {
            self.current += 1;
            self.focus = Focus::StepBody;
            self.activate_current()?;
        }
        Ok(())
    }

    fn go_back(&mut self) -> anyhow::Result<()> {
        if self.back_enabled() {
            self.current -= 1;
            self.focus = Focus::StepBody;
            self.activate_current()?;
        }
        Ok(())
    }

    /// Call `activate` on the current step. Invoked on initial entry and on
    /// every Back/Next navigation so the step can run entry-time side effects
    /// (e.g. the network step runs its connectivity check here).
    fn activate_current(&mut self) -> anyhow::Result<()> {
        let state = &mut self.state;
        let steps = &mut self.steps;
        steps[self.current].activate(state)
    }

    /// Route a completed subprocess's exit status to the current step so it
    /// can react (e.g. the network step re-checks connectivity after nmtui).
    fn subprocess_done(&mut self, status: std::process::ExitStatus) -> anyhow::Result<()> {
        let state = &mut self.state;
        let steps = &mut self.steps;
        steps[self.current].on_subprocess_done(status, state)
    }

    fn tick(&mut self) -> anyhow::Result<Action> {
        let action = self.steps[self.current].tick(&mut self.state)?;
        self.apply_step_action(action)
    }

    /// Apply an action emitted by the current step, regardless of whether it
    /// came from the step body, a modal dialog, or a footer button.
    fn apply_step_action(&mut self, action: StepAction) -> anyhow::Result<Action> {
        Ok(match action {
            StepAction::None => Action::Continue,
            StepAction::Next => {
                self.go_next()?;
                Action::Continue
            }
            StepAction::Back => {
                self.go_back()?;
                Action::Continue
            }
            StepAction::Quit => Action::Quit,
            StepAction::SuspendRun(prog, args) => Action::RunSubprocess(prog, args),
        })
    }

    pub fn render(&mut self, frame: &mut Frame) {
        let area = frame.area();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(0),
                Constraint::Length(1),
            ])
            .split(area);

        self.render_header(frame, chunks[0]);

        // Split borrow: `steps` (mut, for stateful render) and `state`
        // (shared) are disjoint fields so Rust allows borrowing both.
        {
            let state = &self.state;
            let steps = &mut self.steps;
            steps[self.current].render(frame, chunks[1], state, self.focus == Focus::StepBody);
        }

        self.render_footer(frame, chunks[2]);

        if self.quit_confirm.is_some() {
            self.render_quit_dialog(frame);
        }
    }

    fn render_header(&self, frame: &mut Frame, area: Rect) {
        let title = t!("app.title");
        let indicator = format!(
            "{} {}/{}  {}",
            t!("app.step_indicator"),
            self.current + 1,
            self.steps.len(),
            self.steps[self.current].id().title(),
        );
        let header = Paragraph::new(vec![
            Line::from(Span::styled(
                title,
                Style::default().add_modifier(Modifier::BOLD),
            )),
            Line::from(indicator),
        ]);
        frame.render_widget(header, area);
    }

    /// Style for a footer button: dimmed when disabled, reversed when focused.
    fn button_style(&self, which: Focus, enabled: bool) -> Style {
        if !enabled {
            Style::default().add_modifier(Modifier::DIM)
        } else if self.focus == which {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        }
    }

    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        if !self.steps[self.current].shows_navigation_footer() {
            return;
        }
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(12),
                Constraint::Min(0),
                Constraint::Length(12),
            ])
            .split(area);

        let back_style = self.button_style(Focus::BackButton, self.back_enabled());
        let back = Paragraph::new(Line::from(Span::styled(
            format!("[ {} ]", t!("button.back")),
            back_style,
        )))
        .alignment(Alignment::Left);
        frame.render_widget(back, chunks[0]);

        let hint = t!("footer.hint");
        let hint_p = Paragraph::new(hint)
            .alignment(Alignment::Center)
            .style(Style::default().add_modifier(Modifier::DIM));
        frame.render_widget(hint_p, chunks[1]);

        let next_style = self.button_style(Focus::NextButton, self.next_enabled());
        let next = Paragraph::new(Line::from(Span::styled(
            format!("[ {} ]", t!("button.next")),
            next_style,
        )))
        .alignment(Alignment::Right);
        frame.render_widget(next, chunks[2]);
    }

    fn render_quit_dialog(&self, frame: &mut Frame) {
        let area = centered_rect(80, 8, frame.area());
        let focus = self.quit_confirm.unwrap_or(QuitFocus::Cancel);
        let cancel_style = if focus == QuitFocus::Cancel {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        let quit_style = if focus == QuitFocus::Quit {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        let cancel_btn = Span::styled(format!("[ {} ]", t!("button.cancel")), cancel_style);
        let quit_btn = Span::styled(format!("[ {} ]", t!("button.quit")), quit_style);
        let text = vec![
            Line::from(""),
            Line::from(t!("quit_dialog.title")),
            Line::from(""),
            Line::from(vec![cancel_btn, Span::raw("    "), quit_btn]),
            Line::from(""),
            Line::from(t!("quit_dialog.hint")),
        ];
        let dialog = Paragraph::new(text)
            .block(Block::default().borders(Borders::ALL))
            .alignment(Alignment::Center);
        frame.render_widget(Clear, area);
        frame.render_widget(dialog, area);
    }

    fn handle_event(&mut self, event: Event) -> anyhow::Result<Action> {
        let Event::Key(key) = event else {
            return Ok(Action::Continue);
        };

        // Quit confirmation defaults to Cancel. Left/Right or Tab changes the
        // focused button, Enter activates it, and Esc always cancels.
        if let Some(mut focus) = self.quit_confirm {
            if key.kind == KeyEventKind::Press {
                match key.code {
                    KeyCode::Enter => {
                        if focus == QuitFocus::Quit {
                            return Ok(Action::Quit);
                        }
                        self.quit_confirm = None;
                    }
                    KeyCode::Esc => self.quit_confirm = None,
                    KeyCode::Left | KeyCode::Right | KeyCode::Tab | KeyCode::BackTab => {
                        focus = focus.toggle();
                        self.quit_confirm = Some(focus);
                    }
                    _ => {}
                }
            }
            return Ok(Action::Continue);
        }

        if key.kind != KeyEventKind::Press {
            return Ok(Action::Continue);
        }

        // Ctrl+C is the one global quit shortcut even while a step dialog is
        // open. Quit confirmation itself still defaults to Cancel.
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            if self.steps[self.current].blocks_global_quit() {
                return Ok(Action::Continue);
            }
            self.quit_confirm = Some(QuitFocus::Cancel);
            return Ok(Action::Continue);
        }

        // Step-owned modal dialogs get first refusal on all remaining keys.
        // This must happen before Esc and Tab handling; otherwise focus can
        // move behind the overlay and Esc cannot cancel it.
        if self.steps[self.current].has_modal() {
            let action = self.steps[self.current].handle_key(key, &mut self.state)?;
            return self.apply_step_action(action);
        }

        // Esc follows the same path as the Back button.
        if key.code == KeyCode::Esc {
            let action = self.steps[self.current].on_back_button(&mut self.state)?;
            self.focus = Focus::StepBody;
            return self.apply_step_action(action);
        }
        // Tab / Shift+Tab focus cycling.
        //
        // Two layers: the global cycle (StepBody <-> Back/Next buttons) and,
        // within StepBody, a step-internal cycle (e.g. mirror's list <->
        // input). They compose via `Step::consume_tab`: when the step body
        // has focus the app first offers the Tab to the step; if the step
        // consumes it (returns true) the app does nothing, otherwise the app
        // performs the global cycle to the buttons. When a button has focus
        // the app cycles directly. This keeps the full loop
        // (StepBody -> buttons -> StepBody) reachable from every position —
        // a step must return false at the ends of its internal chain so Tab
        // can bubble up.
        let is_shift_tab = matches!(key.code, KeyCode::BackTab)
            || (key.code == KeyCode::Tab && key.modifiers.contains(KeyModifiers::SHIFT));
        let is_tab = key.code == KeyCode::Tab;
        if is_shift_tab || is_tab {
            match self.focus {
                Focus::StepBody => {
                    if !self.steps[self.current].consume_tab(is_shift_tab) {
                        self.focus = if is_shift_tab {
                            self.focus.prev(self.back_enabled(), self.next_enabled())
                        } else {
                            self.focus.next(self.back_enabled(), self.next_enabled())
                        };
                    }
                }
                _ => {
                    self.focus = if is_shift_tab {
                        self.focus.prev(self.back_enabled(), self.next_enabled())
                    } else {
                        self.focus.next(self.back_enabled(), self.next_enabled())
                    };
                }
            }
            return Ok(Action::Continue);
        }

        // When a button has focus, only Tab/Shift+Tab/Enter are meaningful;
        // other keys are ignored so they don't bleed into the step body.
        match self.focus {
            Focus::BackButton => {
                if key.code == KeyCode::Enter {
                    let action = self.steps[self.current].on_back_button(&mut self.state)?;
                    self.focus = Focus::StepBody;
                    return self.apply_step_action(action);
                }
                return Ok(Action::Continue);
            }
            Focus::NextButton => {
                if key.code == KeyCode::Enter {
                    let action = self.steps[self.current].on_next_button(&mut self.state)?;
                    self.focus = Focus::StepBody;
                    return self.apply_step_action(action);
                }
                return Ok(Action::Continue);
            }
            Focus::StepBody => {}
        }

        // Step body has focus: dispatch the key to the step. A step may still
        // emit Next (e.g. Enter on a list) to advance; Back is also honored
        // for forward-compat with steps that have their own cancel logic.
        let action = self.steps[self.current].handle_key(key, &mut self.state)?;
        self.apply_step_action(action)
    }
}

#[cfg(test)]
mod tests;

/// Main loop. Draws the wizard and pumps events until the user quits.
pub fn run(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> anyhow::Result<()> {
    let mut app = App::new()?;
    app.activate_current()?;
    loop {
        if matches!(app.tick()?, Action::Quit) {
            break;
        }
        terminal.draw(|frame| app.render(frame))?;
        if !crossterm::event::poll(Duration::from_millis(100))? {
            continue;
        }
        let event = crossterm::event::read()?;
        match app.handle_event(event)? {
            Action::Continue => {}
            Action::Quit => break,
            Action::RunSubprocess(prog, args) => {
                let args_ref: Vec<&str> = args.iter().map(String::as_str).collect();
                let status = run_fullscreen(&prog, &args_ref)
                    .map_err(|e| anyhow::anyhow!("subprocess {prog} failed: {e}"))?;
                terminal.clear()?;
                app.subprocess_done(status)?;
            }
        }
    }
    Ok(())
}
