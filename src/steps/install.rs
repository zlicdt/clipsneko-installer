//! Final installation page: background execution, progress spinner, failure
//! log access, and the reboot decision.

use crate::installer::{
    run_install, unmount_and_reboot, InstallConfig, InstallProgress, WorkerMessage,
};
use crate::state::InstallerState;
use crate::steps::{Step, StepAction, StepId};
use crate::t;
use crate::util::ui::{centered_rect, render_autosized_dialog, rounded_block};
use anyhow::{Context, Result};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Gauge, Paragraph, Wrap};
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
    failure_message: String,
    log_text: String,
    log_scroll: u16,
    log_max_scroll: u16,
    /// Viewer size the current `log_max_scroll` was computed for. Wrapped
    /// line counting walks the whole log, so it only reruns on resize.
    log_layout_size: Option<(u16, u16)>,
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
            failure_message: String::new(),
            log_text: String::new(),
            log_scroll: 0,
            log_max_scroll: 0,
            log_layout_size: None,
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
                    tracing::error!(error = %format!("{error:#}"), "installation failed");
                    // `:#` includes the whole context chain, not just the
                    // outermost context, so the dialog names the root cause.
                    let _ = sender.send(WorkerMessage::Failed(format!("{error:#}")));
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
            InstallProgress::Packages { .. } => t!("install_step.progress.packages"),
            InstallProgress::Fstab => t!("install_step.progress.fstab"),
            InstallProgress::TargetConfig => t!("install_step.progress.target_config"),
            InstallProgress::Initramfs => t!("install_step.progress.initramfs"),
            InstallProgress::Bootloader => t!("install_step.progress.bootloader"),
            InstallProgress::Postinstall => t!("install_step.progress.postinstall"),
        }
    }

    fn render_status(&self, frame: &mut Frame, area: Rect) {
        let (text, percent) = match self.phase {
            Phase::Rebooting => (t!("install_step.progress.rebooting"), 100),
            _ => (self.progress_text(), self.progress.percent()),
        };
        let spinner = SPINNER[self.spinner % SPINNER.len()];
        let dialog_area = centered_rect(76, 8, area);
        let block = rounded_block();
        let inner = block.inner(dialog_area);
        frame.render_widget(block, dialog_area);
        let rows = Layout::vertical([
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);
        let heading = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                format!("{spinner} {text}"),
                Style::default().add_modifier(Modifier::BOLD),
            )),
        ])
        .alignment(Alignment::Center);
        frame.render_widget(heading, rows[0]);
        let gauge = Gauge::default()
            .gauge_style(Style::default().fg(Color::White).bg(Color::Black))
            .percent(percent)
            .label(format!("{percent}%"));
        frame.render_widget(gauge, rows[2]);
        let wait = Paragraph::new(Line::from(t!("install_step.progress.wait")))
            .alignment(Alignment::Center);
        frame.render_widget(wait, rows[4]);
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
        let mut lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                t!("install_step.failure.title"),
                Style::default().add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
        ];
        // The worker error can carry many lines of command stderr; the dialog
        // shows only the head as a summary and defers the rest to the log.
        const MAX_ERROR_LINES: usize = 4;
        let mut error_lines = self
            .failure_message
            .lines()
            .filter(|line| !line.trim().is_empty());
        let mut shown_any = false;
        for line in error_lines.by_ref().take(MAX_ERROR_LINES) {
            shown_any = true;
            lines.push(Line::from(Span::styled(
                line.to_string(),
                Style::default().fg(Color::Red),
            )));
        }
        if error_lines.next().is_some() {
            lines.push(Line::from("…"));
        }
        if shown_any {
            lines.push(Line::from(""));
        }
        lines.push(Line::from(t!("install_step.failure.body")));
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled(
                format!("[ {} ]", t!("install_step.failure.return")),
                return_style,
            ),
            Span::raw("    "),
            Span::styled(
                format!("[ {} ]", t!("install_step.failure.view_log")),
                log_style,
            ),
        ]));
        let dialog = Paragraph::new(lines)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true })
            .block(rounded_block());
        render_autosized_dialog(frame, area, 82, dialog);
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
        render_autosized_dialog(frame, area, 82, dialog);
    }

    /// Load this session's slice of the log file for the viewer. A read
    /// failure becomes viewer content instead of an error so the failure
    /// dialog (the only way out of this phase) is never lost to a crash.
    fn open_log(&mut self) {
        self.log_text = match Self::read_session_log() {
            Ok(text) => text,
            Err(error) => format!("{}\n{error:#}", t!("install_step.log.read_error")),
        };
        self.log_scroll = u16::MAX;
        self.log_layout_size = None;
        self.phase = Phase::LogView;
    }

    fn read_session_log() -> Result<String> {
        use std::io::{Read, Seek, SeekFrom};
        let path = crate::log_path()?;
        let mut file = std::fs::File::open(&path)
            .with_context(|| format!("opening installer log {}", path.display()))?;
        file.seek(SeekFrom::Start(crate::log_session_start()))
            .context("seeking installer log")?;
        let mut text = String::new();
        file.read_to_string(&mut text)
            .context("reading installer log")?;
        Ok(text)
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
                    self.failure_message = error;
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
                let log = Paragraph::new(self.log_text.as_str())
                    .wrap(Wrap { trim: false })
                    .block(rounded_block().title(t!("install_step.log.title")));
                // With wrap enabled the paragraph scrolls in *wrapped* lines,
                // so the limit must count wrapped lines too — `line_count`
                // wraps at the inner width and includes both border rows,
                // matching `area.height`.
                let inner_width = area.width.saturating_sub(2).max(1);
                if self.log_layout_size != Some((inner_width, area.height)) {
                    self.log_max_scroll =
                        log.line_count(inner_width)
                            .saturating_sub(usize::from(area.height))
                            .min(usize::from(u16::MAX)) as u16;
                    self.log_layout_size = Some((inner_width, area.height));
                }
                self.log_scroll = self.log_scroll.min(self.log_max_scroll);
                frame.render_widget(log.scroll((self.log_scroll, 0)), area);
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
                    self.open_log();
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
