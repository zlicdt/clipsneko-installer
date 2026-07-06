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
#[allow(dead_code)] // first used by M2/M4 commands that shell out to root tools.
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
mod tests {
    use super::*;

    #[test]
    fn is_root_returns_bool() {
        // We cannot assert the exact value (depends on who runs the test),
        // but it must be a deterministic bool and not panic.
        let _ = is_root();
    }

    #[test]
    fn privileged_command_as_root_has_no_sudo() {
        // When root, the command should be the program directly.
        // We can only verify this when the test runs as root.
        if is_root() {
            let cmd = privileged_command("mount");
            assert_eq!(cmd.get_program(), std::ffi::OsStr::new("mount"));
        }
    }

    #[test]
    fn privileged_command_as_user_has_sudo() {
        // When not root, the command should be `sudo -- <program>`.
        if !is_root() {
            let cmd = privileged_command("mount");
            assert_eq!(cmd.get_program(), std::ffi::OsStr::new("sudo"));
        }
    }
}
