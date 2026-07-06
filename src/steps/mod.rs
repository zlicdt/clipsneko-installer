//! Step trait, identifiers, and the placeholder step used until each step's
//! real UI lands. Per-step modules (`steps/language.rs`, etc.) will be
//! created as individual steps get real implementations; until then a single
//! `StubStep` services all 12 slots so navigation works end-to-end.

use crate::state::InstallerState;
use crate::t;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::Rect;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

/// Result of a step handling a key.
pub enum StepAction {
    None,
    Next,
    Back,
    /// Emitted by a step that wants the whole wizard to exit (e.g. the install
    /// step after a successful run).
    #[allow(dead_code)] // constructed when the install step lands.
    Quit,
}

/// Identifier for a wizard step. Order matches the layout in `design.md` §4.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepId {
    Language,
    Keyboard,
    Network,
    Mirror,
    Disk,
    Kernel,
    Nvidia,
    Timezone,
    User,
    Hostname,
    Confirm,
    Install,
}

impl StepId {
    /// Translated title shown in the header and any per-step block.
    pub fn title(self) -> String {
        match self {
            StepId::Language => t!("Language"),
            StepId::Keyboard => t!("Keyboard Layout"),
            StepId::Network => t!("Network"),
            StepId::Mirror => t!("Mirror List"),
            StepId::Disk => t!("Disk Partitioning"),
            StepId::Kernel => t!("Kernel"),
            StepId::Nvidia => t!("Nvidia Driver"),
            StepId::Timezone => t!("Timezone"),
            StepId::User => t!("User Account"),
            StepId::Hostname => t!("Hostname"),
            StepId::Confirm => t!("Confirm Installation"),
            StepId::Install => t!("Installing"),
        }
    }
}

/// One wizard step. Steps own their local UI state (cursor, input buffer,
/// selection index, etc.) and read/write the shared `InstallerState`.
pub trait Step {
    fn id(&self) -> StepId;
    fn render(&self, frame: &mut Frame, area: Rect, state: &InstallerState);
    fn handle_key(&mut self, key: KeyEvent, state: &mut InstallerState) -> StepAction;
}

/// Placeholder step: renders a "not implemented" notice and advances on
/// Enter / goes back on Esc. Replaced step-by-step as real UI is written.
pub struct StubStep {
    id: StepId,
}

impl StubStep {
    pub fn new(id: StepId) -> Self {
        StubStep { id }
    }
}

impl Step for StubStep {
    fn id(&self) -> StepId {
        self.id
    }

    fn render(&self, frame: &mut Frame, area: Rect, _state: &InstallerState) {
        let body = format!(
            "{}\n\n{}",
            t!("This step is not implemented yet."),
            t!("Press Enter to continue, Esc to go back."),
        );
        frame.render_widget(Paragraph::new(body), area);
    }

    fn handle_key(&mut self, key: KeyEvent, _state: &mut InstallerState) -> StepAction {
        if key.kind != KeyEventKind::Press {
            return StepAction::None;
        }
        match key.code {
            KeyCode::Enter => StepAction::Next,
            KeyCode::Esc => StepAction::Back,
            _ => StepAction::None,
        }
    }
}

/// Build the full 12-step wizard. Each entry is a stub for now and will be
/// swapped for a real step module as that step's UI is implemented.
pub fn build_steps() -> Vec<Box<dyn Step>> {
    vec![
        Box::new(StubStep::new(StepId::Language)),
        Box::new(StubStep::new(StepId::Keyboard)),
        Box::new(StubStep::new(StepId::Network)),
        Box::new(StubStep::new(StepId::Mirror)),
        Box::new(StubStep::new(StepId::Disk)),
        Box::new(StubStep::new(StepId::Kernel)),
        Box::new(StubStep::new(StepId::Nvidia)),
        Box::new(StubStep::new(StepId::Timezone)),
        Box::new(StubStep::new(StepId::User)),
        Box::new(StubStep::new(StepId::Hostname)),
        Box::new(StubStep::new(StepId::Confirm)),
        Box::new(StubStep::new(StepId::Install)),
    ]
}
