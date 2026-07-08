//! Subprocess helpers — privilege escalation and full-screen subprocess
//! suspension.
//!
//! The installer runs as a normal user (either `root` on a root shell or the
//! passwordless `installer` user on the ClipsNeko ISO). Commands that need
//! root privileges are wrapped with `sudo` automatically when the running
//! user is not already root. Both `root` and `installer` are passwordless
//! for sudo on the ISO, so no prompt is ever shown.
//!
//! Interactive full-screen TUIs (`nmtui`, `cfdisk`, …) cannot run inside
//! ratatui's alternate screen / raw-mode session. `run_fullscreen` temporarily
//! leaves the alt screen and disables raw mode, runs the subprocess attached
//! to the real terminal, then restores both — see `design.md` §9.

use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use std::process::Command;

/// True when the current process has effective UID 0 (i.e. already root,
/// no `sudo` prefix needed).
#[allow(dead_code)] // first used by M2/M4 commands that shell out to root tools.
pub fn is_root() -> bool {
    // SAFETY: `geteuid` is a read-only syscall with no preconditions.
    unsafe { libc::geteuid() == 0 }
}

/// Start building a `Command` for `program`, prefixing `sudo` when the
/// running user is not root. When already root the command runs directly.
///
/// Example:
/// ```
/// use crate::util::process::privileged_command;
/// let status = privileged_command("mount")
///     .args(["/dev/sda1", "/mnt"])
///     .status()?;
/// ```
pub fn privileged_command(program: &str) -> Command {
    if is_root() {
        Command::new(program)
    } else {
        let mut cmd = Command::new("sudo");
        cmd.arg("--").arg(program);
        cmd
    }
}

/// Run a full-screen interactive subprocess (e.g. `nmtui`, `cfdisk`) by
/// temporarily suspending the ratatui terminal: leave the alternate screen
/// and disable raw mode so the subprocess gets a normal terminal, then
/// restore both after it exits. The caller must clear the ratatui
/// `Terminal` buffer afterwards (e.g. `terminal.clear()`) because the
/// internal frame buffer is stale after the screen was replaced.
///
/// Returns the subprocess exit status on success. If the subprocess fails
/// to spawn, the ratatui session is still resumed (the error is propagated
/// to the caller for logging).
pub fn run_fullscreen(program: &str, args: &[&str]) -> std::io::Result<std::process::ExitStatus> {
    disable_raw_mode()?;
    execute!(std::io::stdout(), LeaveAlternateScreen)?;

    let result = Command::new(program).args(args).status();

    // Always try to resume ratatui, even if the subprocess failed to spawn.
    // If resume itself fails the terminal is in a bad state, but logging is
    // the best we can do — the panic hook will clean up on exit.
    if let Err(e) = execute!(std::io::stdout(), EnterAlternateScreen) {
        tracing::error!("failed to re-enter alternate screen: {e}");
    }
    if let Err(e) = enable_raw_mode() {
        tracing::error!("failed to re-enable raw mode: {e}");
    }

    result
}

#[cfg(test)]
mod tests;
