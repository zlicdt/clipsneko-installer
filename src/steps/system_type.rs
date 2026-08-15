//! System type selection step.
//!
//! The three supported install profiles — a bare server or one of the two
//! supported desktop environments — are shown as a single-select list with a
//! development-tools checkbox below it. Hyprland is selected by default.
//! Space records the highlighted choice or toggles the checkbox; Enter
//! records and advances. Tab/Shift+Tab cycles focus between the list and the
//! checkbox before bubbling out to the footer buttons.

use crate::state::{InstallerState, SystemType};
use crate::steps::{Step, StepAction, StepId};
use crate::t;
use crate::util::ui::{focusable_block, rounded_block, selected_style, wrap_plain};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};
use ratatui::Frame;

/// Default selected system type for a new installer session.
pub const DEFAULT_SYSTEM_TYPE: SystemType = SystemType::Hyprland;

/// Which sub-widget currently has focus within the step body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SystemTypeFocus {
    ProfileList,
    DevTools,
}

/// System type picker plus development-tools checkbox used by wizard step 6.
pub struct SystemTypeStep {
    list_state: ListState,
    focus: SystemTypeFocus,
}

impl SystemTypeStep {
    /// Create the picker with Hyprland highlighted and the list focused.
    pub fn new() -> Self {
        let mut list_state = ListState::default();
        list_state.select(Some(choice_index(DEFAULT_SYSTEM_TYPE)));
        Self {
            list_state,
            focus: SystemTypeFocus::ProfileList,
        }
    }

    fn highlighted(&self) -> SystemType {
        let index = self.list_state.selected().unwrap_or(0);
        SystemType::ALL[index]
    }

    fn move_highlight(&mut self, delta: i32) {
        let len = SystemType::ALL.len() as i32;
        let current = self.list_state.selected().unwrap_or(0) as i32;
        let next = (current + delta).rem_euclid(len) as usize;
        self.list_state.select(Some(next));
    }

    fn commit_highlight(&self, state: &mut InstallerState) {
        state.system_type = Some(self.highlighted());
    }
}

impl Default for SystemTypeStep {
    fn default() -> Self {
        Self::new()
    }
}

impl Step for SystemTypeStep {
    fn id(&self) -> StepId {
        StepId::SystemType
    }

    fn activate(&mut self, state: &mut InstallerState) -> Result<()> {
        let selected = state.system_type.unwrap_or(DEFAULT_SYSTEM_TYPE);
        state.system_type = Some(selected);
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
            .constraints([
                Constraint::Min(0),
                Constraint::Length(3),
                Constraint::Length(1),
            ])
            .split(area);

        // List items cannot wrap on their own and some translated labels are
        // wider than the terminal, so wrap them by hand.
        let item_width = chunks[0].width.saturating_sub(2);
        let items = SystemType::ALL.map(|choice| {
            let style = if state.system_type == Some(choice) {
                selected_style()
            } else {
                Style::default()
            };
            let lines: Vec<ratatui::text::Line> = wrap_plain(&choice_label(choice), item_width)
                .into_iter()
                .map(ratatui::text::Line::from)
                .collect();
            ListItem::new(lines).style(style)
        });
        let list_focused = body_focused && self.focus == SystemTypeFocus::ProfileList;
        let list = List::new(items)
            .block(focusable_block(
                rounded_block().title(t!("system_type_step.title")),
                list_focused,
            ))
            .highlight_style(if list_focused {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            });
        frame.render_stateful_widget(list, chunks[0], &mut self.list_state);

        let checkbox_focused = body_focused && self.focus == SystemTypeFocus::DevTools;
        let marker = if state.dev_tools { "[x]" } else { "[ ]" };
        let checkbox_style = if state.dev_tools {
            selected_style()
        } else {
            Style::default()
        };
        let checkbox = Paragraph::new(format!("{marker} {}", t!("system_type_step.dev_tools")))
            .block(focusable_block(rounded_block(), checkbox_focused))
            .style(checkbox_style);
        frame.render_widget(checkbox, chunks[1]);

        let hint = Paragraph::new(t!("system_type_step.key_hint"))
            .alignment(Alignment::Center)
            .style(Style::default().add_modifier(Modifier::DIM));
        frame.render_widget(hint, chunks[2]);
    }

    fn handle_key(&mut self, key: KeyEvent, state: &mut InstallerState) -> Result<StepAction> {
        if key.kind != KeyEventKind::Press {
            return Ok(StepAction::None);
        }

        Ok(match key.code {
            KeyCode::Down | KeyCode::Char('j') if self.focus == SystemTypeFocus::ProfileList => {
                self.move_highlight(1);
                StepAction::None
            }
            KeyCode::Up | KeyCode::Char('k') if self.focus == SystemTypeFocus::ProfileList => {
                self.move_highlight(-1);
                StepAction::None
            }
            KeyCode::Char(' ') => {
                match self.focus {
                    SystemTypeFocus::ProfileList => self.commit_highlight(state),
                    SystemTypeFocus::DevTools => state.dev_tools = !state.dev_tools,
                }
                StepAction::None
            }
            KeyCode::Enter => {
                self.commit_highlight(state);
                StepAction::Next
            }
            _ => StepAction::None,
        })
    }

    fn consume_tab(&mut self, is_shift: bool) -> bool {
        match (self.focus, is_shift) {
            (SystemTypeFocus::ProfileList, false) => {
                self.focus = SystemTypeFocus::DevTools;
                true
            }
            (SystemTypeFocus::DevTools, false) => {
                self.focus = SystemTypeFocus::ProfileList;
                false
            }
            (SystemTypeFocus::DevTools, true) => {
                self.focus = SystemTypeFocus::ProfileList;
                true
            }
            (SystemTypeFocus::ProfileList, true) => {
                self.focus = SystemTypeFocus::DevTools;
                false
            }
        }
    }

    fn is_complete(&self, state: &InstallerState) -> bool {
        state.system_type.is_some()
    }

    fn on_next_button(&mut self, state: &mut InstallerState) -> Result<StepAction> {
        self.commit_highlight(state);
        Ok(StepAction::Next)
    }
}

fn choice_index(choice: SystemType) -> usize {
    SystemType::ALL
        .iter()
        .position(|candidate| *candidate == choice)
        .expect("every SystemType must be present in SystemType::ALL")
}

fn choice_label(choice: SystemType) -> String {
    match choice {
        SystemType::Server => t!("system_type_step.option.server"),
        SystemType::Kde => t!("system_type_step.option.kde"),
        SystemType::Hyprland => t!("system_type_step.option.hyprland"),
    }
}

#[cfg(test)]
#[path = "system_type/tests.rs"]
mod tests;
