//! Kernel selection step.
//!
//! The four supported Arch kernel packages are shown as a single-select list.
//! `linux-zen` is selected by default. Space records the highlighted choice;
//! Enter records it and advances. The matching headers package is derived by
//! `KernelChoice` and is always installed alongside the selected kernel.

use crate::state::{InstallerState, KernelChoice};
use crate::steps::{Step, StepAction, StepId};
use crate::t;
use crate::util::ui::focusable_block;
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

/// Default selected kernel for a new installer session.
pub const DEFAULT_KERNEL: KernelChoice = KernelChoice::LinuxZen;

/// Single-select kernel picker used by wizard step 6.
pub struct KernelStep {
    list_state: ListState,
}

impl KernelStep {
    /// Create the picker with `linux-zen` highlighted.
    pub fn new() -> Self {
        let mut list_state = ListState::default();
        list_state.select(Some(choice_index(DEFAULT_KERNEL)));
        Self { list_state }
    }

    fn highlighted(&self) -> KernelChoice {
        let index = self.list_state.selected().unwrap_or(0);
        KernelChoice::ALL[index]
    }

    fn move_highlight(&mut self, delta: i32) {
        let len = KernelChoice::ALL.len() as i32;
        let current = self.list_state.selected().unwrap_or(0) as i32;
        let next = (current + delta).rem_euclid(len) as usize;
        self.list_state.select(Some(next));
    }

    fn commit_highlight(&self, state: &mut InstallerState) {
        state.kernel = Some(self.highlighted());
    }
}

impl Default for KernelStep {
    fn default() -> Self {
        Self::new()
    }
}

impl Step for KernelStep {
    fn id(&self) -> StepId {
        StepId::Kernel
    }

    fn activate(&mut self, state: &mut InstallerState) -> Result<()> {
        let selected = state.kernel.unwrap_or(DEFAULT_KERNEL);
        state.kernel = Some(selected);
        self.list_state.select(Some(choice_index(selected)));
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

        let items = KernelChoice::ALL.map(|choice| {
            let style = if state.kernel == Some(choice) {
                Style::default().add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(choice_label(choice)).style(style)
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
                    .title(t!("kernel_step.title")),
                body_focused,
            ))
            .highlight_style(highlight_style);
        frame.render_stateful_widget(list, chunks[0], &mut self.list_state);

        let hint = Paragraph::new(vec![
            t!("kernel_step.headers_hint").into(),
            t!("kernel_step.key_hint").into(),
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
                self.move_highlight(1);
                StepAction::None
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_highlight(-1);
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

    fn is_complete(&self, state: &InstallerState) -> bool {
        state.kernel.is_some()
    }

    fn on_next_button(&mut self, state: &mut InstallerState) -> Result<StepAction> {
        self.commit_highlight(state);
        Ok(StepAction::Next)
    }
}

fn choice_index(choice: KernelChoice) -> usize {
    KernelChoice::ALL
        .iter()
        .position(|candidate| *candidate == choice)
        .expect("every KernelChoice must be present in KernelChoice::ALL")
}

fn choice_label(choice: KernelChoice) -> String {
    match choice {
        KernelChoice::Linux => t!("kernel_step.option.linux"),
        KernelChoice::LinuxLts => t!("kernel_step.option.linux_lts"),
        KernelChoice::LinuxZen => t!("kernel_step.option.linux_zen"),
        KernelChoice::LinuxHardened => t!("kernel_step.option.linux_hardened"),
    }
}

#[cfg(test)]
#[path = "kernel/tests.rs"]
mod tests;
