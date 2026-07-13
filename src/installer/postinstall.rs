//! Target-user customization performed as the final installation command.

use super::{CommandRunner, TARGET_ROOT};
use anyhow::Result;

const INSTALL_DOTFILES_COMMAND: &str = "clipsneko-install-dotfiles -y";

fn chroot_args(username: &str) -> Vec<String> {
    [
        TARGET_ROOT,
        "runuser",
        "--login",
        "--command",
        INSTALL_DOTFILES_COMMAND,
        username,
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

/// Install the packaged dotfiles from a clean login environment as the new user.
pub fn run(runner: &mut dyn CommandRunner, username: &str) -> Result<()> {
    runner.run("arch-chroot", &chroot_args(username), None)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dotfiles_run_uses_the_target_users_login_environment() {
        assert_eq!(
            chroot_args("clipsneko"),
            [
                "/mnt",
                "runuser",
                "--login",
                "--command",
                "clipsneko-install-dotfiles -y",
                "clipsneko"
            ]
        );
    }
}
