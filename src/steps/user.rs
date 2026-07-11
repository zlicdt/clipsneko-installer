//! Local user account form.
//!
//! Passwords are always kept in `SecretString` buffers. The UI displays only
//! bullets, accepts every non-empty password regardless of strength, and
//! blocks forward navigation only for invalid usernames, empty passwords, or
//! a confirmation mismatch.

use crate::state::{InstallerState, UserInfo};
use crate::steps::{Step, StepAction, StepId};
use crate::t;
use crate::util::password::{password_strength, PasswordStrength, SecretString};
use crate::util::ui::focusable_block;
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Gauge, Paragraph};
use ratatui::Frame;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UserFocus {
    Username,
    Password,
    ConfirmPassword,
}

/// Centered username and password form used by wizard step 9.
pub struct UserStep {
    username: String,
    password: SecretString,
    confirm_password: SecretString,
    focus: UserFocus,
}

impl UserStep {
    /// Create an empty account form focused on the username field.
    pub fn new() -> Self {
        Self {
            username: String::new(),
            password: SecretString::default(),
            confirm_password: SecretString::default(),
            focus: UserFocus::Username,
        }
    }

    fn form_is_valid(&self) -> bool {
        username_is_valid(&self.username)
            && !self.password.is_empty()
            && self.password.expose_secret() == self.confirm_password.expose_secret()
    }

    fn state_is_committed(state: &InstallerState) -> bool {
        state
            .user
            .as_ref()
            .is_some_and(|user| user.password_set && username_is_valid(&user.username))
            && state.user_password.is_some()
    }

    fn validation_message(&self) -> (String, Color) {
        if self.username.is_empty() {
            return (t!("user_step.error_username_required"), Color::Red);
        }
        if !username_is_valid(&self.username) {
            return (t!("user_step.error_username_invalid"), Color::Red);
        }
        if self.password.is_empty() {
            return (t!("user_step.error_password_required"), Color::Red);
        }
        if self.password.expose_secret() != self.confirm_password.expose_secret() {
            return (t!("user_step.error_password_mismatch"), Color::Red);
        }
        (t!("user_step.ready"), Color::Green)
    }

    fn commit_and_advance(&mut self, state: &mut InstallerState) -> StepAction {
        if !self.form_is_valid() {
            return StepAction::None;
        }

        state.user = Some(UserInfo {
            username: self.username.clone(),
            password_set: true,
        });
        state.user_password = Some(std::mem::take(&mut self.password));
        self.confirm_password.clear();
        StepAction::Next
    }

    fn render_input(
        &self,
        frame: &mut Frame,
        area: Rect,
        title: String,
        value: String,
        field: UserFocus,
        body_focused: bool,
    ) {
        let focused = body_focused && self.focus == field;
        let cursor = if focused { "█" } else { "" };
        let display = format!("{value}{cursor}");
        let visible_width = area.width.saturating_sub(2) as usize;
        let display_width = Line::from(display.as_str()).width();
        let scroll = display_width.saturating_sub(visible_width) as u16;
        frame.render_widget(
            Paragraph::new(display)
                .block(focusable_block(
                    Block::default().borders(Borders::ALL).title(title),
                    focused,
                ))
                .scroll((0, scroll)),
            area,
        );
    }

    fn masked(secret: &SecretString) -> String {
        "•".repeat(secret.expose_secret().chars().count())
    }

    fn strength_label(strength: PasswordStrength) -> String {
        match strength {
            PasswordStrength::Weak => t!("user_step.strength_weak"),
            PasswordStrength::Fair => t!("user_step.strength_fair"),
            PasswordStrength::Good => t!("user_step.strength_good"),
            PasswordStrength::Strong => t!("user_step.strength_strong"),
        }
    }

    fn strength_style(strength: PasswordStrength) -> (f64, Color) {
        match strength {
            PasswordStrength::Weak => (0.25, Color::Red),
            PasswordStrength::Fair => (0.5, Color::Yellow),
            PasswordStrength::Good => (0.75, Color::Cyan),
            PasswordStrength::Strong => (1.0, Color::Green),
        }
    }
}

impl Default for UserStep {
    fn default() -> Self {
        Self::new()
    }
}

impl Step for UserStep {
    fn id(&self) -> StepId {
        StepId::User
    }

    fn activate(&mut self, state: &mut InstallerState) -> Result<()> {
        if let Some(user) = &state.user {
            self.username.clone_from(&user.username);
        }
        if self.password.is_empty() {
            if let Some(password) = state.user_password.take() {
                self.confirm_password = SecretString::new(password.expose_secret().to_owned());
                self.password = password;
            }
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
        let height = area.height.min(16);
        let form_area = Rect::new(
            area.x + (area.width - width) / 2,
            area.y + (area.height - height) / 2,
            width,
            height,
        );
        let form_block = Block::default()
            .borders(Borders::ALL)
            .title(t!("user_step.form_title"));
        let inner = form_block.inner(form_area);
        frame.render_widget(form_block, form_area);

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Min(2),
            ])
            .split(inner);

        self.render_input(
            frame,
            rows[0],
            t!("user_step.username"),
            self.username.clone(),
            UserFocus::Username,
            body_focused,
        );
        self.render_input(
            frame,
            rows[1],
            t!("user_step.password"),
            Self::masked(&self.password),
            UserFocus::Password,
            body_focused,
        );
        self.render_input(
            frame,
            rows[2],
            t!("user_step.confirm_password"),
            Self::masked(&self.confirm_password),
            UserFocus::ConfirmPassword,
            body_focused,
        );

        let strength = password_strength(self.password.expose_secret());
        let (ratio, color) = Self::strength_style(strength);
        let strength_label = format!(
            "{}: {}",
            t!("user_step.strength"),
            Self::strength_label(strength)
        );
        frame.render_widget(
            Gauge::default()
                .block(Block::default().borders(Borders::ALL))
                .gauge_style(Style::default().fg(color).bg(Color::Black))
                .ratio(ratio)
                .label(strength_label),
            rows[3],
        );

        let (message, color) = self.validation_message();
        frame.render_widget(
            Paragraph::new(message)
                .alignment(Alignment::Center)
                .style(Style::default().fg(color)),
            rows[4],
        );
    }

    fn handle_key(&mut self, key: KeyEvent, state: &mut InstallerState) -> Result<StepAction> {
        if key.kind != KeyEventKind::Press {
            return Ok(StepAction::None);
        }

        Ok(match (self.focus, key.code) {
            (UserFocus::Username, KeyCode::Enter) => {
                self.focus = UserFocus::Password;
                StepAction::None
            }
            (UserFocus::Password, KeyCode::Enter) => {
                self.focus = UserFocus::ConfirmPassword;
                StepAction::None
            }
            (UserFocus::ConfirmPassword, KeyCode::Enter) => self.commit_and_advance(state),
            (UserFocus::Username, KeyCode::Backspace) => {
                self.username.pop();
                StepAction::None
            }
            (UserFocus::Password, KeyCode::Backspace) => {
                self.password.pop();
                StepAction::None
            }
            (UserFocus::ConfirmPassword, KeyCode::Backspace) => {
                self.confirm_password.pop();
                StepAction::None
            }
            (UserFocus::Username, KeyCode::Char(character)) => {
                self.username.push(character);
                StepAction::None
            }
            (UserFocus::Password, KeyCode::Char(character)) => {
                self.password.push(character);
                StepAction::None
            }
            (UserFocus::ConfirmPassword, KeyCode::Char(character)) => {
                self.confirm_password.push(character);
                StepAction::None
            }
            _ => StepAction::None,
        })
    }

    fn consume_tab(&mut self, is_shift: bool) -> bool {
        match (self.focus, is_shift) {
            (UserFocus::Username, false) => {
                self.focus = UserFocus::Password;
                true
            }
            (UserFocus::Password, false) => {
                self.focus = UserFocus::ConfirmPassword;
                true
            }
            (UserFocus::ConfirmPassword, false) => {
                self.focus = UserFocus::Username;
                false
            }
            (UserFocus::ConfirmPassword, true) => {
                self.focus = UserFocus::Password;
                true
            }
            (UserFocus::Password, true) => {
                self.focus = UserFocus::Username;
                true
            }
            (UserFocus::Username, true) => {
                self.focus = UserFocus::ConfirmPassword;
                false
            }
        }
    }

    fn is_complete(&self, state: &InstallerState) -> bool {
        self.form_is_valid() || Self::state_is_committed(state)
    }

    fn on_next_button(&mut self, state: &mut InstallerState) -> Result<StepAction> {
        Ok(self.commit_and_advance(state))
    }
}

fn username_is_valid(username: &str) -> bool {
    let mut characters = username.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    (first.is_ascii_lowercase() || first == '_')
        && characters.all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '_' | '-')
        })
}

#[cfg(test)]
mod tests;
