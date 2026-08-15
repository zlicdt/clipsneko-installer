//! Destructive installation pipeline. The TUI starts this work only after the
//! final explicit confirmation and runs it on a worker thread so rendering can
//! continue. Unit tests exercise command construction through a fake runner;
//! they never execute disk or chroot commands.

pub mod bootloader;
pub mod chroot;
pub mod pacstrap;
pub mod partition;
pub mod postinstall;

use crate::state::{BtrfsRaidMode, InstallerState, NvidiaChoice, SystemType};
use crate::util::password::SecretString;
use crate::util::process::privileged_command;
use anyhow::{bail, Context, Result};
use std::io::Write;
use std::process::Stdio;
use std::sync::mpsc::Sender;

pub const TARGET_ROOT: &str = "/mnt";
/// Static package files shipped by the Live ISO under
/// `/etc/clipsneko-installer`. The base set is always installed; the system
/// type appends one more file and the development-tools choice appends
/// `packages.dev`.
pub const PACKAGES_BASE: &str = "/etc/clipsneko-installer/packages.base";
pub const PACKAGES_DEV: &str = "/etc/clipsneko-installer/packages.dev";
pub const PACKAGES_KDE: &str = "/etc/clipsneko-installer/packages.kde";
pub const PACKAGES_HYPR: &str = "/etc/clipsneko-installer/packages.hypr";

/// Static package files selected by the wizard choices. The base set always
/// comes first so its packages keep their file order during deduplication.
pub fn package_files(config: &InstallConfig) -> Vec<&'static str> {
    let mut files = vec![PACKAGES_BASE];
    match config.system_type {
        SystemType::Server => {}
        SystemType::Kde => files.push(PACKAGES_KDE),
        SystemType::Hyprland => files.push(PACKAGES_HYPR),
    }
    if config.dev_tools {
        files.push(PACKAGES_DEV);
    }
    files
}

/// Immutable installation choices transferred from wizard state to the
/// worker thread. The password remains non-Debug and is consumed separately.
pub struct InstallConfig {
    pub target_locale: String,
    pub target_locales: Vec<String>,
    pub keymap: String,
    pub system_type: SystemType,
    pub dev_tools: bool,
    pub kernel_package: String,
    pub headers_package: String,
    pub nvidia: NvidiaChoice,
    pub timezone: String,
    pub username: String,
    pub password: SecretString,
    pub hostname: String,
    pub esp_partition: String,
    pub esp_needs_format: bool,
    pub target_partitions: Vec<String>,
    pub raid_mode: Option<BtrfsRaidMode>,
}

impl InstallConfig {
    /// Validate and consume the complete wizard state. Taking the password
    /// prevents navigation back into editable steps after installation starts.
    pub fn take_from_state(state: &mut InstallerState) -> Result<Self> {
        let user = state
            .user
            .as_ref()
            .context("user configuration is missing")?;
        if !user.password_set || user.username.is_empty() {
            bail!("user configuration is incomplete");
        }
        let password = state
            .user_password
            .take()
            .context("confirmed password is missing")?;
        if password.is_empty() {
            bail!("confirmed password is empty");
        }
        let targets = state.disk.target_partitions.clone();
        if targets.is_empty() {
            bail!("target partition selection is empty");
        }
        if targets.len() > 1 && state.disk.raid_mode.is_none() {
            bail!("btrfs RAID mode is missing");
        }

        let target_locale = state
            .target_locale
            .clone()
            .context("default target locale is missing")?;
        if state.target_locales.is_empty() {
            bail!("target locale selection is empty");
        }
        if !state.target_locales.contains(&target_locale) {
            bail!("default target locale is not enabled");
        }

        Ok(Self {
            target_locale,
            target_locales: state.target_locales.clone(),
            keymap: state.keymap.clone().context("keymap is missing")?,
            system_type: state.system_type.context("system type choice is missing")?,
            dev_tools: state.dev_tools,
            kernel_package: state
                .kernel
                .context("kernel choice is missing")?
                .package_name()
                .to_string(),
            headers_package: state
                .kernel
                .context("kernel choice is missing")?
                .headers_package_name()
                .to_string(),
            nvidia: state.nvidia,
            timezone: state.timezone.clone().context("timezone is missing")?,
            username: user.username.clone(),
            password,
            hostname: state.hostname.clone().context("hostname is missing")?,
            esp_partition: state
                .disk
                .esp_partition
                .clone()
                .context("ESP selection is missing")?,
            esp_needs_format: state
                .disk
                .esp_needs_format
                .context("ESP format decision is missing")?,
            target_partitions: targets,
            raid_mode: state.disk.raid_mode,
        })
    }
}

/// Captured command output used by configuration transforms and tests.
pub struct CommandOutput {
    pub stdout: Vec<u8>,
}

/// Remove one ANSI escape sequence class at a time: CSI (`ESC [ … final`),
/// OSC (`ESC ] … BEL` or `ESC ] … ESC \`), and two-byte escapes. Tabs become
/// spaces and remaining control characters are dropped, so the result renders
/// cleanly in the log viewer's paragraph.
fn strip_ansi(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars();
    while let Some(c) = chars.next() {
        match c {
            '\u{1b}' => match chars.next() {
                Some('[') => {
                    for c in chars.by_ref() {
                        if ('\u{40}'..='\u{7e}').contains(&c) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    while let Some(c) = chars.next() {
                        if c == '\u{07}' {
                            break;
                        }
                        if c == '\u{1b}' {
                            chars.next();
                            break;
                        }
                    }
                }
                _ => {}
            },
            '\t' => out.push_str("    "),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

/// Reduce raw subprocess output to plain text fit for the log file and the
/// in-app log viewer: on every line only the final `\r`-redrawn frame
/// survives (pacman repaints download bars this way), ANSI sequences are
/// stripped, and blank lines are dropped.
fn sanitize_output(raw: &[u8]) -> String {
    let text = String::from_utf8_lossy(raw);
    let mut lines = Vec::new();
    for line in text.lines() {
        let last_frame = line.rsplit('\r').next().unwrap_or(line);
        let stripped = strip_ansi(last_frame);
        let trimmed = stripped.trim_end();
        if !trimmed.is_empty() {
            lines.push(trimmed.to_string());
        }
    }
    lines.join("\n")
}

/// Minimal command seam for destructive operations. Implementations must not
/// log stdin because it may contain the account password.
pub trait CommandRunner {
    fn run(
        &mut self,
        program: &str,
        args: &[String],
        stdin: Option<&[u8]>,
    ) -> Result<CommandOutput>;
}

/// Real privileged command runner used only by the installation worker.
pub struct SystemRunner;

impl CommandRunner for SystemRunner {
    fn run(
        &mut self,
        program: &str,
        args: &[String],
        stdin: Option<&[u8]>,
    ) -> Result<CommandOutput> {
        tracing::info!(program, args = ?args, "running install command");
        let mut command = privileged_command(program);
        command
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if stdin.is_some() {
            command.stdin(Stdio::piped());
        }
        let mut child = command
            .spawn()
            .with_context(|| format!("spawning {program}"))?;
        if let Some(input) = stdin {
            child
                .stdin
                .take()
                .context("command stdin was not piped")?
                .write_all(input)
                .with_context(|| format!("writing stdin for {program}"))?;
        }
        let output = child
            .wait_with_output()
            .with_context(|| format!("waiting for {program}"))?;
        if !output.stdout.is_empty() {
            tracing::info!(program, output = %sanitize_output(&output.stdout));
        }
        if !output.stderr.is_empty() {
            tracing::info!(program, output = %sanitize_output(&output.stderr));
        }
        if !output.status.success() {
            bail!(
                "{program} exited with {}: {}",
                output.status,
                sanitize_output(&output.stderr)
            );
        }
        Ok(CommandOutput {
            stdout: output.stdout,
        })
    }
}

/// Coarse-grained progress values shown by the installation spinner.
#[derive(Clone, Copy)]
pub enum InstallProgress {
    Formatting,
    Mounting,
    Packages,
    Fstab,
    TargetConfig,
    Initramfs,
    Bootloader,
    Postinstall,
}

impl InstallProgress {
    /// Cumulative completion percentage reached when this stage begins. The
    /// fixed weights (5/2/60/2/10/10/6/5) reflect typical stage durations:
    /// package installation dominates at 60% and the remaining stages share
    /// the rest. The final stage begins at a full 100%.
    pub fn percent(self) -> u16 {
        match self {
            Self::Formatting => 5,
            Self::Mounting => 7,
            Self::Packages => 67,
            Self::Fstab => 69,
            Self::TargetConfig => 79,
            Self::Initramfs => 89,
            Self::Bootloader => 95,
            Self::Postinstall => 100,
        }
    }
}

/// Messages sent from the worker to the TUI.
pub enum WorkerMessage {
    Progress(InstallProgress),
    Complete,
    Failed(String),
    RebootIssued,
}

fn report(sender: &Sender<WorkerMessage>, progress: InstallProgress) {
    let _ = sender.send(WorkerMessage::Progress(progress));
}

/// Run the complete install pipeline. Any error stops at the failing command;
/// rollback is intentionally deferred and mounted filesystems are preserved.
pub fn run_install(mut config: InstallConfig, sender: &Sender<WorkerMessage>) -> Result<()> {
    let mut runner = SystemRunner;
    report(sender, InstallProgress::Formatting);
    partition::format_targets(&mut runner, &config)?;
    report(sender, InstallProgress::Mounting);
    partition::mount_layout(&mut runner, &config)?;
    report(sender, InstallProgress::Packages);
    pacstrap::install_packages(&mut runner, &config, &package_files(&config))?;
    report(sender, InstallProgress::Fstab);
    pacstrap::generate_fstab(&mut runner)?;
    report(sender, InstallProgress::TargetConfig);
    chroot::configure_target(&mut runner, &mut config)?;
    report(sender, InstallProgress::Initramfs);
    chroot::generate_initramfs(&mut runner, config.nvidia)?;
    report(sender, InstallProgress::Bootloader);
    bootloader::install(&mut runner)?;
    report(sender, InstallProgress::Postinstall);
    postinstall::run(&mut runner, &config.username)?;
    Ok(())
}

/// Unmount the installed system and request reboot. Both commands use the
/// shared privileged-command path, which invokes sudo for the Live ISO user.
pub fn unmount_and_reboot() -> Result<()> {
    let mut runner = SystemRunner;
    runner.run("umount", &["-R".to_string(), TARGET_ROOT.to_string()], None)?;
    runner.run("reboot", &[], None)?;
    Ok(())
}

/// Convert an lsblk device name to the absolute path accepted by filesystem
/// tools. No shell is involved, so the name remains a single argument.
pub fn device_path(name: &str) -> String {
    format!("/dev/{name}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_files_follow_system_type_and_dev_tools() {
        let mut config = InstallConfig {
            target_locale: "en_US.UTF-8".to_string(),
            target_locales: vec!["en_US.UTF-8".to_string()],
            keymap: "us".to_string(),
            system_type: SystemType::Server,
            dev_tools: false,
            kernel_package: "linux".to_string(),
            headers_package: "linux-headers".to_string(),
            nvidia: NvidiaChoice::None,
            timezone: "UTC".to_string(),
            username: "user".to_string(),
            password: SecretString::new("secret".to_string()),
            hostname: "host".to_string(),
            esp_partition: "sda1".to_string(),
            esp_needs_format: false,
            target_partitions: vec!["sda2".to_string()],
            raid_mode: None,
        };
        assert_eq!(package_files(&config), [PACKAGES_BASE]);

        config.dev_tools = true;
        assert_eq!(package_files(&config), [PACKAGES_BASE, PACKAGES_DEV]);

        config.system_type = SystemType::Kde;
        assert_eq!(
            package_files(&config),
            [PACKAGES_BASE, PACKAGES_KDE, PACKAGES_DEV]
        );

        config.system_type = SystemType::Hyprland;
        config.dev_tools = false;
        assert_eq!(package_files(&config), [PACKAGES_BASE, PACKAGES_HYPR]);
    }

    #[test]
    fn sanitize_output_strips_ansi_and_resolves_progress_frames() {
        let raw = b"\x1b[1;33mwarning:\x1b[0m something\n 10%\r 50%\r100% done\r\ncol1\tcol2\n\n";
        assert_eq!(
            sanitize_output(raw),
            "warning: something\n100% done\ncol1    col2"
        );
    }

    #[test]
    fn sanitize_output_drops_osc_sequences_and_lone_controls() {
        let raw = b"\x1b]0;window title\x07kept\x08 text\n";
        assert_eq!(sanitize_output(raw), "kept text");
    }

    #[test]
    fn progress_percentages_are_cumulative_and_end_at_100() {
        let ordered = [
            InstallProgress::Formatting,
            InstallProgress::Mounting,
            InstallProgress::Packages,
            InstallProgress::Fstab,
            InstallProgress::TargetConfig,
            InstallProgress::Initramfs,
            InstallProgress::Bootloader,
            InstallProgress::Postinstall,
        ];
        let mut previous = 0;
        for stage in ordered {
            let percent = stage.percent();
            assert!(percent > previous);
            previous = percent;
        }
        assert_eq!(previous, 100);
    }
}
