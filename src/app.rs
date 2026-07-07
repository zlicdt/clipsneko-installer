//! The linear wizard state machine and main render loop. Holds the ordered
//! list of steps, the current step index, the shared `InstallerState`, a
//! focus pointer (step body vs. Back/Next buttons), and a quit-confirmation
//! flag. Esc and Ctrl+C both request quit; the Back/Next buttons are the
//! only way to navigate between steps.

use crate::state::InstallerState;
use crate::steps::{build_steps, Step, StepAction};
use crate::t;
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;
use ratatui::Terminal;
use std::io::Stdout;

/// Which widget currently has keyboard focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    StepBody,
    BackButton,
    NextButton,
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
}

pub struct App {
    steps: Vec<Box<dyn Step>>,
    current: usize,
    state: InstallerState,
    quit_confirm: bool,
    focus: Focus,
}

impl App {
    pub fn new() -> Self {
        Self {
            steps: build_steps(),
            current: 0,
            state: InstallerState::default(),
            quit_confirm: false,
            focus: Focus::StepBody,
        }
    }

    fn back_enabled(&self) -> bool {
        self.current > 0
    }

    fn next_enabled(&self) -> bool {
        self.current + 1 < self.steps.len()
    }

    fn go_next(&mut self) {
        if self.next_enabled() {
            self.current += 1;
            self.focus = Focus::StepBody;
        }
    }

    fn go_back(&mut self) {
        if self.back_enabled() {
            self.current -= 1;
            self.focus = Focus::StepBody;
        }
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
            steps[self.current].render(frame, chunks[1], state);
        }

        self.render_footer(frame, chunks[2]);

        if self.quit_confirm {
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
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(16),
                Constraint::Min(0),
                Constraint::Length(16),
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
        let area = centered_rect(50, 8, frame.area());
        let quit_btn = Span::styled(
            format!("[ {} ]", t!("button.quit")),
            Style::default().add_modifier(Modifier::REVERSED),
        );
        let text = vec![
            Line::from(""),
            Line::from(t!("quit_dialog.title")),
            Line::from(""),
            Line::from(quit_btn),
            Line::from(""),
            Line::from(t!("quit_dialog.hint")),
        ];
        let dialog = Paragraph::new(text)
            .block(Block::default().borders(Borders::ALL))
            .alignment(Alignment::Center);
        frame.render_widget(Clear, area);
        frame.render_widget(dialog, area);
    }

    fn handle_event(&mut self, event: Event) -> Action {
        let Event::Key(key) = event else {
            return Action::Continue;
        };

        // Quit-confirmation dialog: Enter quits, Esc cancels, everything else
        // is swallowed so the user must pick one.
        if self.quit_confirm {
            if key.kind == KeyEventKind::Press {
                match key.code {
                    KeyCode::Enter => return Action::Quit,
                    KeyCode::Esc => self.quit_confirm = false,
                    _ => {}
                }
            }
            return Action::Continue;
        }

        if key.kind != KeyEventKind::Press {
            return Action::Continue;
        }

        // Global keys: Esc and Ctrl+C both request quit (with confirmation).
        if key.code == KeyCode::Esc {
            self.quit_confirm = true;
            return Action::Continue;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.quit_confirm = true;
            return Action::Continue;
        }
        if key.code == KeyCode::F(1) {
            // Help screen: not implemented yet.
            return Action::Continue;
        }

        // Tab / Shift+Tab: cycle focus between step body and the buttons.
        let is_shift_tab = matches!(key.code, KeyCode::BackTab)
            || (key.code == KeyCode::Tab && key.modifiers.contains(KeyModifiers::SHIFT));
        if is_shift_tab {
            self.focus = self.focus.prev(self.back_enabled(), self.next_enabled());
            return Action::Continue;
        }
        if key.code == KeyCode::Tab {
            self.focus = self.focus.next(self.back_enabled(), self.next_enabled());
            return Action::Continue;
        }

        // When a button has focus, only Tab/Shift+Tab/Enter are meaningful;
        // other keys are ignored so they don't bleed into the step body.
        match self.focus {
            Focus::BackButton => {
                if key.code == KeyCode::Enter {
                    self.go_back();
                }
                return Action::Continue;
            }
            Focus::NextButton => {
                if key.code == KeyCode::Enter {
                    self.go_next();
                }
                return Action::Continue;
            }
            Focus::StepBody => {}
        }

        // Step body has focus: dispatch the key to the step. A step may still
        // emit Next (e.g. Enter on a list) to advance; Back is also honored
        // for forward-compat with steps that have their own cancel logic.
        let action = self.steps[self.current].handle_key(key, &mut self.state);
        match action {
            StepAction::None => {}
            StepAction::Next => self.go_next(),
            StepAction::Back => self.go_back(),
            StepAction::Quit => return Action::Quit,
        }
        Action::Continue
    }
}

/// Compute a centered rect for dialogs. `width_pct` is the dialog width as
/// a percentage of `area.width`; `height_rows` is the dialog height as a
/// fixed number of rows (clamped to `area.height` so it never overflows).
fn centered_rect(width_pct: u16, height_rows: u16, area: Rect) -> Rect {
    let h = height_rows.min(area.height);
    let y = area.y + (area.height - h) / 2;
    let w_pad = (100u16).saturating_sub(width_pct) / 2;
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(w_pad),
            Constraint::Percentage(width_pct),
            Constraint::Percentage(w_pad),
        ])
        .split(area);
    let inner = horizontal[1];
    Rect::new(inner.x, y, inner.width, h)
}

/// Main loop. Draws the wizard and pumps events until the user quits.
pub fn run(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> anyhow::Result<()> {
    let mut app = App::new();
    loop {
        terminal.draw(|frame| app.render(frame))?;
        let event = crossterm::event::read()?;
        match app.handle_event(event) {
            Action::Continue => {}
            Action::Quit => break,
        }
    }
    Ok(())
}
