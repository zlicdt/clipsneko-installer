//! Network configuration step — verify connectivity and launch `nmtui`.
//!
//! Interaction (per `design.md` §4 step 3): on entry, run a connectivity
//! check (`curl --max-time 5 -sI http://ip-api.com/json`); if online the
//! Next button is enabled immediately. If offline, the user presses Enter
//! to launch `nmtui` full-screen (ratatui is suspended via
//! `util::process::run_fullscreen`); after `nmtui` exits, connectivity is
//! re-checked automatically. The user can also press `R` to re-check
//! without launching `nmtui`, or `N` to launch `nmtui` even when already
//! connected. Esc is handled by `app.rs` as Back.
//!
//! The check runs on a background worker thread (the `curl` timeout would
//! otherwise freeze the TUI for up to 5 seconds). While it is in flight a
//! centered modal loading dialog with a spinner overlays the step and
//! swallows all keys; `Step::tick()` collects the worker result. Entry,
//! `R`, and the post-`nmtui` re-check all share this path.
//!
//! The Next button is disabled (greyed out, not focusable) until
//! `state.network_ok` is true. This is enforced via `Step::is_complete()`,
//! which `app.rs` consults in `next_enabled()`.
//!
//! `nmtui` uses polkit and does not need `sudo`; the connectivity `curl`
//! and the info commands (`hostname -I`, `ip route`) are plain user
//! commands. None of them go through `privileged_command` (see
//! `design.md` §9).

use crate::state::InstallerState;
use crate::steps::{Step, StepAction, StepId};
use crate::t;
use crate::util::ui::{focusable_block, render_loading_dialog, rounded_block};
use anyhow::{bail, Context, Result};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;
use std::process::Command;
use std::sync::mpsc::{self, Receiver, TryRecvError};

/// Outcome of one background connectivity check, sent over the channel by
/// the worker thread.
struct RecheckResult {
    connected: bool,
    ips: Vec<String>,
    /// `(gateway, interface)` of the default route, if one exists.
    route: Option<(String, String)>,
}

pub struct NetworkStep {
    /// Local IP addresses from `hostname -I` (last check).
    ips: Vec<String>,
    /// Default gateway from `ip route show default` (last check).
    gateway: Option<String>,
    /// Network interface of the default route (last check).
    interface: Option<String>,
    /// Whether a background connectivity check is in flight.
    checking: bool,
    /// Spinner frame counter, advanced by `tick` while checking.
    spinner: usize,
    /// Receives the worker's result; `Some` only while `checking`.
    receiver: Option<Receiver<Result<RecheckResult, String>>>,
}

impl NetworkStep {
    pub fn new() -> Self {
        Self {
            ips: Vec::new(),
            gateway: None,
            interface: None,
            checking: false,
            spinner: 0,
            receiver: None,
        }
    }

    /// Spawn the background connectivity check and show the modal loading
    /// dialog until `tick()` collects its result. Called on entry
    /// (`activate`), after `nmtui` returns (`on_subprocess_done`), and when
    /// the user presses `R`.
    fn start_recheck(&mut self) -> Result<()> {
        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new()
            .name("clipsneko-network-check".to_string())
            .spawn(move || {
                let result = run_check().map_err(|e| format!("{e:#}"));
                let _ = sender.send(result);
            })
            .context("spawning network check worker")?;
        self.receiver = Some(receiver);
        self.checking = true;
        Ok(())
    }
}

impl Step for NetworkStep {
    fn id(&self) -> StepId {
        StepId::Network
    }

    fn activate(&mut self, _state: &mut InstallerState) -> Result<()> {
        self.start_recheck()
    }

    fn is_complete(&self, state: &InstallerState) -> bool {
        state.network_ok
    }

    fn has_modal(&self) -> bool {
        self.checking
    }

    fn tick(&mut self, state: &mut InstallerState) -> Result<StepAction> {
        if !self.checking {
            return Ok(StepAction::None);
        }
        self.spinner = self.spinner.wrapping_add(1);
        let received = self.receiver.as_ref().map(|r| r.try_recv());
        match received {
            Some(Ok(Ok(result))) => {
                self.checking = false;
                self.receiver = None;
                state.network_ok = result.connected;
                self.ips = result.ips;
                match result.route {
                    Some((gateway, interface)) => {
                        self.gateway = Some(gateway);
                        self.interface = Some(interface);
                    }
                    None => {
                        self.gateway = None;
                        self.interface = None;
                    }
                }
                tracing::info!(
                    "network recheck: connected={}, ips={:?}, gateway={:?}, interface={:?}",
                    state.network_ok,
                    self.ips,
                    self.gateway,
                    self.interface
                );
                Ok(StepAction::None)
            }
            Some(Ok(Err(msg))) => {
                self.checking = false;
                self.receiver = None;
                Err(anyhow::anyhow!(msg))
            }
            Some(Err(TryRecvError::Empty)) => Ok(StepAction::None),
            Some(Err(TryRecvError::Disconnected)) | None => {
                self.checking = false;
                self.receiver = None;
                Err(anyhow::anyhow!(
                    "network check worker terminated unexpectedly"
                ))
            }
        }
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

        let connected = state.network_ok;
        let hint = if connected {
            t!("network_step.hint_connected")
        } else {
            t!("network_step.hint_disconnected")
        };

        let mut lines: Vec<Line> = Vec::new();
        if connected {
            let not_available = t!("common.not_available");
            lines.push(Line::from(t!("network_step.status_connected")));
            lines.push(Line::from(""));
            lines.push(Line::from(format!(
                "{}:  {}",
                t!("network_step.label_interface"),
                self.interface.as_deref().unwrap_or(&not_available)
            )));
            let ip_str = if self.ips.is_empty() {
                not_available.clone()
            } else {
                self.ips.join(", ")
            };
            lines.push(Line::from(format!(
                "{}:  {}",
                t!("network_step.label_address"),
                ip_str
            )));
            lines.push(Line::from(format!(
                "{}:  {}",
                t!("network_step.label_gateway"),
                self.gateway.as_deref().unwrap_or(&not_available)
            )));
        } else {
            lines.push(Line::from(t!("network_step.status_disconnected")));
        }

        let block = focusable_block(
            rounded_block().title(t!("network_step.title")),
            body_focused,
        );
        // The address line joins every IP on the interface and can exceed
        // the terminal width.
        frame.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .block(block),
            chunks[0],
        );

        frame.render_widget(
            Paragraph::new(hint)
                .alignment(Alignment::Center)
                .style(Style::default().add_modifier(Modifier::DIM)),
            chunks[1],
        );

        if self.checking {
            render_loading_dialog(frame, &t!("network_step.checking"), self.spinner);
        }
    }

    fn handle_key(&mut self, key: KeyEvent, state: &mut InstallerState) -> Result<StepAction> {
        if key.kind != KeyEventKind::Press {
            return Ok(StepAction::None);
        }

        // The loading dialog is modal: swallow every key while a check runs.
        if self.checking {
            return Ok(StepAction::None);
        }

        let connected = state.network_ok;

        Ok(match key.code {
            KeyCode::Enter => {
                if connected {
                    StepAction::Next
                } else {
                    StepAction::SuspendRun("nmtui".to_string(), Vec::new())
                }
            }
            KeyCode::Char('n') | KeyCode::Char('N') => {
                StepAction::SuspendRun("nmtui".to_string(), Vec::new())
            }
            KeyCode::Char('r') | KeyCode::Char('R') => {
                self.start_recheck()?;
                StepAction::None
            }
            // Esc is handled by app.rs as Back.
            _ => StepAction::None,
        })
    }

    fn on_subprocess_done(
        &mut self,
        _status: std::process::ExitStatus,
        _state: &mut InstallerState,
    ) -> Result<()> {
        self.start_recheck()
    }
}

// --- pure-logic helpers (unit-tested) ---

/// Parse `hostname -I` output: whitespace-separated IP addresses on one line.
/// Returns the addresses in order, trimming whitespace and dropping empties.
fn parse_hostname_i(stdout: &str) -> Vec<String> {
    stdout.split_whitespace().map(String::from).collect()
}

/// Parse `ip route show default` output and extract `(gateway, interface)`.
/// Expects a line like `default via 192.168.1.1 dev eth0 proto dhcp ...`.
/// Returns `None` if the `via` or `dev` field is missing. Only the first
/// line is examined.
fn parse_default_route(stdout: &str) -> Option<(String, String)> {
    let line = stdout.lines().next()?;
    let mut gateway = None;
    let mut interface = None;
    let mut tokens = line.split_whitespace();
    while let Some(tok) = tokens.next() {
        match tok {
            "via" => {
                gateway = tokens.next().map(String::from);
            }
            "dev" => {
                interface = tokens.next().map(String::from);
            }
            _ => {}
        }
    }
    match (gateway, interface) {
        (Some(g), Some(i)) => Some((g, i)),
        _ => None,
    }
}

// --- subprocess wrappers (not unit-tested; thin shells around Command) ---

/// Run one full connectivity check (probe + detail gathering) on the
/// background worker thread. Blocking: `curl` may take up to 5 seconds on
/// timeout, which is why this never runs on the UI thread.
fn run_check() -> Result<RecheckResult> {
    let connected = check_connectivity()?;
    let ips = gather_local_ips()?;
    let route = gather_default_route()?;
    Ok(RecheckResult {
        connected,
        ips,
        route,
    })
}

/// Run `curl --max-time 5 -sI http://ip-api.com/json` and return `true` if
/// the exit status is success (0). A timeout or any non-zero exit means
/// not connected. Uses `.output()` (not `.status()`) so curl's stdout/stderr
/// are captured to memory instead of inheriting the parent terminal —
/// printing HTTP headers into ratatui's raw-mode session would corrupt the
/// screen.
fn check_connectivity() -> Result<bool> {
    let output = Command::new("curl")
        .args(["--max-time", "5", "-sI", "http://ip-api.com/json"])
        .output()
        .context("running curl connectivity check")?;
    Ok(output.status.success())
}

/// Run `hostname -i` and parse the output into a list of IP address strings.
/// Uses the lowercase `-i` (`--ip-addresses`) flag supported by GNU
/// inetutils' `hostname` (the version shipped on the ClipsNeko ISO); the
/// uppercase `-I` is a busybox/hostnamectl extension and is not available
/// there.
fn gather_local_ips() -> Result<Vec<String>> {
    let output = Command::new("hostname")
        .arg("-i")
        .output()
        .context("running hostname -i")?;
    if !output.status.success() {
        bail!(
            "hostname -i failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(parse_hostname_i(&String::from_utf8_lossy(&output.stdout)))
}

/// Run `ip route show default` and parse the gateway + interface. Returns
/// `None` if there is no default route or parsing fails.
fn gather_default_route() -> Result<Option<(String, String)>> {
    let output = Command::new("ip")
        .args(["route", "show", "default"])
        .output()
        .context("running ip route show default")?;
    if !output.status.success() {
        bail!(
            "ip route show default failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(parse_default_route(&String::from_utf8_lossy(
        &output.stdout,
    )))
}

#[cfg(test)]
mod tests;
