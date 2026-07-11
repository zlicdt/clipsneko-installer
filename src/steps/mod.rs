//! Step trait, identifiers, and construction of the complete wizard flow.

mod confirm;
mod disk;
mod hostname;
mod install;
mod kernel;
mod keyboard;
mod language;
mod mirror;
mod network;
mod nvidia;
mod timezone;
mod user;

use crate::state::InstallerState;
use crate::t;
use anyhow::Result;
use crossterm::event::KeyEvent;
use ratatui::layout::Rect;
use ratatui::Frame;
use std::process::ExitStatus;

pub use confirm::ConfirmStep;
pub use disk::DiskStep;
pub use hostname::HostnameStep;
pub use install::InstallStep;
pub use kernel::KernelStep;
pub use keyboard::KeyboardStep;
pub use language::LanguageStep;
pub use mirror::MirrorStep;
pub use network::NetworkStep;
pub use nvidia::NvidiaStep;
pub use timezone::TimezoneStep;
pub use user::UserStep;

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

    /// Allow the shared Back path. The destructive install step disables it.
    fn allows_back(&self) -> bool {
        true
    }

    /// Suppress the app-level Ctrl+C quit dialog. The install step owns all
    /// exits so a running destructive command cannot be abandoned.
    fn blocks_global_quit(&self) -> bool {
        false
    }

    /// Whether the shared Back/Next footer and its global shortcut hint are
    /// meaningful for this step. The install step owns its actions in-body.
    fn shows_navigation_footer(&self) -> bool {
        true
    }

    /// Periodic update called by the app event loop even when no key arrives.
    /// Background steps use it to animate and receive worker messages.
    fn tick(&mut self, _state: &mut InstallerState) -> Result<StepAction> {
        Ok(StepAction::None)
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

/// Build the full 12-step wizard.
pub fn build_steps() -> Result<Vec<Box<dyn Step>>> {
    Ok(vec![
        Box::new(LanguageStep::new()?),
        Box::new(KeyboardStep::new()?),
        Box::new(NetworkStep::new()),
        Box::new(MirrorStep::new()?),
        Box::new(DiskStep::new()),
        Box::new(KernelStep::new()),
        Box::new(NvidiaStep::new()),
        Box::new(TimezoneStep::new()?),
        Box::new(UserStep::new()),
        Box::new(HostnameStep::new()),
        Box::new(ConfirmStep::new()),
        Box::new(InstallStep::new()),
    ])
}
