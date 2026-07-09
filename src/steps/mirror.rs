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
//! - The rewritten mirrorlist is written back, then `pacman -Sy` is run
//!   (`.output()` so its output never reaches the ratatui terminal). Exit
//!   0 → `state.mirror_lines` recorded, advance. Non-zero → a modal error
//!   dialog shows the failure; the user dismisses it and retries.
//!
//! Esc is not handled here — `app.rs` intercepts it as a global quit.

use crate::state::InstallerState;
use crate::steps::{Step, StepAction, StepId};
use crate::t;
use crate::util::process::privileged_command;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

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
    /// True once a mirror selection has validated successfully via
    /// `pacman -Sy`. Gates `is_complete` / the Next button.
    validated: bool,
}

impl MirrorStep {
    pub fn new() -> Self {
        let raw = std::fs::read_to_string(MIRRORLIST_PATH)
            .unwrap_or_else(|e| panic!("failed to read {MIRRORLIST_PATH}: {e}"));
        let regions = parse_mirrorlist_regions(&raw);
        let mut list_state = ListState::default();
        list_state.select(Some(0));
        Self {
            regions,
            raw,
            list_state,
            selected: String::new(),
            input: String::new(),
            focus: MirrorFocus::List,
            error: ErrorDialog::default(),
            validated: false,
        }
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

    /// Run the validation flow: rewrite the mirrorlist for the given
    /// selection, run `pacman -Sy`, and on success record the choice and
    /// mark the step validated. On failure, surface a modal error. Returns
    /// `Next` if the caller should advance, `None` otherwise.
    fn validate_and_advance(&mut self, state: &mut InstallerState) -> StepAction {
        let input_trimmed = self.input.trim().to_string();
        let selection = if !input_trimmed.is_empty() {
            match normalize_server_line(&input_trimmed) {
                Some(line) => Selection::Manual(line),
                None => {
                    self.error = ErrorDialog {
                        visible: true,
                        message: t!("mirror_step.error_invalid_url"),
                    };
                    return StepAction::None;
                }
            }
        } else {
            Selection::Region(self.highlighted().to_string())
        };

        let new_text = match &selection {
            Selection::Region(region) => reorder_mirrorlist(&self.raw, region),
            Selection::Manual(line) => {
                let mut s = String::new();
                s.push_str(line);
                s.push('\n');
                s.push('\n');
                s.push_str(&self.raw);
                s
            }
        };

        if let Err(e) = write_mirrorlist(MIRRORLIST_PATH, &new_text) {
            tracing::error!("failed to write {MIRRORLIST_PATH}: {e}");
            self.error = ErrorDialog {
                visible: true,
                message: t!("mirror_step.error_write"),
            };
            return StepAction::None;
        }

        let status = privileged_command("pacman").arg("-Sy").output();
        let ok = match &status {
            Ok(o) => o.status.success(),
            Err(e) => {
                tracing::error!("pacman -Sy spawn failed: {e}");
                false
            }
        };

        if ok {
            self.validated = true;
            state.mirror_lines = match &selection {
                Selection::Region(region) => extract_region_servers(&self.raw, region),
                Selection::Manual(line) => vec![line.clone()],
            };
            self.selected = match &selection {
                Selection::Region(r) => r.clone(),
                Selection::Manual(l) => l.clone(),
            };
            StepAction::Next
        } else {
            let stderr_msg = match &status {
                Ok(o) => String::from_utf8_lossy(&o.stderr).trim().to_string(),
                Err(e) => e.to_string(),
            };
            tracing::warn!("pacman -Sy failed: {stderr_msg}");
            self.error = ErrorDialog {
                visible: true,
                message: t!("mirror_step.error_pacman"),
            };
            StepAction::None
        }
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

    fn is_complete(&self, _state: &InstallerState) -> bool {
        self.validated
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, _state: &InstallerState) {
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
                let marker = if *r == self.selected { "▶" } else { " " };
                ListItem::new(format!("{marker} {r}"))
            })
            .collect();
        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(t!("mirror_step.list_title")),
            )
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
        frame.render_stateful_widget(list, chunks[0], &mut self.list_state);

        // Manual input field (bottom, above the hint): a single-line bordered box.
        // When focused, show a block cursor at the end of the text instead of
        // reversing the whole box — a single visible caret is clearer than
        // inverting the entire field.
        let input_label = t!("mirror_step.input_label");
        let cursor = if self.focus == MirrorFocus::Input {
            "█"
        } else {
            ""
        };
        let input_display = format!("{input_label}: {}{cursor}", self.input);
        let input_box = Paragraph::new(input_display).block(
            Block::default()
                .borders(Borders::ALL)
                .title(t!("mirror_step.input_title")),
        );
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
    }

    fn handle_key(&mut self, key: KeyEvent, state: &mut InstallerState) -> StepAction {
        if key.kind != KeyEventKind::Press {
            return StepAction::None;
        }

        // Error dialog swallows all keys except Esc/Enter which dismiss it.
        if self.error.visible {
            match key.code {
                KeyCode::Esc | KeyCode::Enter => {
                    self.error.visible = false;
                }
                _ => {}
            }
            return StepAction::None;
        }

        // Tab/BackTab are routed through `consume_tab` by app.rs, not here.
        if key.code == KeyCode::Tab || key.code == KeyCode::BackTab {
            return StepAction::None;
        }

        match self.focus {
            MirrorFocus::List => match key.code {
                KeyCode::Down | KeyCode::Char('j') => {
                    self.move_highlight(1);
                    StepAction::None
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.move_highlight(-1);
                    StepAction::None
                }
                KeyCode::Enter => self.validate_and_advance(state),
                _ => StepAction::None,
            },
            MirrorFocus::Input => match key.code {
                KeyCode::Enter => self.validate_and_advance(state),
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
        }
    }

    fn consume_tab(&mut self, is_shift: bool) -> bool {
        // Internal focus chain: List <-> Input. At either end, return false
        // so the Tab/BackTab bubbles up to app.rs's global StepBody <-> button
        // cycle — this keeps the full loop (StepBody -> buttons -> StepBody)
        // reachable from every focus position.
        match (self.focus, is_shift) {
            (MirrorFocus::List, false) => {
                // Tab from List -> Input (consumed).
                self.focus = MirrorFocus::Input;
                true
            }
            (MirrorFocus::Input, false) => {
                // Tab from Input -> bubble to app (goes to Back/Next button).
                false
            }
            (MirrorFocus::Input, true) => {
                // BackTab from Input -> List (consumed).
                self.focus = MirrorFocus::List;
                true
            }
            (MirrorFocus::List, true) => {
                // BackTab from List -> bubble to app (goes to Next/Back button).
                false
            }
        }
    }
}

impl MirrorStep {
    fn render_error_dialog(&self, frame: &mut Frame) {
        let area = centered_rect(60, 7, frame.area());
        let text = vec![
            ratatui::text::Line::from(""),
            ratatui::text::Line::from(self.error.message.clone()),
            ratatui::text::Line::from(""),
            ratatui::text::Line::from(t!("mirror_step.error_hint")),
        ];
        let dialog = Paragraph::new(text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(t!("mirror_step.error_title")),
            )
            .alignment(Alignment::Center);
        frame.render_widget(Clear, area);
        frame.render_widget(dialog, area);
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

/// Centered rect helper for the error dialog. `width_pct` is a percentage of
/// `area.width`; `height_rows` is a fixed row count (clamped to area height).
fn centered_rect(width_pct: u16, height_rows: u16, area: Rect) -> Rect {
    let h = height_rows.min(area.height);
    let y = area.y + (area.height - h) / 2;
    let w_pad = (100u16).saturating_sub(width_pct) / 2;
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(w_pad),
            Constraint::Percentage(width_pct),
            Constraint::Percentage(w_pad),
        ])
        .split(area);
    let inner = horizontal[1];
    Rect::new(inner.x, y, inner.width, h)
}

#[cfg(test)]
mod tests;
