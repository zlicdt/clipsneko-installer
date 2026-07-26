//! Mirror list selection step — pick a region or enter a custom `Server =`
//! line, then validate via `pacman -Sy`.
//!
//! Interaction (per `design.md` §4 step 4, revised to drop reflector):
//! the step reads `/etc/pacman.d/mirrorlist` at entry and parses it into
//! region blocks (a `## <Region>` header followed by its `Server =` lines).
//! The body shows a single-select list of region names (Up/Down/j/k) and,
//! below it, a one-line input field for a manual `Server = ...` URL. Tab
//! toggles focus between the list and the input field.
//!
//! On Next:
//! - If the input field is non-empty, it is validated as a `Server =` line
//!   and used as the sole mirror (appended to the top of the file).
//! - Otherwise the selected region's `Server =` lines are moved to the top
//!   of `/etc/pacman.d/mirrorlist`, ahead of all other regions. The file
//!   header comments are preserved.
//! - The rewritten mirrorlist is written back and `pacman -Sy` is run on a
//!   background worker thread (`.output()` so its output never reaches the
//!   ratatui terminal) — syncing databases can take many seconds and must
//!   not freeze the TUI. While the worker runs, a centered modal loading
//!   dialog with a spinner overlays the step and swallows all keys;
//!   `Step::tick()` collects the outcome. Exit 0 → `state.mirror_lines`
//!   recorded, advance. Non-zero → a modal error dialog shows the failure;
//!   the user dismisses it and retries.
//!
//! Esc is handled by `app.rs` as Back unless the step's error modal is open
//! or a validation is in flight.

use crate::state::InstallerState;
use crate::steps::{Step, StepAction, StepId};
use crate::t;
use crate::util::process::privileged_command;
use crate::util::ui::{
    focusable_block, render_autosized_dialog, render_loading_dialog, rounded_block,
};
use anyhow::{bail, Context, Result};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;
use std::sync::mpsc::{self, Receiver, TryRecvError};

/// Path to the pacman mirrorlist. On the ClipsNeko ISO this is always
/// present and well-formed (the ISO build ships a full mirrorlist); the
/// step assumes it exists.
const MIRRORLIST_PATH: &str = "/etc/pacman.d/mirrorlist";

/// Which sub-widget within the mirror step body has focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MirrorFocus {
    List,
    Input,
}

/// Validation/error dialog state. `None` = no dialog; `Some` = show the
/// dialog with the given message (Esc/Enter dismisses).
#[derive(Debug, Default, Clone)]
struct ErrorDialog {
    visible: bool,
    message: String,
}

pub struct MirrorStep {
    /// Parsed region names in file order. `Worldwide` is a region like any
    /// other — it is just the first block in the stock mirrorlist.
    regions: Vec<String>,
    /// The full original mirrorlist text, kept so reordering can re-emit
    /// the exact lines without re-reading the file.
    raw: String,
    /// List cursor (highlighted region).
    list_state: ListState,
    /// Currently applied (▶) region. Empty until the user confirms one via
    /// Next, or equals the input-field value when a manual source is used.
    selected: String,
    /// Manual input buffer.
    input: String,
    /// Focus within the step body: list vs. input field.
    focus: MirrorFocus,
    /// Error dialog state.
    error: ErrorDialog,
    /// Whether a background `pacman -Sy` validation is in flight.
    validating: bool,
    /// Spinner frame counter, advanced by `tick` while validating.
    spinner: usize,
    /// Receives the validation worker's outcome; `Some` only while
    /// `validating`. `Ok(true)` = `pacman -Sy` exit 0, `Ok(false)` =
    /// non-zero exit (recoverable), `Err` = fatal write/spawn failure.
    receiver: Option<Receiver<Result<bool, String>>>,
    /// Mirror lines recorded into shared state on validation success.
    pending_lines: Vec<String>,
    /// Label applied to the list highlight on validation success.
    pending_selected: String,
}

impl MirrorStep {
    pub fn new() -> Result<Self> {
        let raw = std::fs::read_to_string(MIRRORLIST_PATH)
            .with_context(|| format!("reading {MIRRORLIST_PATH}"))?;
        let regions = parse_mirrorlist_regions(&raw);
        if regions.is_empty() {
            bail!("{MIRRORLIST_PATH} contains no region blocks");
        }
        let mut list_state = ListState::default();
        list_state.select(Some(0));
        Ok(Self {
            regions,
            raw,
            list_state,
            selected: String::new(),
            input: String::new(),
            focus: MirrorFocus::List,
            error: ErrorDialog::default(),
            validating: false,
            spinner: 0,
            receiver: None,
            pending_lines: Vec::new(),
            pending_selected: String::new(),
        })
    }

    /// The region under the current highlight cursor.
    fn highlighted(&self) -> &str {
        let idx = self.list_state.selected().unwrap_or(0);
        &self.regions[idx]
    }

    /// Move the highlight by `delta` positions, wrapping around.
    fn move_highlight(&mut self, delta: i32) {
        let len = self.regions.len() as i32;
        if len == 0 {
            return;
        }
        let cur = self.list_state.selected().unwrap_or(0) as i32;
        let next = (cur + delta).rem_euclid(len) as usize;
        self.list_state.select(Some(next));
    }

    /// Start the validation flow: rewrite the mirrorlist for the given
    /// selection and run `pacman -Sy` on a background worker thread so the
    /// TUI stays responsive. The modal loading dialog is shown until
    /// `tick()` collects the outcome: success records the choice and
    /// advances, a non-zero exit surfaces a modal error. Always returns
    /// `None` — the actual advance happens in `tick()`.
    fn validate_and_advance(&mut self, _state: &mut InstallerState) -> Result<StepAction> {
        let input_trimmed = self.input.trim().to_string();
        let selection = if !input_trimmed.is_empty() {
            match normalize_server_line(&input_trimmed) {
                Some(line) => Selection::Manual(line),
                None => {
                    self.error = ErrorDialog {
                        visible: true,
                        message: t!("mirror_step.error_invalid_url"),
                    };
                    return Ok(StepAction::None);
                }
            }
        } else {
            Selection::Region(self.highlighted().to_string())
        };

        let new_text = match &selection {
            Selection::Region(region) => reorder_mirrorlist(&self.raw, region),
            Selection::Manual(line) => manual_mirrorlist(&self.raw, line),
        };

        self.pending_lines = match &selection {
            Selection::Region(region) => extract_region_servers(&self.raw, region),
            Selection::Manual(line) => vec![line.clone()],
        };
        self.pending_selected = match &selection {
            Selection::Region(r) => r.clone(),
            Selection::Manual(l) => l.clone(),
        };

        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new()
            .name("clipsneko-mirror-validate".to_string())
            .spawn(move || {
                let result = run_validation(&new_text).map_err(|e| format!("{e:#}"));
                let _ = sender.send(result);
            })
            .context("spawning mirror validation worker")?;
        self.receiver = Some(receiver);
        self.validating = true;
        Ok(StepAction::None)
    }
}

/// What the user picked on the mirror step.
enum Selection {
    Region(String),
    Manual(String),
}

impl Step for MirrorStep {
    fn id(&self) -> StepId {
        StepId::Mirror
    }

    fn has_modal(&self) -> bool {
        self.error.visible || self.validating
    }

    fn tick(&mut self, state: &mut InstallerState) -> Result<StepAction> {
        if !self.validating {
            return Ok(StepAction::None);
        }
        self.spinner = self.spinner.wrapping_add(1);
        let received = self.receiver.as_ref().map(|r| r.try_recv());
        match received {
            Some(Ok(Ok(true))) => {
                self.validating = false;
                self.receiver = None;
                state.mirror_lines = std::mem::take(&mut self.pending_lines);
                self.selected = std::mem::take(&mut self.pending_selected);
                Ok(StepAction::Next)
            }
            Some(Ok(Ok(false))) => {
                self.validating = false;
                self.receiver = None;
                self.error = ErrorDialog {
                    visible: true,
                    message: t!("mirror_step.error_pacman"),
                };
                Ok(StepAction::None)
            }
            Some(Ok(Err(msg))) => {
                self.validating = false;
                self.receiver = None;
                Err(anyhow::anyhow!(msg))
            }
            Some(Err(TryRecvError::Empty)) => Ok(StepAction::None),
            Some(Err(TryRecvError::Disconnected)) | None => {
                self.validating = false;
                self.receiver = None;
                Err(anyhow::anyhow!(
                    "mirror validation worker terminated unexpectedly"
                ))
            }
        }
    }

    fn render(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        _state: &InstallerState,
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

        // Region list (top).
        let items: Vec<ListItem> = self
            .regions
            .iter()
            .map(|r| {
                // The currently-applied region (▶ in the old style) is now
                // bold + a bright color. The cursor row is separately
                // indicated by the `REVERSED` highlight style.
                let style = if *r == self.selected {
                    Style::default().add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                ListItem::new(r.clone()).style(style)
            })
            .collect();
        let list_highlight = if body_focused && self.focus == MirrorFocus::List {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        let list = List::new(items)
            .block(focusable_block(
                rounded_block().title(t!("mirror_step.list_title")),
                body_focused && self.focus == MirrorFocus::List,
            ))
            .highlight_style(list_highlight);
        frame.render_stateful_widget(list, chunks[0], &mut self.list_state);

        // Manual input field (bottom, above the hint): a single-line bordered box.
        // When focused, show a block cursor at the end of the text instead of
        // reversing the whole box — a single visible caret is clearer than
        // inverting the entire field.
        let input_label = t!("mirror_step.input_label");
        let cursor = if body_focused && self.focus == MirrorFocus::Input {
            "█"
        } else {
            ""
        };
        let input_display = format!("{input_label}: {}{cursor}", self.input);
        let input_width = ratatui::text::Line::from(input_display.as_str()).width();
        let visible_width = chunks[1].width.saturating_sub(2) as usize;
        let horizontal_scroll = input_width.saturating_sub(visible_width) as u16;
        let input_focused = body_focused && self.focus == MirrorFocus::Input;
        let input_box = Paragraph::new(input_display)
            .block(focusable_block(
                rounded_block().title(t!("mirror_step.input_title")),
                input_focused,
            ))
            .scroll((0, horizontal_scroll));
        frame.render_widget(input_box, chunks[1]);

        // Bottom hint.
        let hint = if self.focus == MirrorFocus::List {
            t!("mirror_step.hint_list")
        } else {
            t!("mirror_step.hint_input")
        };
        frame.render_widget(
            Paragraph::new(hint)
                .alignment(Alignment::Center)
                .style(Style::default().add_modifier(Modifier::DIM)),
            chunks[2],
        );

        if self.error.visible {
            self.render_error_dialog(frame);
        }

        if self.validating {
            render_loading_dialog(frame, &t!("mirror_step.validating"), self.spinner);
        }
    }

    fn handle_key(&mut self, key: KeyEvent, state: &mut InstallerState) -> Result<StepAction> {
        if key.kind != KeyEventKind::Press {
            return Ok(StepAction::None);
        }

        // The loading dialog is modal: swallow every key while a
        // validation worker is running.
        if self.validating {
            return Ok(StepAction::None);
        }

        // Error dialog swallows all keys except Esc/Enter which dismiss it.
        if self.error.visible {
            match key.code {
                KeyCode::Esc | KeyCode::Enter => {
                    self.error.visible = false;
                }
                _ => {}
            }
            return Ok(StepAction::None);
        }

        // Tab/BackTab are routed through `consume_tab` by app.rs, not here.
        if key.code == KeyCode::Tab || key.code == KeyCode::BackTab {
            return Ok(StepAction::None);
        }

        Ok(match self.focus {
            MirrorFocus::List => match key.code {
                KeyCode::Down | KeyCode::Char('j') => {
                    self.move_highlight(1);
                    StepAction::None
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.move_highlight(-1);
                    StepAction::None
                }
                KeyCode::Enter => return self.validate_and_advance(state),
                _ => StepAction::None,
            },
            MirrorFocus::Input => match key.code {
                KeyCode::Enter => return self.validate_and_advance(state),
                KeyCode::Backspace => {
                    self.input.pop();
                    StepAction::None
                }
                KeyCode::Char(c) => {
                    self.input.push(c);
                    StepAction::None
                }
                _ => StepAction::None,
            },
        })
    }

    fn consume_tab(&mut self, is_shift: bool) -> bool {
        // The complete forward chain is List -> Input -> Back -> Next -> List.
        // Before bubbling out at either end, prepare the opposite endpoint so
        // re-entering the step body from the footer completes the cycle.
        match (self.focus, is_shift) {
            (MirrorFocus::List, false) => {
                // Tab from List -> Input (consumed).
                self.focus = MirrorFocus::Input;
                true
            }
            (MirrorFocus::Input, false) => {
                // Tab from Input -> footer; re-entry starts at List.
                self.focus = MirrorFocus::List;
                false
            }
            (MirrorFocus::Input, true) => {
                // BackTab from Input -> List (consumed).
                self.focus = MirrorFocus::List;
                true
            }
            (MirrorFocus::List, true) => {
                // BackTab from List -> footer; reverse re-entry starts at Input.
                self.focus = MirrorFocus::Input;
                false
            }
        }
    }

    fn on_next_button(&mut self, state: &mut InstallerState) -> Result<StepAction> {
        self.validate_and_advance(state)
    }
}

impl MirrorStep {
    fn render_error_dialog(&self, frame: &mut Frame) {
        let text = vec![
            ratatui::text::Line::from(""),
            ratatui::text::Line::from(self.error.message.clone()),
            ratatui::text::Line::from(""),
            ratatui::text::Line::from(t!("mirror_step.error_hint")),
        ];
        let dialog = Paragraph::new(text)
            .block(rounded_block().title(t!("mirror_step.error_title")))
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true });
        render_autosized_dialog(frame, frame.area(), 80, dialog);
    }
}

// --- pure-logic helpers (unit-tested) ---

/// Parse a mirrorlist into region names, in file order. A region header is
/// a line starting with `## ` that is NOT one of the two stock file-header
/// lines (`Arch Linux repository mirrorlist` / `Generated on ...`). Lines
/// starting with `#` (single) or `Server =` are not regions.
fn parse_mirrorlist_regions(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(name) = trimmed.strip_prefix("## ") {
            // Skip the two stock header comments.
            if name.starts_with("Arch Linux repository mirrorlist")
                || name.starts_with("Generated on")
            {
                continue;
            }
            out.push(name.trim().to_string());
        }
    }
    out
}

/// Reorder `text` so the `region` block (its `## <Region>` header plus all
/// following `Server =` lines up to the next `## ` header) is moved to the
/// top, immediately after the file header comments. All other regions keep
/// their original relative order. The file header comments (`## Arch Linux
/// ...` / `## Generated on ...`) stay at the very top.
fn reorder_mirrorlist(text: &str, region: &str) -> String {
    let (header, body) = split_header(text);
    let blocks = split_blocks(&body);
    let (chosen, rest): (Vec<&MirrorBlock>, Vec<&MirrorBlock>) = blocks
        .iter()
        .partition(|b| b.region.as_deref() == Some(region));
    let mut out = header;
    if let Some(b) = chosen.first() {
        out.push_str(&b.text);
    }
    for b in &rest {
        out.push_str(&b.text);
    }
    out
}

/// Build a mirrorlist containing only the stock file header and one manual
/// server. Keeping old active servers here would let `pacman -Sy` fall back to
/// them and incorrectly validate a broken manual URL.
fn manual_mirrorlist(text: &str, server_line: &str) -> String {
    let (mut header, _body) = split_header(text);
    if !header.ends_with('\n') {
        header.push('\n');
    }
    header.push_str(server_line);
    header.push('\n');
    header
}

/// Extract all `Server = ...` lines belonging to `region` from `text`.
fn extract_region_servers(text: &str, region: &str) -> Vec<String> {
    let (_header, body) = split_header(text);
    let blocks = split_blocks(&body);
    blocks
        .into_iter()
        .find(|b| b.region.as_deref() == Some(region))
        .map(|b| b.servers)
        .unwrap_or_default()
}

/// A parsed region block: its name (if the header is `## <Region>`), the
/// raw text (header + server lines + trailing newline), and the extracted
/// `Server = ...` lines.
struct MirrorBlock {
    region: Option<String>,
    servers: Vec<String>,
    text: String,
}
/// Split `text` into `(header, body)` where `header` is everything up to
/// (but not including) the first real region header (`## <Region>` that is
/// NOT `Arch Linux repository mirrorlist` or `Generated on ...`). `body`
/// starts at that first region header.
fn split_header(text: &str) -> (String, String) {
    let lines: Vec<&str> = text.lines().collect();
    let mut split = lines.len();
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim();
        if let Some(name) = t.strip_prefix("## ") {
            if !name.starts_with("Arch Linux repository mirrorlist")
                && !name.starts_with("Generated on")
            {
                split = i;
                break;
            }
        }
    }
    let header = lines[..split].join("\n");
    let body = lines[split..].join("\n");
    (header + "\n", body)
}

/// Split `body` into region blocks. Each block starts at a `## <Region>`
/// line and includes all lines until the next `## ` line (or EOF).
fn split_blocks(body: &str) -> Vec<MirrorBlock> {
    let mut blocks = Vec::new();
    let mut current: Option<(Option<String>, Vec<String>)> = None;
    for line in body.lines() {
        let t = line.trim();
        if let Some(name) = t.strip_prefix("## ") {
            if let Some((region, lines)) = current.take() {
                let text = lines.join("\n") + "\n";
                let servers = lines
                    .into_iter()
                    .filter(|l| l.trim().starts_with("Server = "))
                    .collect();
                blocks.push(MirrorBlock {
                    region,
                    servers,
                    text,
                });
            }
            current = Some((Some(name.trim().to_string()), vec![line.to_string()]));
        } else if let Some((_, lines)) = current.as_mut() {
            lines.push(line.to_string());
        }
    }
    if let Some((region, lines)) = current {
        let text = lines.join("\n") + "\n";
        let servers = lines
            .into_iter()
            .filter(|l| l.trim().starts_with("Server = "))
            .collect();
        blocks.push(MirrorBlock {
            region,
            servers,
            text,
        });
    }
    blocks
}

/// Validate the structure of a manual `Server =` line. Accepts a line with
/// or without the leading `Server = ` prefix (the step adds it if absent).
/// Returns the normalized `Server = <url>` string if valid, `None` otherwise.
fn normalize_server_line(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    let line = if let Some(url) = trimmed.strip_prefix("Server = ") {
        url.trim().to_string()
    } else if let Some(url) = trimmed.strip_prefix("Server=") {
        url.trim().to_string()
    } else {
        trimmed.to_string()
    };
    if line.is_empty() {
        return None;
    }
    if !(line.starts_with("http://")
        || line.starts_with("https://")
        || line.starts_with("ftp://")
        || line.starts_with("rsync://"))
    {
        return None;
    }
    Some(format!("Server = {line}"))
}

/// Run the mirrorlist rewrite + `pacman -Sy` validation on the background
/// worker thread. Returns `Ok(true)` when `pacman -Sy` exits 0, `Ok(false)`
/// on a non-zero exit (recoverable: the user retries), and `Err` on fatal
/// write/spawn failures.
fn run_validation(new_text: &str) -> Result<bool> {
    write_mirrorlist(MIRRORLIST_PATH, new_text)
        .with_context(|| format!("writing {MIRRORLIST_PATH}"))?;
    let output = privileged_command("pacman")
        .arg("-Sy")
        .output()
        .context("running pacman -Sy")?;
    if !output.status.success() {
        let stderr_msg = String::from_utf8_lossy(&output.stderr).trim().to_string();
        tracing::warn!("pacman -Sy failed: {stderr_msg}");
    }
    Ok(output.status.success())
}

/// Write `text` to `path`, replacing the file. Uses `privileged_command`
/// indirectly via a temp-file + `cp` so the write succeeds even when the
/// installer is not root and `/etc/pacman.d/` is not user-writable.
fn write_mirrorlist(path: &str, text: &str) -> std::io::Result<()> {
    let tmp = std::env::temp_dir().join("clipsneko-mirrorlist.tmp");
    std::fs::write(&tmp, text)?;
    let status = privileged_command("cp").arg(&tmp).arg(path).status()?;
    let _ = std::fs::remove_file(&tmp);
    if !status.success() {
        return Err(std::io::Error::other(format!(
            "cp {} {} failed",
            tmp.display(),
            path
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
