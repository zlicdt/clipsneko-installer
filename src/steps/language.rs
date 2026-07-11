//! Independent installer-language and target-locale selection.

use crate::i18n::{set_language, UiLang};
use crate::state::InstallerState;
use crate::steps::{Step, StepAction, StepId};
use crate::t;
use crate::util::locale_list::list_utf8_locales;
use crate::util::ui::focusable_block;
use anyhow::{Context, Result};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

const ALL_LANGS: [UiLang; 2] = [UiLang::En, UiLang::ZhCn];
const DEFAULT_TARGET_LOCALE: &str = "en_US.UTF-8";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LanguageFocus {
    UiLanguage,
    TargetLocale,
}

fn language_label(lang: UiLang) -> String {
    match lang {
        UiLang::En => t!("language.name.english"),
        UiLang::ZhCn => t!("language.name.simplified_chinese"),
    }
}

pub struct LanguageStep {
    ui_state: ListState,
    locale_state: ListState,
    locales: Vec<String>,
    selected_ui: UiLang,
    selected_locale: String,
    focus: LanguageFocus,
}

impl LanguageStep {
    pub fn new() -> Result<Self> {
        let locales = list_utf8_locales()?;
        let locale_index = locales
            .iter()
            .position(|locale| locale == DEFAULT_TARGET_LOCALE)
            .context("en_US.UTF-8 is absent from /etc/locale.gen")?;
        let mut ui_state = ListState::default();
        ui_state.select(Some(0));
        let mut locale_state = ListState::default();
        locale_state.select(Some(locale_index));
        Ok(Self {
            ui_state,
            locale_state,
            locales,
            selected_ui: UiLang::En,
            selected_locale: DEFAULT_TARGET_LOCALE.to_string(),
            focus: LanguageFocus::UiLanguage,
        })
    }

    fn sync_from_state(&mut self, state: &InstallerState) {
        let ui = state.ui_lang.unwrap_or(UiLang::En);
        if ui != self.selected_ui {
            self.selected_ui = ui;
            let index = ALL_LANGS.iter().position(|lang| *lang == ui).unwrap_or(0);
            self.ui_state.select(Some(index));
        }
        if let Some(locale) = state.target_locale.as_ref() {
            if locale != &self.selected_locale {
                self.selected_locale = locale.clone();
                if let Some(index) = self.locales.iter().position(|item| item == locale) {
                    self.locale_state.select(Some(index));
                }
            }
        }
    }

    fn apply_ui_language(&mut self, lang: UiLang, state: &mut InstallerState) -> Result<()> {
        set_language(lang).with_context(|| format!("applying UI language {lang:?}"))?;
        self.selected_ui = lang;
        state.ui_lang = Some(lang);
        Ok(())
    }

    fn apply_target_locale(&mut self, locale: String, state: &mut InstallerState) {
        self.selected_locale = locale.clone();
        state.target_locale = Some(locale);
    }

    fn highlighted_ui(&self) -> UiLang {
        ALL_LANGS[self.ui_state.selected().unwrap_or(0)]
    }

    fn highlighted_locale(&self) -> String {
        self.locales[self.locale_state.selected().unwrap_or(0)].clone()
    }

    fn move_highlight(&mut self, delta: i32) {
        let (state, len) = match self.focus {
            LanguageFocus::UiLanguage => (&mut self.ui_state, ALL_LANGS.len()),
            LanguageFocus::TargetLocale => (&mut self.locale_state, self.locales.len()),
        };
        let current = state.selected().unwrap_or(0) as i32;
        state.select(Some((current + delta).rem_euclid(len as i32) as usize));
    }

    fn apply_focused(&mut self, state: &mut InstallerState) -> Result<()> {
        match self.focus {
            LanguageFocus::UiLanguage => self.apply_ui_language(self.highlighted_ui(), state),
            LanguageFocus::TargetLocale => {
                self.apply_target_locale(self.highlighted_locale(), state);
                Ok(())
            }
        }
    }
}

impl Step for LanguageStep {
    fn id(&self) -> StepId {
        StepId::Language
    }

    fn activate(&mut self, state: &mut InstallerState) -> Result<()> {
        self.sync_from_state(state);
        state.ui_lang.get_or_insert(self.selected_ui);
        state
            .target_locale
            .get_or_insert_with(|| self.selected_locale.clone());
        Ok(())
    }

    fn render(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        _state: &InstallerState,
        body_focused: bool,
    ) {
        let vertical = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(area);
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(vertical[0]);

        let ui_items = ALL_LANGS.iter().map(|lang| {
            let style = if *lang == self.selected_ui {
                Style::default().add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(language_label(*lang)).style(style)
        });
        let ui_highlight = if body_focused && self.focus == LanguageFocus::UiLanguage {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        let ui_list = List::new(ui_items)
            .block(focusable_block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(t!("language_step.ui_title")),
                body_focused && self.focus == LanguageFocus::UiLanguage,
            ))
            .highlight_style(ui_highlight);
        frame.render_stateful_widget(ui_list, columns[0], &mut self.ui_state);

        let locale_items = self.locales.iter().map(|locale| {
            let style = if locale == &self.selected_locale {
                Style::default().add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(locale.clone()).style(style)
        });
        let locale_highlight = if body_focused && self.focus == LanguageFocus::TargetLocale {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        let locale_list = List::new(locale_items)
            .block(focusable_block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(t!("language_step.target_title")),
                body_focused && self.focus == LanguageFocus::TargetLocale,
            ))
            .highlight_style(locale_highlight);
        frame.render_stateful_widget(locale_list, columns[1], &mut self.locale_state);

        let hint = match self.focus {
            LanguageFocus::UiLanguage => t!("language_step.hint_ui"),
            LanguageFocus::TargetLocale => t!("language_step.hint_target"),
        };
        frame.render_widget(
            Paragraph::new(hint)
                .alignment(Alignment::Center)
                .style(Style::default().add_modifier(Modifier::DIM)),
            vertical[1],
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
                self.apply_focused(state)?;
                StepAction::None
            }
            KeyCode::Enter => match self.focus {
                LanguageFocus::UiLanguage => {
                    self.apply_focused(state)?;
                    self.focus = LanguageFocus::TargetLocale;
                    StepAction::None
                }
                LanguageFocus::TargetLocale => {
                    self.apply_focused(state)?;
                    StepAction::Next
                }
            },
            _ => StepAction::None,
        })
    }

    fn consume_tab(&mut self, is_shift: bool) -> bool {
        match (self.focus, is_shift) {
            (LanguageFocus::UiLanguage, false) => {
                self.focus = LanguageFocus::TargetLocale;
                true
            }
            (LanguageFocus::TargetLocale, false) => {
                self.focus = LanguageFocus::UiLanguage;
                false
            }
            (LanguageFocus::TargetLocale, true) => {
                self.focus = LanguageFocus::UiLanguage;
                true
            }
            (LanguageFocus::UiLanguage, true) => {
                self.focus = LanguageFocus::TargetLocale;
                false
            }
        }
    }

    fn on_next_button(&mut self, state: &mut InstallerState) -> Result<StepAction> {
        self.apply_ui_language(self.highlighted_ui(), state)?;
        self.apply_target_locale(self.highlighted_locale(), state);
        Ok(StepAction::Next)
    }
}

#[cfg(test)]
mod tests;
