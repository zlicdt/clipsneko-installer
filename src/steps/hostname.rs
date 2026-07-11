//! Target hostname input step.
//!
//! The installer accepts one ASCII DNS label of at most 63 characters.
//! The value is stored in wizard state here and written to `/etc/hostname`
//! and the target `/etc/hosts` mapping during the install stage.

use crate::state::InstallerState;
use crate::steps::{Step, StepAction, StepId};
use crate::t;
use crate::util::ui::input_border_style;
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

/// Centered hostname form used by wizard step 10.
pub struct HostnameStep {
    hostname: String,
}

impl HostnameStep {
    /// Create an empty hostname form.
    pub fn new() -> Self {
        Self {
            hostname: String::new(),
        }
    }

    fn validation_message(&self) -> (String, Color) {
        if self.hostname.is_empty() {
            return (t!("hostname_step.error_required"), Color::Red);
        }
        if !hostname_is_valid(&self.hostname) {
            return (t!("hostname_step.error_invalid"), Color::Red);
        }
        (t!("hostname_step.ready"), Color::Green)
    }

    fn commit_and_advance(&self, state: &mut InstallerState) -> StepAction {
        if !hostname_is_valid(&self.hostname) {
            return StepAction::None;
        }

        state.hostname = Some(self.hostname.clone());
        StepAction::Next
    }
}

impl Default for HostnameStep {
    fn default() -> Self {
        Self::new()
    }
}

impl Step for HostnameStep {
    fn id(&self) -> StepId {
        StepId::Hostname
    }

    fn activate(&mut self, state: &mut InstallerState) -> Result<()> {
        if let Some(hostname) = &state.hostname {
            self.hostname.clone_from(hostname);
        }
        Ok(())
    }

    fn render(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        _state: &InstallerState,
        body_focused: bool,
    ) {
        let width = area.width.min(72);
        let height = area.height.min(10);
        let form_area = Rect::new(
            area.x + (area.width - width) / 2,
            area.y + (area.height - height) / 2,
            width,
            height,
        );
        let form_block = Block::default()
            .borders(Borders::ALL)
            .title(t!("hostname_step.form_title"));
        let inner = form_block.inner(form_area);
        frame.render_widget(form_block, form_area);

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(3),
                Constraint::Length(2),
                Constraint::Min(1),
            ])
            .split(inner);

        let cursor = if body_focused { "█" } else { "" };
        let display = format!("{}{cursor}", self.hostname);
        let visible_width = rows[1].width.saturating_sub(2) as usize;
        let display_width = Line::from(display.as_str()).width();
        let scroll = display_width.saturating_sub(visible_width) as u16;
        frame.render_widget(
            Paragraph::new(display)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(input_border_style(body_focused))
                        .title(t!("hostname_step.hostname")),
                )
                .scroll((0, scroll)),
            rows[1],
        );

        let (message, color) = self.validation_message();
        frame.render_widget(
            Paragraph::new(message)
                .alignment(Alignment::Center)
                .style(Style::default().fg(color)),
            rows[2],
        );
        frame.render_widget(
            Paragraph::new(t!("hostname_step.hint"))
                .alignment(Alignment::Center)
                .style(Style::default().add_modifier(Modifier::DIM)),
            rows[3],
        );
    }

    fn handle_key(&mut self, key: KeyEvent, state: &mut InstallerState) -> Result<StepAction> {
        if key.kind != KeyEventKind::Press {
            return Ok(StepAction::None);
        }

        Ok(match key.code {
            KeyCode::Enter => self.commit_and_advance(state),
            KeyCode::Backspace => {
                self.hostname.pop();
                StepAction::None
            }
            KeyCode::Char(character) => {
                self.hostname.push(character);
                StepAction::None
            }
            _ => StepAction::None,
        })
    }

    fn is_complete(&self, _state: &InstallerState) -> bool {
        hostname_is_valid(&self.hostname)
    }

    fn on_next_button(&mut self, state: &mut InstallerState) -> Result<StepAction> {
        Ok(self.commit_and_advance(state))
    }
}

fn hostname_is_valid(hostname: &str) -> bool {
    if hostname.is_empty() || hostname.len() > 63 {
        return false;
    }

    let bytes = hostname.as_bytes();
    bytes.first().is_some_and(u8::is_ascii_alphanumeric)
        && bytes.last().is_some_and(u8::is_ascii_alphanumeric)
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'-')
}

#[cfg(test)]
#[path = "hostname/tests.rs"]
mod tests;
