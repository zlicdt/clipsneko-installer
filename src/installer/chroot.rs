//! Target configuration performed through `arch-chroot /mnt`.

use super::{CommandOutput, CommandRunner, InstallConfig, TARGET_ROOT};
use crate::state::NvidiaChoice;
use anyhow::{bail, Context, Result};
use zeroize::{Zeroize, Zeroizing};

fn chroot_args(program: &str, args: &[&str]) -> Vec<String> {
    std::iter::once(TARGET_ROOT.to_string())
        .chain(std::iter::once(program.to_string()))
        .chain(args.iter().map(|arg| (*arg).to_string()))
        .collect()
}

fn run_chroot(
    runner: &mut dyn CommandRunner,
    program: &str,
    args: &[&str],
    stdin: Option<&[u8]>,
) -> Result<CommandOutput> {
    runner.run("arch-chroot", &chroot_args(program, args), stdin)
}

fn read_target_file(runner: &mut dyn CommandRunner, path: &str) -> Result<String> {
    let output = run_chroot(runner, "cat", &[path], None)?;
    String::from_utf8(output.stdout).with_context(|| format!("{path} is not UTF-8"))
}

fn write_target_file(runner: &mut dyn CommandRunner, path: &str, contents: &[u8]) -> Result<()> {
    run_chroot(runner, "tee", &[path], Some(contents))?;
    Ok(())
}

/// Uncomment the selected locale in locale.gen without changing other lines.
pub fn enable_locale(contents: &str, locale: &str) -> Result<String> {
    let mut found = false;
    let mut output = String::with_capacity(contents.len());
    for line in contents.split_inclusive('\n') {
        let raw = line.strip_suffix('\n').unwrap_or(line);
        let leading_len = raw.len() - raw.trim_start().len();
        let leading = &raw[..leading_len];
        let trimmed = raw.trim_start();
        let candidate = trimmed
            .strip_prefix('#')
            .map(str::trim_start)
            .unwrap_or(trimmed);
        if candidate.split_whitespace().next() == Some(locale) {
            output.push_str(leading);
            output.push_str(candidate);
            found = true;
        } else {
            output.push_str(raw);
        }
        if line.ends_with('\n') {
            output.push('\n');
        }
    }
    if !found {
        bail!("selected locale is absent from target locale.gen");
    }
    Ok(output)
}

/// Enable the standard wheel sudo rule, preserving unrelated sudoers lines.
pub fn enable_wheel_sudo(contents: &str) -> Result<String> {
    const RULE: &str = "%wheel ALL=(ALL:ALL) ALL";
    let mut found = false;
    let mut output = String::with_capacity(contents.len());
    for line in contents.split_inclusive('\n') {
        let raw = line.strip_suffix('\n').unwrap_or(line);
        let candidate = raw
            .trim_start()
            .strip_prefix('#')
            .map(str::trim_start)
            .unwrap_or(raw.trim_start());
        if candidate == RULE {
            output.push_str(RULE);
            found = true;
        } else {
            output.push_str(raw);
        }
        if line.ends_with('\n') {
            output.push('\n');
        }
    }
    if !found {
        bail!("wheel sudo rule is absent from target sudoers");
    }
    Ok(output)
}

/// Remove the standalone `kms` token from the mkinitcpio HOOKS assignment.
pub fn remove_kms_hook(contents: &str) -> Result<String> {
    let mut found = false;
    let mut output = String::with_capacity(contents.len());
    for line in contents.split_inclusive('\n') {
        let raw = line.strip_suffix('\n').unwrap_or(line);
        let trimmed = raw.trim_start();
        if let Some(hooks) = trimmed
            .strip_prefix("HOOKS=(")
            .and_then(|rest| rest.strip_suffix(')'))
        {
            let leading_len = raw.len() - trimmed.len();
            output.push_str(&raw[..leading_len]);
            output.push_str("HOOKS=(");
            output.push_str(
                &hooks
                    .split_whitespace()
                    .filter(|hook| *hook != "kms")
                    .collect::<Vec<_>>()
                    .join(" "),
            );
            output.push(')');
            found = true;
        } else {
            output.push_str(raw);
        }
        if line.ends_with('\n') {
            output.push('\n');
        }
    }
    if !found {
        bail!("HOOKS assignment is absent from target mkinitcpio.conf");
    }
    Ok(output)
}

/// Apply timezone, locale, console, hostname, account, password, and sudoers
/// configuration. The password is sent only to chpasswd stdin and is cleared
/// immediately after chpasswd succeeds.
pub fn configure_target(runner: &mut dyn CommandRunner, config: &mut InstallConfig) -> Result<()> {
    let zoneinfo = format!("/usr/share/zoneinfo/{}", config.timezone);
    run_chroot(runner, "ln", &["-sf", &zoneinfo, "/etc/localtime"], None)?;
    run_chroot(runner, "hwclock", &["--systohc"], None)?;

    let locale_gen = read_target_file(runner, "/etc/locale.gen")?;
    let locale_gen = enable_locale(&locale_gen, &config.target_locale)?;
    write_target_file(runner, "/etc/locale.gen", locale_gen.as_bytes())?;
    run_chroot(runner, "locale-gen", &[], None)?;
    write_target_file(
        runner,
        "/etc/locale.conf",
        format!("LANG={}\n", config.target_locale).as_bytes(),
    )?;
    write_target_file(
        runner,
        "/etc/vconsole.conf",
        format!("KEYMAP={}\n", config.keymap).as_bytes(),
    )?;
    write_target_file(
        runner,
        "/etc/hostname",
        format!("{}\n", config.hostname).as_bytes(),
    )?;
    write_target_file(
        runner,
        "/etc/hosts",
        format!(
            "127.0.0.1 localhost\n::1 localhost\n127.0.1.1 {}\n",
            config.hostname
        )
        .as_bytes(),
    )?;

    run_chroot(
        runner,
        "useradd",
        &["-m", "-G", "wheel", "-s", "/bin/zsh", &config.username],
        None,
    )?;
    let mut credentials = Zeroizing::new(Vec::new());
    credentials.extend_from_slice(config.username.as_bytes());
    credentials.push(b':');
    credentials.extend_from_slice(config.password.expose_secret().as_bytes());
    credentials.push(b'\n');
    run_chroot(runner, "chpasswd", &[], Some(credentials.as_slice()))?;
    credentials.zeroize();
    config.password.clear();

    let sudoers = read_target_file(runner, "/etc/sudoers")?;
    let sudoers = enable_wheel_sudo(&sudoers)?;
    write_target_file(runner, "/etc/sudoers", sudoers.as_bytes())?;
    Ok(())
}

/// Apply the NVIDIA-specific mkinitcpio edit and regenerate all initramfs
/// images. No MODULES entries are added.
pub fn generate_initramfs(runner: &mut dyn CommandRunner, nvidia: NvidiaChoice) -> Result<()> {
    if nvidia != NvidiaChoice::None {
        let config = read_target_file(runner, "/etc/mkinitcpio.conf")?;
        let config = remove_kms_hook(&config)?;
        write_target_file(runner, "/etc/mkinitcpio.conf", config.as_bytes())?;
    }
    run_chroot(runner, "mkinitcpio", &["-P"], None)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::BtrfsRaidMode;
    use crate::util::password::SecretString;

    #[derive(Default)]
    struct PasswordRunner {
        password_seen_on_stdin: bool,
        password_seen_in_args: bool,
    }

    impl CommandRunner for PasswordRunner {
        fn run(
            &mut self,
            _program: &str,
            args: &[String],
            stdin: Option<&[u8]>,
        ) -> Result<CommandOutput> {
            self.password_seen_in_args |= args.iter().any(|arg| arg.contains("secret-value"));
            if args.ends_with(&[TARGET_ROOT.to_string(), "chpasswd".to_string()]) {
                self.password_seen_on_stdin = stdin == Some(b"user:secret-value\n".as_slice());
            }
            let stdout = if args.last().map(String::as_str) == Some("/etc/locale.gen") {
                b"#en_US.UTF-8 UTF-8\n".to_vec()
            } else if args.last().map(String::as_str) == Some("/etc/sudoers") {
                b"# %wheel ALL=(ALL:ALL) ALL\n".to_vec()
            } else {
                Vec::new()
            };
            Ok(CommandOutput { stdout })
        }
    }

    fn install_config() -> InstallConfig {
        InstallConfig {
            target_locale: "en_US.UTF-8".to_string(),
            keymap: "us".to_string(),
            kernel_package: "linux-zen".to_string(),
            headers_package: "linux-zen-headers".to_string(),
            nvidia: NvidiaChoice::None,
            timezone: "UTC".to_string(),
            username: "user".to_string(),
            password: SecretString::new("secret-value".to_string()),
            hostname: "host".to_string(),
            esp_partition: "sda1".to_string(),
            esp_needs_format: false,
            target_partitions: vec!["sda2".to_string()],
            raid_mode: None::<BtrfsRaidMode>,
        }
    }

    #[test]
    fn locale_edit_is_selective_and_idempotent() {
        let input = "#en_US.UTF-8 UTF-8\n# zh_CN.UTF-8 UTF-8\n";
        let once = enable_locale(input, "zh_CN.UTF-8").unwrap();
        assert_eq!(once, "#en_US.UTF-8 UTF-8\nzh_CN.UTF-8 UTF-8\n");
        assert_eq!(enable_locale(&once, "zh_CN.UTF-8").unwrap(), once);
    }

    #[test]
    fn wheel_rule_edit_is_idempotent() {
        let input = "root ALL=(ALL:ALL) ALL\n# %wheel ALL=(ALL:ALL) ALL\n";
        let once = enable_wheel_sudo(input).unwrap();
        assert_eq!(once, "root ALL=(ALL:ALL) ALL\n%wheel ALL=(ALL:ALL) ALL\n");
        assert_eq!(enable_wheel_sudo(&once).unwrap(), once);
    }

    #[test]
    fn kms_hook_removal_only_changes_hooks_assignment() {
        let input = "MODULES=(nvidia)\nHOOKS=(base udev kms autodetect filesystems)\n";
        assert_eq!(
            remove_kms_hook(input).unwrap(),
            "MODULES=(nvidia)\nHOOKS=(base udev autodetect filesystems)\n"
        );
    }

    #[test]
    fn password_uses_only_chpasswd_stdin_and_is_cleared_after_success() {
        let mut runner = PasswordRunner::default();
        let mut config = install_config();
        configure_target(&mut runner, &mut config).unwrap();

        assert!(runner.password_seen_on_stdin);
        assert!(!runner.password_seen_in_args);
        assert!(config.password.is_empty());
    }
}
