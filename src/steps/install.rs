//! Final installation page: background execution, progress spinner, failure
//! log access, and the reboot decision.

use crate::installer::{
    run_install, unmount_and_reboot, InstallConfig, InstallProgress, WorkerMessage,
};
use crate::state::InstallerState;
use crate::steps::{Step, StepAction, StepId};
use crate::t;
use crate::util::ui::{centered_rect, rounded_block};
use anyhow::{Context, Result};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph, Wrap};
use ratatui::Frame;
use std::sync::mpsc::{self, Receiver, TryRecvError};

const SPINNER: [char; 4] = ['|', '/', '-', '\\'];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Idle,
    Running,
    Failed,
    LogView,
    RebootPrompt,
    Rebooting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FailureFocus {
    Return,
    ViewLog,
}

impl FailureFocus {
    fn toggle(self) -> Self {
        match self {
            Self::Return => Self::ViewLog,
            Self::ViewLog => Self::Return,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RebootFocus {
    Reboot,
    NotNow,
}

impl RebootFocus {
    fn toggle(self) -> Self {
        match self {
            Self::Reboot => Self::NotNow,
            Self::NotNow => Self::Reboot,
        }
    }
}

/// The terminal UI for the destructive worker. Navigation and global quitting
/// stay locked for the whole step; its own dialogs provide the safe exits.
pub struct InstallStep {
    phase: Phase,
    progress: InstallProgress,
    spinner: usize,
    receiver: Option<Receiver<WorkerMessage>>,
    failure_focus: FailureFocus,
    reboot_focus: RebootFocus,
    log_text: String,
    log_scroll: u16,
    log_max_scroll: u16,
}

impl InstallStep {
    pub fn new() -> Self {
        Self {
            phase: Phase::Idle,
            progress: InstallProgress::Formatting,
            spinner: 0,
            receiver: None,
            failure_focus: FailureFocus::Return,
            reboot_focus: RebootFocus::Reboot,
            log_text: String::new(),
            log_scroll: 0,
            log_max_scroll: 0,
        }
    }

    fn start_install(&mut self, state: &mut InstallerState) -> Result<()> {
        let config = InstallConfig::take_from_state(state)?;
        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new()
            .name("clipsneko-install".to_string())
            .spawn(move || match run_install(config, &sender) {
                Ok(()) => {
                    let _ = sender.send(WorkerMessage::Complete);
                }
                Err(error) => {
                    tracing::error!(error = %error, "installation failed");
                    let _ = sender.send(WorkerMessage::Failed(error.to_string()));
                }
            })
            .context("spawning installation worker")?;
        self.receiver = Some(receiver);
        self.phase = Phase::Running;
        Ok(())
    }

    fn start_reboot(&mut self) -> Result<()> {
        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new()
            .name("clipsneko-reboot".to_string())
            .spawn(move || match unmount_and_reboot() {
                Ok(()) => {
                    let _ = sender.send(WorkerMessage::RebootIssued);
                }
                Err(error) => {
                    tracing::error!(error = %error, "unmount or reboot failed");
                    let _ = sender.send(WorkerMessage::Failed(error.to_string()));
                }
            })
            .context("spawning reboot worker")?;
        self.receiver = Some(receiver);
        self.phase = Phase::Rebooting;
        Ok(())
    }

    fn progress_text(&self) -> String {
        match self.progress {
            InstallProgress::Formatting => t!("install_step.progress.formatting"),
            InstallProgress::Mounting => t!("install_step.progress.mounting"),
            InstallProgress::Packages => t!("install_step.progress.packages"),
            InstallProgress::Fstab => t!("install_step.progress.fstab"),
            InstallProgress::TargetConfig => t!("install_step.progress.target_config"),
            InstallProgress::Initramfs => t!("install_step.progress.initramfs"),
            InstallProgress::Bootloader => t!("install_step.progress.bootloader"),
            InstallProgress::Postinstall => t!("install_step.progress.postinstall"),
        }
    }

    fn render_status(&self, frame: &mut Frame, area: Rect) {
        let text = match self.phase {
            Phase::Rebooting => t!("install_step.progress.rebooting"),
            _ => self.progress_text(),
        };
        let spinner = SPINNER[self.spinner % SPINNER.len()];
        let body = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                format!("{spinner} {text}"),
                Style::default().add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(t!("install_step.progress.wait")),
        ])
        .alignment(Alignment::Center)
        .block(rounded_block());
        frame.render_widget(body, centered_rect(76, 8, area));
    }

    fn render_failure(&self, frame: &mut Frame, area: Rect) {
        let return_style = if self.failure_focus == FailureFocus::Return {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        let log_style = if self.failure_focus == FailureFocus::ViewLog {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        let dialog = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                t!("install_step.failure.title"),
                Style::default().add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(t!("install_step.failure.body")),
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    format!("[ {} ]", t!("install_step.failure.return")),
                    return_style,
                ),
                Span::raw("    "),
                Span::styled(
                    format!("[ {} ]", t!("install_step.failure.view_log")),
                    log_style,
                ),
            ]),
        ])
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true })
        .block(rounded_block());
        let dialog_area = centered_rect(82, 10, area);
        frame.render_widget(Clear, dialog_area);
        frame.render_widget(dialog, dialog_area);
    }

    fn render_reboot_prompt(&self, frame: &mut Frame, area: Rect) {
        let reboot_style = if self.reboot_focus == RebootFocus::Reboot {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        let later_style = if self.reboot_focus == RebootFocus::NotNow {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        let dialog = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                t!("install_step.success.title"),
                Style::default().add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(t!("install_step.success.body")),
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    format!("[ {} ]", t!("install_step.success.reboot")),
                    reboot_style,
                ),
                Span::raw("    "),
                Span::styled(
                    format!("[ {} ]", t!("install_step.success.not_now")),
                    later_style,
                ),
            ]),
        ])
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true })
        .block(rounded_block());
        let dialog_area = centered_rect(82, 10, area);
        frame.render_widget(Clear, dialog_area);
        frame.render_widget(dialog, dialog_area);
    }

    fn open_log(&mut self) -> Result<()> {
        self.log_text =
            std::fs::read_to_string(crate::log_path()?).context("reading installer log")?;
        self.log_scroll = u16::MAX;
        self.phase = Phase::LogView;
        Ok(())
    }

    fn receive_worker_messages(&mut self) -> StepAction {
        let Some(receiver) = self.receiver.as_ref() else {
            return StepAction::None;
        };
        let mut messages = Vec::new();
        loop {
            match receiver.try_recv() {
                Ok(message) => messages.push(message),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break,
            }
        }
        let mut action = StepAction::None;
        for message in messages {
            match message {
                WorkerMessage::Progress(progress) => self.progress = progress,
                WorkerMessage::Complete => {
                    self.receiver = None;
                    self.reboot_focus = RebootFocus::Reboot;
                    self.phase = Phase::RebootPrompt;
                }
                WorkerMessage::Failed(error) => {
                    tracing::error!(error = %error, "installation worker reported failure");
                    self.receiver = None;
                    self.failure_focus = FailureFocus::Return;
                    self.phase = Phase::Failed;
                }
                WorkerMessage::RebootIssued => {
                    self.receiver = None;
                    action = StepAction::Quit;
                }
            }
        }
        action
    }
}

impl Step for InstallStep {
    fn id(&self) -> StepId {
        StepId::Install
    }

    fn activate(&mut self, state: &mut InstallerState) -> Result<()> {
        if self.phase == Phase::Idle {
            self.start_install(state)?;
        }
        Ok(())
    }

    fn render(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        _state: &InstallerState,
        _body_focused: bool,
    ) {
        match self.phase {
            Phase::Running | Phase::Rebooting | Phase::Idle => self.render_status(frame, area),
            Phase::Failed => self.render_failure(frame, area),
            Phase::RebootPrompt => self.render_reboot_prompt(frame, area),
            Phase::LogView => {
                let visible_height = area.height.saturating_sub(2) as usize;
                let line_count = self.log_text.lines().count();
                self.log_max_scroll = line_count
                    .saturating_sub(visible_height)
                    .min(u16::MAX as usize) as u16;
                self.log_scroll = self.log_scroll.min(self.log_max_scroll);
                let log = Paragraph::new(self.log_text.as_str())
                    .scroll((self.log_scroll, 0))
                    .wrap(Wrap { trim: false })
                    .block(rounded_block().title(t!("install_step.log.title")));
                frame.render_widget(log, area);
            }
        }
    }

    fn handle_key(&mut self, key: KeyEvent, _state: &mut InstallerState) -> Result<StepAction> {
        if key.kind != KeyEventKind::Press {
            return Ok(StepAction::None);
        }
        match self.phase {
            Phase::Failed => match key.code {
                KeyCode::Left | KeyCode::Right | KeyCode::Tab | KeyCode::BackTab => {
                    self.failure_focus = self.failure_focus.toggle();
                    Ok(StepAction::None)
                }
                KeyCode::Enter if self.failure_focus == FailureFocus::Return => {
                    Ok(StepAction::Quit)
                }
                KeyCode::Enter => {
                    self.open_log()?;
                    Ok(StepAction::None)
                }
                _ => Ok(StepAction::None),
            },
            Phase::RebootPrompt => match key.code {
                KeyCode::Left | KeyCode::Right | KeyCode::Tab | KeyCode::BackTab => {
                    self.reboot_focus = self.reboot_focus.toggle();
                    Ok(StepAction::None)
                }
                KeyCode::Enter if self.reboot_focus == RebootFocus::Reboot => {
                    self.start_reboot()?;
                    Ok(StepAction::None)
                }
                KeyCode::Enter => Ok(StepAction::Quit),
                _ => Ok(StepAction::None),
            },
            Phase::LogView => {
                match key.code {
                    KeyCode::Esc | KeyCode::Enter => self.phase = Phase::Failed,
                    KeyCode::Down | KeyCode::Char('j') => {
                        self.log_scroll = self.log_scroll.saturating_add(1).min(self.log_max_scroll)
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        self.log_scroll = self.log_scroll.saturating_sub(1)
                    }
                    KeyCode::PageDown => {
                        self.log_scroll =
                            self.log_scroll.saturating_add(10).min(self.log_max_scroll)
                    }
                    KeyCode::PageUp => self.log_scroll = self.log_scroll.saturating_sub(10),
                    KeyCode::Home => self.log_scroll = 0,
                    KeyCode::End => self.log_scroll = self.log_max_scroll,
                    _ => {}
                }
                Ok(StepAction::None)
            }
            Phase::Idle | Phase::Running | Phase::Rebooting => Ok(StepAction::None),
        }
    }

    fn tick(&mut self, _state: &mut InstallerState) -> Result<StepAction> {
        if matches!(self.phase, Phase::Running | Phase::Rebooting) {
            self.spinner = self.spinner.wrapping_add(1);
        }
        Ok(self.receive_worker_messages())
    }

    fn is_complete(&self, _state: &InstallerState) -> bool {
        false
    }

    fn has_modal(&self) -> bool {
        matches!(
            self.phase,
            Phase::Failed | Phase::LogView | Phase::RebootPrompt
        )
    }

    fn allows_back(&self) -> bool {
        false
    }

    fn blocks_global_quit(&self) -> bool {
        true
    }

    fn shows_navigation_footer(&self) -> bool {
        false
    }

    fn on_back_button(&mut self, _state: &mut InstallerState) -> Result<StepAction> {
        Ok(StepAction::None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn failure_defaults_to_return_and_log_returns_to_failure_dialog() {
        let mut step = InstallStep::new();
        let mut state = InstallerState::default();
        step.phase = Phase::Failed;
        assert_eq!(step.failure_focus, FailureFocus::Return);
        step.phase = Phase::LogView;
        step.handle_key(key(KeyCode::Esc), &mut state).unwrap();
        assert_eq!(step.phase, Phase::Failed);
    }

    #[test]
    fn reboot_prompt_defaults_to_reboot_and_not_now_quits() {
        let mut step = InstallStep::new();
        let mut state = InstallerState::default();
        step.phase = Phase::RebootPrompt;
        assert_eq!(step.reboot_focus, RebootFocus::Reboot);
        step.handle_key(key(KeyCode::Right), &mut state).unwrap();
        assert!(matches!(
            step.handle_key(key(KeyCode::Enter), &mut state).unwrap(),
            StepAction::Quit
        ));
    }
}
