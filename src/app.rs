//! The linear wizard state machine and main render loop. Holds the ordered
//! list of steps, the current step index, the shared `InstallerState`, and a
//! quit-confirmation flag. Global keys (Ctrl+C / F1) are handled here; all
//! other keys are dispatched to the current step.

use crate::state::InstallerState;
use crate::steps::{build_steps, Step, StepAction};
use crate::t;
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;
use ratatui::Terminal;
use std::io::Stdout;

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
}

impl App {
    pub fn new() -> Self {
        Self {
            steps: build_steps(),
            current: 0,
            state: InstallerState::default(),
            quit_confirm: false,
        }
    }

    pub fn render(&self, frame: &mut Frame) {
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
        self.steps[self.current].render(frame, chunks[1], &self.state);
        self.render_footer(frame, chunks[2]);

        if self.quit_confirm {
            self.render_quit_dialog(frame);
        }
    }

    fn render_header(&self, frame: &mut Frame, area: Rect) {
        let title = t!("ClipsNeko Linux Installer");
        let indicator = format!(
            "{} {}/{}  {}",
            t!("Step"),
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

    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        let hint = t!("Enter=Next  Esc=Back  Ctrl+C=Quit  F1=Help");
        let footer = Paragraph::new(hint)
            .alignment(Alignment::Center)
            .style(Style::default().add_modifier(Modifier::DIM));
        frame.render_widget(footer, area);
    }

    fn render_quit_dialog(&self, frame: &mut Frame) {
        let area = centered_rect(60, 7, frame.area());
        let text = vec![
            Line::from(""),
            Line::from(t!("Are you sure you want to quit?")),
            Line::from(""),
            Line::from(t!("Press Y to quit, any other key to cancel.")),
        ];
        let dialog = Paragraph::new(text)
            .block(Block::default().borders(Borders::ALL))
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true });
        frame.render_widget(Clear, area);
        frame.render_widget(dialog, area);
    }

    fn handle_event(&mut self, event: Event) -> Action {
        let Event::Key(key) = event else {
            return Action::Continue;
        };

        if self.quit_confirm {
            if key.kind == KeyEventKind::Press
                && (key.code == KeyCode::Char('y') || key.code == KeyCode::Char('Y'))
            {
                return Action::Quit;
            }
            self.quit_confirm = false;
            return Action::Continue;
        }

        if key.kind == KeyEventKind::Press {
            if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                self.quit_confirm = true;
                return Action::Continue;
            }
            if key.code == KeyCode::F(1) {
                // Help screen: not implemented for the stub phase.
                return Action::Continue;
            }
        }

        let action = self.steps[self.current].handle_key(key, &mut self.state);
        match action {
            StepAction::None => {}
            StepAction::Next => {
                if self.current + 1 < self.steps.len() {
                    self.current += 1;
                }
            }
            StepAction::Back => {
                if self.current > 0 {
                    self.current -= 1;
                }
            }
            StepAction::Quit => return Action::Quit,
        }
        Action::Continue
    }
}

/// Compute a centered rect for dialogs. `w`/`h` are percentages.
fn centered_rect(w: u16, h: u16, area: Rect) -> Rect {
    let h_pad = (100u16).saturating_sub(h) / 2;
    let w_pad = (100u16).saturating_sub(w) / 2;
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(h_pad),
            Constraint::Percentage(h),
            Constraint::Percentage(h_pad),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(w_pad),
            Constraint::Percentage(w),
            Constraint::Percentage(w_pad),
        ])
        .split(vertical[1])[1]
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
