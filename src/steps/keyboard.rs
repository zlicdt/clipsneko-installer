//! Keyboard layout selection step — pick the console keymap.
//!
//! Interaction (per `design.md` §6): Up/Down or j/k moves the highlight; Space
//! selects the highlighted keymap and applies it immediately via `loadkeys`
//! so the effect is visible in the live console; Enter selects, applies, and
//! advances to the next step. Esc is handled by `app.rs` as Back.
//!
//! The list of keymaps is loaded once from `localectl list-keymaps` at
//! construction time; the currently active keymap is detected from
//! `localectl status` (the `VC KEYMAP:` line) so the picker opens with the
//! live keymap highlighted and applied (rendered bold). The chosen
//! keymap is persisted in `state.keymap` and later written to the target's
//! `/etc/vconsole.conf` in the install stage (§5).
//!
//! `localectl` and `loadkeys` are Live ISO invariants; command/output failures
//! propagate as fatal errors after the terminal is restored.

use crate::state::InstallerState;
use crate::steps::{Step, StepAction, StepId};
use crate::t;
use crate::util::process::privileged_command;
use anyhow::{bail, Context, Result};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
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
    /// The keymap currently applied via `loadkeys`. Kept in sync
    /// with `state.keymap` by `apply()`; used purely for rendering the
    /// selection marker, independent of the highlight cursor.
    selected: String,
}

impl KeyboardStep {
    pub fn new() -> Result<Self> {
        let keymaps = load_keymap_list()?;
        let current = current_keymap()?;
        let idx = keymaps
            .iter()
            .position(|keymap| keymap == &current)
            .with_context(|| format!("active keymap {current:?} is absent from localectl list"))?;
        let mut list_state = ListState::default();
        list_state.select(Some(idx));
        Ok(Self {
            keymaps,
            list_state,
            selected: current,
        })
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

    /// Apply `keymap` via `loadkeys` and record it in shared state. The keymap
    /// originates from `localectl`, so any command failure is a fatal Live ISO
    /// invariant violation rather than a recoverable user-input error.
    fn apply(&mut self, keymap: &str, state: &mut InstallerState) -> Result<()> {
        let output = privileged_command("loadkeys")
            .arg(keymap)
            .output()
            .with_context(|| format!("running loadkeys {keymap}"))?;
        if !output.status.success() {
            bail!(
                "loadkeys {keymap} failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        self.selected = keymap.to_string();
        state.keymap = Some(keymap.to_string());
        Ok(())
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

    fn activate(&mut self, state: &mut InstallerState) -> Result<()> {
        self.sync_from_state(state);
        state.keymap.get_or_insert_with(|| self.selected.clone());
        Ok(())
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
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(area);

        let items: Vec<ListItem> = self
            .keymaps
            .iter()
            .map(|k| {
                // The row that is *selected/applied* (via Space/Enter) is
                // rendered bold so it stands out. The cursor
                // row is separately indicated by the `REVERSED` highlight
                // style (mirrors the language step).
                let style = if *k == self.selected {
                    Style::default().add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                ListItem::new(k.clone()).style(style)
            })
            .collect();

        let highlight_style = if body_focused {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(t!("keyboard_step.title")),
            )
            .highlight_style(highlight_style);

        frame.render_stateful_widget(list, chunks[0], &mut self.list_state);

        let hint = t!("keyboard_step.hint");
        frame.render_widget(
            Paragraph::new(hint)
                .alignment(Alignment::Center)
                .style(Style::default().add_modifier(Modifier::DIM)),
            chunks[1],
        );
    }

    fn handle_key(&mut self, key: KeyEvent, state: &mut InstallerState) -> Result<StepAction> {
        if key.kind != KeyEventKind::Press {
            return Ok(StepAction::None);
        }

        self.sync_from_state(state);

        Ok(match key.code {
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
                self.apply(&keymap, state)?;
                StepAction::None
            }
            KeyCode::Enter => {
                let keymap = self.highlighted().to_string();
                self.apply(&keymap, state)?;
                StepAction::Next
            }
            // Esc is handled by app.rs as Back.
            _ => StepAction::None,
        })
    }

    fn on_next_button(&mut self, state: &mut InstallerState) -> Result<StepAction> {
        let keymap = self.highlighted().to_string();
        self.apply(&keymap, state)?;
        Ok(StepAction::Next)
    }
}

/// Run `localectl list-keymaps` and return the keymap names, one per line.
/// Command absence, non-zero exit, or an empty result is a fatal Live ISO
/// invariant violation.
fn load_keymap_list() -> Result<Vec<String>> {
    let output = privileged_command("localectl")
        .arg("list-keymaps")
        .output()
        .context("running localectl list-keymaps")?;
    if !output.status.success() {
        bail!(
            "localectl list-keymaps failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let keymaps = parse_keymap_list(&String::from_utf8_lossy(&output.stdout));
    if keymaps.is_empty() {
        bail!("localectl list-keymaps returned no keymaps");
    }
    Ok(keymaps)
}

/// Run `localectl status` and return the active VC keymap. Returns an error
/// when the field is absent/unset or the command fails.
fn current_keymap() -> Result<String> {
    let output = privileged_command("localectl")
        .arg("status")
        .output()
        .context("running localectl status")?;
    if !output.status.success() {
        bail!(
            "localectl status failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    parse_current_keymap(&String::from_utf8_lossy(&output.stdout))
        .context("localectl status did not report an active VC keymap")
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
