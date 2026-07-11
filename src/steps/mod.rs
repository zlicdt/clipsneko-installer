//! Step trait, identifiers, and the placeholder step used until each step's
//! real UI lands. Per-step modules (`steps/language.rs`, etc.) will be
//! created as individual steps get real implementations; until then a single
//! `StubStep` services all 12 slots so navigation works end-to-end.

mod disk;
mod kernel;
mod keyboard;
mod language;
mod mirror;
mod network;
mod nvidia;

use crate::state::InstallerState;
use crate::t;
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::Rect;
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use std::process::ExitStatus;

pub use disk::DiskStep;
pub use kernel::KernelStep;
pub use keyboard::KeyboardStep;
pub use language::LanguageStep;
pub use mirror::MirrorStep;
pub use network::NetworkStep;
pub use nvidia::NvidiaStep;

/// Result of a step handling a key.
pub enum StepAction {
    None,
    Next,
    /// Move through the current step's Back path.
    Back,
    /// Emitted by a step that wants the whole wizard to exit (e.g. the install
    /// step after a successful run).
    #[allow(dead_code)] // constructed when the install step lands.
    Quit,
    /// Suspend ratatui, run `program args` as a full-screen subprocess, then
    /// resume. `app.rs` handles the actual suspension/resume and calls
    /// `Step::on_subprocess_done` with the exit status afterwards. Used by
    /// steps that need to launch interactive TUIs such as `nmtui` or `cfdisk`.
    SuspendRun(String, Vec<String>),
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
    fn render(&mut self, frame: &mut Frame, area: Rect, state: &InstallerState, body_focused: bool);
    fn handle_key(&mut self, key: KeyEvent, state: &mut InstallerState) -> Result<StepAction>;

    /// Called when this step becomes the current step (on initial entry and
    /// on every Back/Next navigation into it). Lets the step run entry-time
    /// side effects such as a connectivity check or refreshing device lists.
    /// Default: no-op.
    fn activate(&mut self, _state: &mut InstallerState) -> Result<()> {
        Ok(())
    }

    /// Whether this step is complete enough for the Next button to be
    /// enabled. Steps that require a validated choice before proceeding
    /// (e.g. network must be online) override this. Default: `true`.
    fn is_complete(&self, _state: &InstallerState) -> bool {
        true
    }

    /// Whether this step currently shows a modal overlay that must receive all
    /// keyboard input before the app handles global keys or footer focus.
    /// Modal steps override this while their dialog is visible so keys cannot
    /// activate controls behind the overlay.
    fn has_modal(&self) -> bool {
        false
    }

    /// Called after a `StepAction::SuspendRun` subprocess finishes (whether
    /// it succeeded or not). Lets the step react — e.g. re-check connectivity
    /// after `nmtui` returns. Default: no-op.
    fn on_subprocess_done(
        &mut self,
        _status: ExitStatus,
        _state: &mut InstallerState,
    ) -> Result<()> {
        Ok(())
    }

    /// Attempt to consume a Tab/BackTab for internal focus cycling between
    /// sub-widgets within the step body (e.g. the mirror step toggles
    /// between its region list and the manual-input field). Returns `true`
    /// when the step used the key internally (the app must then **not**
    /// perform its global StepBody ↔ button focus cycle); returns `false`
    /// when the step declines the key (the app proceeds with the global
    /// cycle, moving focus to/from the Back/Next buttons).
    ///
    /// `is_shift` is `true` for BackTab (Shift+Tab), `false` for Tab.
    ///
    /// The default is `false` so that steps with a single focusable widget
    /// (the common case) always let Tab bubble up to the app's global
    /// focus cycle. Steps that override this must return `false` at the
    /// ends of their internal focus chain so the user can still Tab out to
    /// the buttons — otherwise the global cycle is broken.
    fn consume_tab(&mut self, _is_shift: bool) -> bool {
        false
    }

    /// Called when the user activates the on-screen **Next** button. The
    /// default matches Enter-based forward navigation. A step can commit its
    /// current selection before returning `Next`, return `None` to stay put
    /// (for an internal page switch or validation dialog), or emit another
    /// action when needed.
    fn on_next_button(&mut self, _state: &mut InstallerState) -> Result<StepAction> {
        Ok(StepAction::Next)
    }

    /// Symmetric counterpart of `on_next_button` for the on-screen **Back**
    /// button. The default moves to the previous wizard step; a step with an
    /// internal sub-page can return `None` after handling the transition.
    fn on_back_button(&mut self, _state: &mut InstallerState) -> Result<StepAction> {
        Ok(StepAction::Back)
    }
}

/// Placeholder step: renders a "not implemented" notice and advances on
/// Enter. App-level Esc follows the same Back path as the footer button.
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

    fn render(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        _state: &InstallerState,
        _body_focused: bool,
    ) {
        let body = format!("{}\n\n{}", t!("stub.body"), t!("stub.hint"),);
        frame.render_widget(Paragraph::new(body), area);
    }

    fn handle_key(&mut self, key: KeyEvent, _state: &mut InstallerState) -> Result<StepAction> {
        if key.kind != KeyEventKind::Press {
            return Ok(StepAction::None);
        }
        Ok(match key.code {
            KeyCode::Enter => StepAction::Next,
            _ => StepAction::None,
        })
    }
}

/// Build the full 12-step wizard. Steps with a real implementation are wired
/// in here; the rest are stubs swapped out as their UI is written.
pub fn build_steps() -> Result<Vec<Box<dyn Step>>> {
    Ok(vec![
        Box::new(LanguageStep::new()?),
        Box::new(KeyboardStep::new()?),
        Box::new(NetworkStep::new()),
        Box::new(MirrorStep::new()?),
        Box::new(DiskStep::new()),
        Box::new(KernelStep::new()),
        Box::new(NvidiaStep::new()),
        Box::new(StubStep::new(StepId::Timezone)),
        Box::new(StubStep::new(StepId::User)),
        Box::new(StubStep::new(StepId::Hostname)),
        Box::new(StubStep::new(StepId::Confirm)),
        Box::new(StubStep::new(StepId::Install)),
    ])
}
