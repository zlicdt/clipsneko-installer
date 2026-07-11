//! NVIDIA driver selection step.
//!
//! Every supported driver remains visible, while choices incompatible with
//! the selected kernel are dimmed and skipped by navigation. The DKMS variant
//! is the default and supports every kernel in the installer.

use crate::state::{InstallerState, KernelChoice, NvidiaChoice};
use crate::steps::{Step, StepAction, StepId};
use crate::t;
use crate::util::ui::focusable_block;
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

/// Default NVIDIA choice for a new installer session.
pub const DEFAULT_NVIDIA: NvidiaChoice = NvidiaChoice::NvidiaOpenDkms;

/// Single-select NVIDIA driver picker used by wizard step 7.
pub struct NvidiaStep {
    list_state: ListState,
}

impl NvidiaStep {
    /// Create the picker with `nvidia-open-dkms` highlighted.
    pub fn new() -> Self {
        let mut list_state = ListState::default();
        list_state.select(Some(choice_index(DEFAULT_NVIDIA)));
        Self { list_state }
    }

    fn kernel(state: &InstallerState) -> KernelChoice {
        state.kernel.unwrap_or_default()
    }

    fn highlighted(&self) -> NvidiaChoice {
        let index = self
            .list_state
            .selected()
            .unwrap_or(choice_index(DEFAULT_NVIDIA));
        NvidiaChoice::ALL[index]
    }

    fn move_highlight(&mut self, delta: i32, kernel: KernelChoice) {
        let len = NvidiaChoice::ALL.len() as i32;
        let current = self
            .list_state
            .selected()
            .unwrap_or(choice_index(DEFAULT_NVIDIA)) as i32;

        for offset in 1..=len {
            let next = (current + delta * offset).rem_euclid(len) as usize;
            if NvidiaChoice::ALL[next].is_compatible_with(kernel) {
                self.list_state.select(Some(next));
                return;
            }
        }
    }

    fn commit_highlight(&self, state: &mut InstallerState) {
        let choice = self.highlighted();
        if choice.is_compatible_with(Self::kernel(state)) {
            state.nvidia = choice;
        }
    }
}

impl Default for NvidiaStep {
    fn default() -> Self {
        Self::new()
    }
}

impl Step for NvidiaStep {
    fn id(&self) -> StepId {
        StepId::Nvidia
    }

    fn activate(&mut self, state: &mut InstallerState) -> Result<()> {
        let kernel = Self::kernel(state);
        if !state.nvidia.is_compatible_with(kernel) {
            state.nvidia = DEFAULT_NVIDIA;
        }
        self.list_state.select(Some(choice_index(state.nvidia)));
        Ok(())
    }

    fn render(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        state: &InstallerState,
        body_focused: bool,
    ) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(2)])
            .split(area);
        let kernel = Self::kernel(state);

        let items = NvidiaChoice::ALL.map(|choice| {
            let compatible = choice.is_compatible_with(kernel);
            let label = if compatible {
                choice_label(choice)
            } else {
                format!(
                    "{} {}",
                    choice_label(choice),
                    t!("nvidia_step.incompatible_suffix")
                )
            };
            let mut style = Style::default();
            if !compatible {
                style = style.add_modifier(Modifier::DIM);
            } else if state.nvidia == choice {
                style = style.add_modifier(Modifier::BOLD);
            }
            ListItem::new(label).style(style)
        });
        let highlight_style = if body_focused {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        let list = List::new(items)
            .block(focusable_block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(t!("nvidia_step.title")),
                body_focused,
            ))
            .highlight_style(highlight_style);
        frame.render_stateful_widget(list, chunks[0], &mut self.list_state);

        let hint = Paragraph::new(vec![
            t!("nvidia_step.headers_hint").into(),
            t!("nvidia_step.key_hint").into(),
        ])
        .alignment(Alignment::Center)
        .style(Style::default().add_modifier(Modifier::DIM));
        frame.render_widget(hint, chunks[1]);
    }

    fn handle_key(&mut self, key: KeyEvent, state: &mut InstallerState) -> Result<StepAction> {
        if key.kind != KeyEventKind::Press {
            return Ok(StepAction::None);
        }

        Ok(match key.code {
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_highlight(1, Self::kernel(state));
                StepAction::None
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_highlight(-1, Self::kernel(state));
                StepAction::None
            }
            KeyCode::Char(' ') => {
                self.commit_highlight(state);
                StepAction::None
            }
            KeyCode::Enter => {
                self.commit_highlight(state);
                StepAction::Next
            }
            _ => StepAction::None,
        })
    }

    fn on_next_button(&mut self, state: &mut InstallerState) -> Result<StepAction> {
        self.commit_highlight(state);
        Ok(StepAction::Next)
    }
}

fn choice_index(choice: NvidiaChoice) -> usize {
    NvidiaChoice::ALL
        .iter()
        .position(|candidate| *candidate == choice)
        .expect("every NvidiaChoice must be present in NvidiaChoice::ALL")
}

fn choice_label(choice: NvidiaChoice) -> String {
    match choice {
        NvidiaChoice::None => t!("nvidia_step.option.none"),
        NvidiaChoice::NvidiaOpen => t!("nvidia_step.option.nvidia_open"),
        NvidiaChoice::NvidiaOpenDkms => t!("nvidia_step.option.nvidia_open_dkms"),
        NvidiaChoice::NvidiaOpenLts => t!("nvidia_step.option.nvidia_open_lts"),
    }
}

#[cfg(test)]
#[path = "nvidia/tests.rs"]
mod tests;
