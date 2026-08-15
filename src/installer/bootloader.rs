//! GRUB installation and target service enablement.

use super::chroot::{read_target_file, write_target_file};
use super::{CommandRunner, TARGET_ROOT};
use crate::state::SystemType;
use anyhow::{bail, Result};

fn chroot_args(program: &str, args: &[&str]) -> Vec<String> {
    std::iter::once(TARGET_ROOT.to_string())
        .chain(std::iter::once(program.to_string()))
        .chain(args.iter().map(|arg| (*arg).to_string()))
        .collect()
}

/// Enable os-prober so `grub-mkconfig` adds boot entries for other installed
/// operating systems. Arch's stock `/etc/default/grub` ships the toggle as
/// the commented line `#GRUB_DISABLE_OS_PROBER=false`; uncomment it while
/// preserving unrelated lines. The stock line is a target-package invariant,
/// so its absence is a fatal error.
pub fn enable_os_prober(contents: &str) -> Result<String> {
    const SETTING: &str = "GRUB_DISABLE_OS_PROBER=false";
    let mut found = false;
    let mut output = String::with_capacity(contents.len());
    for line in contents.split_inclusive('\n') {
        let raw = line.strip_suffix('\n').unwrap_or(line);
        let candidate = raw
            .trim_start()
            .strip_prefix('#')
            .map(str::trim_start)
            .unwrap_or(raw.trim_start());
        if candidate == SETTING {
            output.push_str(SETTING);
            found = true;
        } else {
            output.push_str(raw);
        }
        if line.ends_with('\n') {
            output.push('\n');
        }
    }
    if !found {
        bail!("GRUB_DISABLE_OS_PROBER toggle is absent from target /etc/default/grub");
    }
    Ok(output)
}

/// Enable os-prober, install GRUB, generate grub.cfg, and enable
/// NetworkManager in the target.
/// KDE system type additionally enables sddm.service
pub fn install(runner: &mut dyn CommandRunner, system_type: SystemType) -> Result<()> {
    let grub_defaults = read_target_file(runner, "/etc/default/grub")?;
    let grub_defaults = enable_os_prober(&grub_defaults)?;
    write_target_file(runner, "/etc/default/grub", grub_defaults.as_bytes())?;
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
    if system_type == SystemType::Kde {
        runner.run(
            "arch-chroot",
            &chroot_args("systemctl", &["enable", "sddm"]),
            None,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::installer::CommandOutput;

    #[test]
    fn os_prober_toggle_is_uncommented_idempotently() {
        let input = "GRUB_TIMEOUT=5\n#GRUB_DISABLE_OS_PROBER=false\n";
        let once = enable_os_prober(input).unwrap();
        assert_eq!(once, "GRUB_TIMEOUT=5\nGRUB_DISABLE_OS_PROBER=false\n");
        assert_eq!(enable_os_prober(&once).unwrap(), once);
    }

    #[test]
    fn missing_os_prober_toggle_is_a_fatal_invariant() {
        assert!(enable_os_prober("GRUB_TIMEOUT=5\n").is_err());
    }

    #[derive(Default)]
    struct GrubRunner {
        written_grub_defaults: Option<String>,
        commands: Vec<Vec<String>>,
    }

    impl CommandRunner for GrubRunner {
        fn run(
            &mut self,
            program: &str,
            args: &[String],
            stdin: Option<&[u8]>,
        ) -> Result<CommandOutput> {
            assert_eq!(program, "arch-chroot");
            self.commands.push(args.to_vec());
            let subcommand = args.get(1).map(String::as_str);
            let mut stdout = Vec::new();
            if subcommand == Some("cat")
                && args.last().map(String::as_str) == Some("/etc/default/grub")
            {
                stdout = b"GRUB_TIMEOUT=5\n#GRUB_DISABLE_OS_PROBER=false\n".to_vec();
            }
            if subcommand == Some("tee")
                && args.last().map(String::as_str) == Some("/etc/default/grub")
            {
                self.written_grub_defaults =
                    stdin.map(|bytes| String::from_utf8(bytes.to_vec()).unwrap());
            }
            Ok(CommandOutput { stdout })
        }
    }

    #[test]
    fn install_writes_the_enabled_os_prober_toggle_to_the_target() {
        let mut runner = GrubRunner::default();
        install(&mut runner, SystemType::Hyprland).unwrap();
        assert_eq!(
            runner.written_grub_defaults.as_deref(),
            Some("GRUB_TIMEOUT=5\nGRUB_DISABLE_OS_PROBER=false\n")
        );
    }

    #[test]
    fn kde_enables_sddm_service() {
        let mut runner = GrubRunner::default();
        install(&mut runner, SystemType::Kde).unwrap();
        let has_enable = |service: &str| {
            runner
                .commands
                .iter()
                .any(|args| args == &["/mnt", "systemctl", "enable", service])
        };
        assert!(has_enable("NetworkManager"));
        assert!(has_enable("sddm"));
    }

    #[test]
    fn non_kde_system_types_do_not_enable_sddm() {
        for system_type in [SystemType::Server, SystemType::Hyprland] {
            let mut runner = GrubRunner::default();
            install(&mut runner, system_type).unwrap();
            assert!(
                !runner
                    .commands
                    .iter()
                    .any(|args| args == &["/mnt", "systemctl", "enable", "sddm"]),
                "sddm must stay disabled for {system_type:?}"
            );
        }
    }

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
