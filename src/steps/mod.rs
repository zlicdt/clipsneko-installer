//! Step trait, identifiers, and the placeholder step used until each step's
//! real UI lands. Per-step modules (`steps/language.rs`, etc.) will be
//! created as individual steps get real implementations; until then a single
//! `StubStep` services all 12 slots so navigation works end-to-end.

mod language;

use crate::state::InstallerState;
use crate::t;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::Rect;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub use language::LanguageStep;

/// Result of a step handling a key.
pub enum StepAction {
    None,
    Next,
    /// Emitted by a step that wants to go back. Currently no step emits this
    /// (Esc is intercepted by `app.rs` as quit; Back is the on-screen button),
    /// but the variant is kept for forward-compat with steps that have their
    /// own cancel logic.
    #[allow(dead_code)]
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
            StepId::Language => t!("step.title.language"),
            StepId::Keyboard => t!("step.title.keyboard_layout"),
            StepId::Network => t!("step.title.network"),
            StepId::Mirror => t!("step.title.mirror_list"),
            StepId::Disk => t!("step.title.disk_partitioning"),
            StepId::Kernel => t!("step.title.kernel"),
            StepId::Nvidia => t!("step.title.nvidia_driver"),
            StepId::Timezone => t!("step.title.timezone"),
            StepId::User => t!("step.title.user_account"),
            StepId::Hostname => t!("step.title.hostname"),
            StepId::Confirm => t!("step.title.confirm_installation"),
            StepId::Install => t!("step.title.installing"),
        }
    }
}

/// One wizard step. Steps own their local UI state (cursor, input buffer,
/// selection index, etc.) and read/write the shared `InstallerState`.
pub trait Step {
    fn id(&self) -> StepId;
    fn render(&mut self, frame: &mut Frame, area: Rect, state: &InstallerState);
    fn handle_key(&mut self, key: KeyEvent, state: &mut InstallerState) -> StepAction;
}

/// Placeholder step: renders a "not implemented" notice and advances on
/// Enter. Esc/Back is no longer handled here — app.rs intercepts Esc as a
/// global quit request, so the only way back is the on-screen Back button.
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

    fn render(&mut self, frame: &mut Frame, area: Rect, _state: &InstallerState) {
        let body = format!("{}\n\n{}", t!("stub.body"), t!("stub.hint"),);
        frame.render_widget(Paragraph::new(body), area);
    }

    fn handle_key(&mut self, key: KeyEvent, _state: &mut InstallerState) -> StepAction {
        if key.kind != KeyEventKind::Press {
            return StepAction::None;
        }
        match key.code {
            KeyCode::Enter => StepAction::Next,
            _ => StepAction::None,
        }
    }
}

/// Build the full 12-step wizard. Steps with a real implementation are wired
/// in here; the rest are stubs swapped out as their UI is written.
pub fn build_steps() -> Vec<Box<dyn Step>> {
    vec![
        Box::new(LanguageStep::new()),
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
