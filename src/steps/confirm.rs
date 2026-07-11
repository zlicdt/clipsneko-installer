//! Final review of the configuration collected by the wizard.
//!
//! The summary deliberately excludes the account password. Advancing opens a
//! blocking confirmation dialog before the install step can begin.

use crate::state::{BtrfsRaidMode, InstallerState, NvidiaChoice};
use crate::steps::{Step, StepAction, StepId};
use crate::t;
use crate::util::ui::{centered_rect, focusable_block};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DialogFocus {
    Cancel,
    Install,
}

impl DialogFocus {
    fn toggle(self) -> Self {
        match self {
            Self::Cancel => Self::Install,
            Self::Install => Self::Cancel,
        }
    }
}

/// Scrollable installation summary with a final destructive-action dialog.
pub struct ConfirmStep {
    scroll: u16,
    max_scroll: u16,
    dialog_focus: Option<DialogFocus>,
}

impl ConfirmStep {
    /// Create the confirmation page at the top of its summary.
    pub fn new() -> Self {
        Self {
            scroll: 0,
            max_scroll: 0,
            dialog_focus: None,
        }
    }

    fn open_dialog(&mut self, state: &InstallerState) -> StepAction {
        if summary_is_complete(state) {
            self.dialog_focus = Some(DialogFocus::Cancel);
        }
        StepAction::None
    }

    fn move_scroll(&mut self, delta: i32) {
        self.scroll = (self.scroll as i32 + delta).clamp(0, self.max_scroll as i32) as u16;
    }

    fn render_dialog(&self, frame: &mut Frame) {
        let area = centered_rect(80, 9, frame.area());
        let focus = self.dialog_focus.unwrap_or(DialogFocus::Cancel);
        let cancel_style = if focus == DialogFocus::Cancel {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        let install_style = if focus == DialogFocus::Install {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        let lines = vec![
            Line::from(""),
            Line::from(t!("confirm_step.dialog.body")),
            Line::from(""),
            Line::from(vec![
                Span::styled(format!("[ {} ]", t!("button.cancel")), cancel_style),
                Span::raw("    "),
                Span::styled(
                    format!("[ {} ]", t!("confirm_step.dialog.install")),
                    install_style,
                ),
            ]),
            Line::from(""),
            Line::from(t!("confirm_step.dialog.hint")),
        ];
        let dialog = Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(t!("confirm_step.dialog.title")),
            )
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true });
        frame.render_widget(Clear, area);
        frame.render_widget(dialog, area);
    }
}

impl Default for ConfirmStep {
    fn default() -> Self {
        Self::new()
    }
}

impl Step for ConfirmStep {
    fn id(&self) -> StepId {
        StepId::Confirm
    }

    fn activate(&mut self, _state: &mut InstallerState) -> Result<()> {
        self.scroll = 0;
        self.dialog_focus = None;
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
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(area);
        let lines = summary_lines(state);
        let visible_rows = chunks[0].height.saturating_sub(2) as usize;
        self.max_scroll = lines
            .len()
            .saturating_sub(visible_rows)
            .min(u16::MAX as usize) as u16;
        self.scroll = self.scroll.min(self.max_scroll);

        let summary = Paragraph::new(lines)
            .block(focusable_block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(t!("confirm_step.summary_title")),
                body_focused,
            ))
            .scroll((self.scroll, 0));
        frame.render_widget(summary, chunks[0]);
        frame.render_widget(
            Paragraph::new(t!("confirm_step.hint"))
                .alignment(Alignment::Center)
                .style(Style::default().add_modifier(Modifier::DIM)),
            chunks[1],
        );

        if self.dialog_focus.is_some() {
            self.render_dialog(frame);
        }
    }

    fn handle_key(&mut self, key: KeyEvent, state: &mut InstallerState) -> Result<StepAction> {
        if key.kind != KeyEventKind::Press {
            return Ok(StepAction::None);
        }

        if let Some(focus) = self.dialog_focus {
            return Ok(match key.code {
                KeyCode::Left | KeyCode::Right | KeyCode::Tab | KeyCode::BackTab => {
                    self.dialog_focus = Some(focus.toggle());
                    StepAction::None
                }
                KeyCode::Enter if focus == DialogFocus::Install => {
                    self.dialog_focus = None;
                    StepAction::Next
                }
                KeyCode::Enter | KeyCode::Esc => {
                    self.dialog_focus = None;
                    StepAction::None
                }
                _ => StepAction::None,
            });
        }

        Ok(match key.code {
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_scroll(1);
                StepAction::None
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_scroll(-1);
                StepAction::None
            }
            KeyCode::PageDown => {
                self.move_scroll(5);
                StepAction::None
            }
            KeyCode::PageUp => {
                self.move_scroll(-5);
                StepAction::None
            }
            KeyCode::Home => {
                self.scroll = 0;
                StepAction::None
            }
            KeyCode::End => {
                self.scroll = self.max_scroll;
                StepAction::None
            }
            KeyCode::Enter => self.open_dialog(state),
            _ => StepAction::None,
        })
    }

    fn is_complete(&self, state: &InstallerState) -> bool {
        summary_is_complete(state)
    }

    fn has_modal(&self) -> bool {
        self.dialog_focus.is_some()
    }

    fn on_next_button(&mut self, state: &mut InstallerState) -> Result<StepAction> {
        Ok(self.open_dialog(state))
    }
}

fn summary_is_complete(state: &InstallerState) -> bool {
    state
        .target_locale
        .as_ref()
        .is_some_and(|locale| state.target_locales.contains(locale))
        && !state.target_locales.is_empty()
        && state.keymap.is_some()
        && state.kernel.is_some()
        && state.timezone.is_some()
        && state.hostname.is_some()
        && state
            .user
            .as_ref()
            .is_some_and(|user| user.password_set && !user.username.is_empty())
        && state.user_password.is_some()
        && state.disk.esp_partition.is_some()
        && state.disk.esp_needs_format.is_some()
        && !state.disk.target_partitions.is_empty()
        && !state.disk.affected_disks.is_empty()
        && (state.disk.target_partitions.len() == 1 || state.disk.raid_mode.is_some())
}

fn summary_lines(state: &InstallerState) -> Vec<Line<'static>> {
    let mut lines = vec![
        summary_line(
            t!("confirm_step.locale"),
            value_or_unavailable(state.target_locale.as_deref()),
        ),
        summary_line(t!("confirm_step.locales"), enabled_locales_summary(state)),
        summary_line(
            t!("confirm_step.keyboard"),
            value_or_unavailable(state.keymap.as_deref()),
        ),
        summary_line(
            t!("confirm_step.kernel"),
            state
                .kernel
                .map(|kernel| kernel.package_name().to_string())
                .unwrap_or_else(|| t!("common.not_available")),
        ),
        summary_line(t!("confirm_step.nvidia"), nvidia_summary(state.nvidia)),
        summary_line(
            t!("confirm_step.hostname"),
            value_or_unavailable(state.hostname.as_deref()),
        ),
        summary_line(
            t!("confirm_step.timezone"),
            value_or_unavailable(state.timezone.as_deref()),
        ),
        summary_line(
            t!("confirm_step.username"),
            state
                .user
                .as_ref()
                .map(|user| user.username.clone())
                .filter(|username| !username.is_empty())
                .unwrap_or_else(|| t!("common.not_available")),
        ),
        Line::from(""),
    ];

    if state.disk.affected_disks.is_empty() {
        lines.push(summary_line(
            t!("confirm_step.affected_disk"),
            t!("common.not_available"),
        ));
    } else {
        for (index, disk) in state.disk.affected_disks.iter().enumerate() {
            lines.push(summary_line(
                format!("{} {}", t!("confirm_step.affected_disk"), index + 1),
                device_path(disk),
            ));
        }
    }

    lines.push(summary_line(
        t!("confirm_step.esp"),
        state
            .disk
            .esp_partition
            .as_deref()
            .map(device_path)
            .unwrap_or_else(|| t!("common.not_available")),
    ));

    if state.disk.target_partitions.is_empty() {
        lines.push(summary_line(
            t!("confirm_step.target_partition"),
            t!("common.not_available"),
        ));
    } else {
        for (index, partition) in state.disk.target_partitions.iter().enumerate() {
            lines.push(summary_line(
                format!("{} {}", t!("confirm_step.target_partition"), index + 1),
                device_path(partition),
            ));
        }
    }

    if state.disk.target_partitions.len() > 1 {
        let profile = match state.disk.raid_mode {
            Some(BtrfsRaidMode::Raid0) => "RAID0".to_string(),
            Some(BtrfsRaidMode::Raid1) => "RAID1".to_string(),
            None => t!("common.not_available"),
        };
        lines.push(summary_line(t!("confirm_step.data_profile"), profile));
    }

    lines
}

fn enabled_locales_summary(state: &InstallerState) -> String {
    if state.target_locales.is_empty() {
        return t!("common.not_available");
    }
    state.target_locales.join(", ")
}

fn summary_line(label: String, value: String) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{label}: "),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(value),
    ])
}

fn value_or_unavailable(value: Option<&str>) -> String {
    value
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| t!("common.not_available"))
}

fn nvidia_summary(choice: NvidiaChoice) -> String {
    choice
        .package_name()
        .map(str::to_string)
        .unwrap_or_else(|| t!("confirm_step.no_nvidia"))
}

fn device_path(name: &str) -> String {
    format!("/dev/{name}")
}

#[cfg(test)]
#[path = "confirm/tests.rs"]
mod tests;
