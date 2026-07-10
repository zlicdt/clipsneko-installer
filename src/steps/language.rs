//! Language selection step — pick the installer UI language (en / zh_CN).
//!
//! Interaction (per `design.md` §6): Up/Down or j/k moves the highlight; Space
//! selects the highlighted language and applies it live via `set_language()`
//! so the rest of the UI re-translates immediately; Enter selects and
//! advances to the next step. Esc is no longer handled here — `app.rs`
//! intercepts it as a global quit request; the only way back is the on-screen
//! Back button.
//!
//! The installer UI language is independent of the target system's locale
//! (see `design.md` §4.1). It is not persisted across runs: the Live ISO
//! starts fresh each boot, and the target system's locale is configured in
//! the install stage (§5).
//!
//! Per the project-wide list style (no "▶" marker): the *selected/applied*
//! row's own text is bold + a bright color to signal selected state. The
//! cursor row is indicated by the `REVERSED` highlight style, independent of
//! which row is currently applied.

use crate::i18n::{set_language, UiLang};
use crate::state::InstallerState;
use crate::steps::{Step, StepAction, StepId};
use crate::t;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

/// All supported UI languages in the order they appear in the picker.
const ALL_LANGS: [UiLang; 3] = [UiLang::En, UiLang::ZhCn, UiLang::ZhTw];

pub struct LanguageStep {
    /// Cursor position in the list (which row is highlighted). Owned here so
    /// `render(&mut self)` can pass `&mut self.list_state` to
    /// `render_stateful_widget` — cloning the state (as a `&self` render
    /// would require) loses ratatui's offset bookkeeping and can wedge the
    /// list.
    list_state: ListState,
    /// The language whose translation catalog is currently active. Kept in
    /// sync with `state.ui_lang` by `apply()`; used purely for rendering the
    /// selection marker.
    selected: UiLang,
}

impl LanguageStep {
    pub fn new() -> Self {
        let mut list_state = ListState::default();
        list_state.select(Some(0));
        Self {
            list_state,
            selected: UiLang::En,
        }
    }

    /// Reconcile local state with the shared `InstallerState`. Needed when the
    /// user returns to this step from a later step via Back: `state.ui_lang`
    /// may have been set during a previous visit and the local `selected`
    /// field is updated to match so the marker and highlight are consistent.
    /// Only touches `list_state` when `selected` actually changes, so normal
    /// highlight navigation (which doesn't touch `selected`) is preserved.
    fn sync_from_state(&mut self, state: &InstallerState) {
        let want = state.ui_lang.unwrap_or(UiLang::En);
        if want != self.selected {
            self.selected = want;
            let idx = ALL_LANGS
                .iter()
                .position(|l| *l == self.selected)
                .unwrap_or(0);
            self.list_state.select(Some(idx));
        }
    }

    /// Apply `lang` via gettext and record it in the shared state. On failure
    /// (the locale is not generated on the running system), fall back to
    /// English silently: the ISO build is responsible for generating both
    /// `en_US.UTF-8` and `zh_CN.UTF-8`, so this path is defensive only. The
    /// failure is still logged via `tracing::warn!` for diagnostics.
    fn apply(&mut self, lang: UiLang, state: &mut InstallerState) {
        if let Err(e) = set_language(lang) {
            tracing::warn!(
                "set_language({:?}) failed: {e}; falling back to English",
                lang
            );
            self.selected = UiLang::En;
            state.ui_lang = Some(UiLang::En);
            if let Err(e2) = set_language(UiLang::En) {
                tracing::error!("set_language(En) fallback also failed: {e2}");
            }
            return;
        }
        self.selected = lang;
        state.ui_lang = Some(lang);
    }

    /// The language under the current highlight cursor.
    fn highlighted(&self) -> UiLang {
        let idx = self.list_state.selected().unwrap_or(0);
        ALL_LANGS[idx]
    }

    /// Move the highlight by `delta` positions, wrapping around at the ends.
    fn move_highlight(&mut self, delta: i32) {
        let len = ALL_LANGS.len() as i32;
        let cur = self.list_state.selected().unwrap_or(0) as i32;
        let next = (cur + delta).rem_euclid(len) as usize;
        self.list_state.select(Some(next));
    }
}

impl Step for LanguageStep {
    fn id(&self) -> StepId {
        StepId::Language
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, _state: &InstallerState) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(area);

        let items: Vec<ListItem> = ALL_LANGS
            .iter()
            .map(|l| {
                // The row that is *selected/applied* (via Space/Enter on a
                // previous visit) gets its text bold + a bright color so it
                // stands out from the rest. The cursor row is separately
                // indicated by the `REVERSED` highlight style. This separates
                // "what's active" from "where the keyboard is" — e.g. after
                // launching with En applied and pressing Down once, the cursor
                // is on ZhCn (reversed bg) while "English" stays bold+bright
                // until Space/Enter applies ZhCn.
                let style = if *l == self.selected {
                    Style::default()
                        .add_modifier(Modifier::BOLD)
                        .fg(Color::White)
                } else {
                    Style::default()
                };
                ListItem::new(l.label().to_string()).style(style)
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(t!("language_step.title")),
            )
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED));

        frame.render_stateful_widget(list, chunks[0], &mut self.list_state);

        let hint = t!("language_step.hint");
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
                let lang = self.highlighted();
                self.apply(lang, state);
                StepAction::None
            }
            KeyCode::Enter => {
                let lang = self.highlighted();
                self.apply(lang, state);
                StepAction::Next
            }
            // Esc is no longer handled here; app.rs intercepts it as quit.
            _ => StepAction::None,
        }
    }
}

#[cfg(test)]
mod tests;
