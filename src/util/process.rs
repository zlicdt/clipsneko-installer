//! Subprocess helpers — in particular, privilege escalation.
//!
//! The installer runs as a normal user (either `root` on a root shell or the
//! passwordless `installer` user on the ClipsNeko ISO). Commands that need
//! root privileges are wrapped with `sudo` automatically when the running
//! user is not already root. Both `root` and `installer` are passwordless
//! for sudo on the ISO, so no prompt is ever shown.

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

#[cfg(test)]
mod tests;
