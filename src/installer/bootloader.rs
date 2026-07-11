//! GRUB installation and target service enablement.

use super::{CommandRunner, TARGET_ROOT};
use anyhow::Result;

fn chroot_args(program: &str, args: &[&str]) -> Vec<String> {
    std::iter::once(TARGET_ROOT.to_string())
        .chain(std::iter::once(program.to_string()))
        .chain(args.iter().map(|arg| (*arg).to_string()))
        .collect()
}

/// Install GRUB, generate grub.cfg, and enable NetworkManager in the target.
pub fn install(runner: &mut dyn CommandRunner) -> Result<()> {
    runner.run(
        "arch-chroot",
        &chroot_args(
            "grub-install",
            &[
                "--target=x86_64-efi",
                "--efi-directory=/boot/efi",
                "--bootloader-id=clipsneko",
            ],
        ),
        None,
    )?;
    runner.run(
        "arch-chroot",
        &chroot_args("grub-mkconfig", &["-o", "/boot/grub/grub.cfg"]),
        None,
    )?;
    runner.run(
        "arch-chroot",
        &chroot_args("systemctl", &["enable", "NetworkManager"]),
        None,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootloader_arguments_match_the_uefi_design() {
        assert_eq!(
            chroot_args(
                "grub-install",
                &[
                    "--target=x86_64-efi",
                    "--efi-directory=/boot/efi",
                    "--bootloader-id=clipsneko"
                ]
            ),
            [
                "/mnt",
                "grub-install",
                "--target=x86_64-efi",
                "--efi-directory=/boot/efi",
                "--bootloader-id=clipsneko"
            ]
        );
        assert_eq!(
            chroot_args("grub-mkconfig", &["-o", "/boot/grub/grub.cfg"]),
            ["/mnt", "grub-mkconfig", "-o", "/boot/grub/grub.cfg"]
        );
    }
}
