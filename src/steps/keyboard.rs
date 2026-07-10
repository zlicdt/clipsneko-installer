//! Keyboard layout selection step — pick the console keymap.
//!
//! Interaction (per `design.md` §6): Up/Down or j/k moves the highlight; Space
//! selects the highlighted keymap and applies it immediately via `loadkeys`
//! so the effect is visible in the live console; Enter selects, applies, and
//! advances to the next step. Esc is not handled here — `app.rs` intercepts
//! it as a global quit request; the only way back is the on-screen Back
//! button.
//!
//! The list of keymaps is loaded once from `localectl list-keymaps` at
//! construction time; the currently active keymap is detected from
//! `localectl status` (the `VC KEYMAP:` line) so the picker opens with the
//! live keymap highlighted and applied (rendered bold+bright). The chosen
//! keymap is persisted in `state.keymap` and later written to the target's
//! `/etc/vconsole.conf` in the install stage (§5).
//!
//! Per the keyboard step design, `localectl list-keymaps` and `loadkeys` are
//! assumed to always succeed on the ClipsNeko Live ISO: a failure at startup
//! panics (the panic hook restores the terminal), and a `loadkeys` failure at
//! runtime is logged but does not block navigation.

use crate::state::InstallerState;
use crate::steps::{Step, StepAction, StepId};
use crate::t;
use crate::util::process::privileged_command;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

pub struct KeyboardStep {
    /// All keymap names from `localectl list-keymaps`, in the order returned.
    keymaps: Vec<String>,
    /// Cursor position in the list (which row is highlighted). Owned here so
    /// `render(&mut self)` can pass `&mut self.list_state` to
    /// `render_stateful_widget` — cloning the state (as a `&self` render
    /// would require) loses ratatui's offset bookkeeping and can wedge the
    /// list.
    list_state: ListState,
    /// The keymap currently applied via `loadkeys` (▶ marker). Kept in sync
    /// with `state.keymap` by `apply()`; used purely for rendering the
    /// selection marker, independent of the highlight cursor.
    selected: String,
}

impl KeyboardStep {
    pub fn new() -> Self {
        let keymaps = load_keymap_list();
        let current = current_keymap().unwrap_or_else(|| "us".to_string());
        let idx = keymaps.iter().position(|k| k == &current).unwrap_or(0);
        let mut list_state = ListState::default();
        list_state.select(Some(idx));
        Self {
            keymaps,
            list_state,
            selected: current,
        }
    }

    /// Reconcile local state with the shared `InstallerState`. Needed when the
    /// user returns to this step from a later step via Back: `state.keymap`
    /// may have been set during a previous visit and the local `selected`
    /// field is updated to match so the marker and highlight are consistent.
    /// Only touches `list_state` when `selected` actually changes, so normal
    /// highlight navigation (which doesn't touch `selected`) is preserved.
    fn sync_from_state(&mut self, state: &InstallerState) {
        let want = state
            .keymap
            .clone()
            .unwrap_or_else(|| self.selected.clone());
        if want != self.selected {
            self.selected = want;
            if let Some(idx) = self.keymaps.iter().position(|k| k == &self.selected) {
                self.list_state.select(Some(idx));
            }
        }
    }

    /// Apply `keymap` via `loadkeys` and record it in the shared state. On
    /// failure the marker and state are left untouched and the error is
    /// logged; the design assumes `loadkeys` always succeeds on the Live ISO,
    /// so this is defensive only.
    fn apply(&mut self, keymap: &str, state: &mut InstallerState) {
        let status = privileged_command("loadkeys").arg(keymap).status();
        match status {
            Ok(s) if s.success() => {
                self.selected = keymap.to_string();
                state.keymap = Some(keymap.to_string());
            }
            Ok(s) => tracing::warn!("loadkeys {keymap} exited non-zero: {s}"),
            Err(e) => tracing::warn!("loadkeys {keymap} spawn failed: {e}"),
        }
    }

    /// The keymap under the current highlight cursor.
    fn highlighted(&self) -> &str {
        let idx = self.list_state.selected().unwrap_or(0);
        &self.keymaps[idx]
    }

    /// Move the highlight by `delta` positions, wrapping around at the ends.
    fn move_highlight(&mut self, delta: i32) {
        let len = self.keymaps.len() as i32;
        if len == 0 {
            return;
        }
        let cur = self.list_state.selected().unwrap_or(0) as i32;
        let next = (cur + delta).rem_euclid(len) as usize;
        self.list_state.select(Some(next));
    }
}

impl Step for KeyboardStep {
    fn id(&self) -> StepId {
        StepId::Keyboard
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, _state: &InstallerState) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(area);

        let items: Vec<ListItem> = self
            .keymaps
            .iter()
            .map(|k| {
                // The row that is *selected/applied* (via Space/Enter) is
                // rendered bold + a bright color so it stands out. The cursor
                // row is separately indicated by the `REVERSED` highlight
                // style (mirrors the language step).
                let style = if *k == self.selected {
                    Style::default()
                        .add_modifier(Modifier::BOLD)
                        .fg(Color::White)
                } else {
                    Style::default()
                };
                ListItem::new(k.clone()).style(style)
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(t!("keyboard_step.title")),
            )
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED));

        frame.render_stateful_widget(list, chunks[0], &mut self.list_state);

        let hint = t!("keyboard_step.hint");
        frame.render_widget(
            Paragraph::new(hint)
                .alignment(Alignment::Center)
                .style(Style::default().add_modifier(Modifier::DIM)),
            chunks[1],
        );
    }

    fn handle_key(&mut self, key: KeyEvent, state: &mut InstallerState) -> StepAction {
        if key.kind != KeyEventKind::Press {
            return StepAction::None;
        }

        self.sync_from_state(state);

        match key.code {
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_highlight(1);
                StepAction::None
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_highlight(-1);
                StepAction::None
            }
            KeyCode::Char(' ') => {
                let keymap = self.highlighted().to_string();
                self.apply(&keymap, state);
                StepAction::None
            }
            KeyCode::Enter => {
                let keymap = self.highlighted().to_string();
                self.apply(&keymap, state);
                StepAction::Next
            }
            // Esc is not handled here; app.rs intercepts it as quit.
            _ => StepAction::None,
        }
    }
}

/// Run `localectl list-keymaps` and return the keymap names, one per line.
/// Panics if the command fails — on the ClipsNeko Live ISO `localectl` is
/// always available, so this is a startup-fatal assumption rather than a
/// recoverable error (per the keyboard step design).
fn load_keymap_list() -> Vec<String> {
    let output = privileged_command("localectl")
        .arg("list-keymaps")
        .output()
        .expect("localectl list-keymaps failed");
    parse_keymap_list(&String::from_utf8_lossy(&output.stdout))
}

/// Run `localectl status` and return the active VC keymap, if any. Returns
/// `None` when the `VC KEYMAP:` line is absent or unset (e.g. `n/a`); the
/// caller defaults to `"us"` in that case. Panics if the command itself
/// fails (startup-fatal, same assumption as `load_keymap_list`).
fn current_keymap() -> Option<String> {
    let output = privileged_command("localectl")
        .arg("status")
        .output()
        .expect("localectl status failed");
    parse_current_keymap(&String::from_utf8_lossy(&output.stdout))
}

/// Parse `localectl list-keymaps` output into a list of keymap names: one
/// per line, trimming whitespace and dropping empty lines.
fn parse_keymap_list(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect()
}

/// Parse `localectl status` output and return the value of the `VC KEYMAP:`
/// field, if present and non-empty. `n/a` is treated as unset.
fn parse_current_keymap(stdout: &str) -> Option<String> {
    stdout.lines().find_map(|line| {
        let line = line.trim_start();
        let rest = line.strip_prefix("VC KEYMAP:")?;
        let v = rest.trim();
        if v.is_empty() || v.eq_ignore_ascii_case("n/a") {
            None
        } else {
            Some(v.to_string())
        }
    })
}

#[cfg(test)]
mod tests;
