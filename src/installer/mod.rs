//! Destructive installation pipeline. The TUI starts this work only after the
//! final explicit confirmation and runs it on a worker thread so rendering can
//! continue. Unit tests exercise command construction through a fake runner;
//! they never execute disk or chroot commands.

pub mod bootloader;
pub mod chroot;
pub mod pacstrap;
pub mod partition;
pub mod postinstall;

use crate::state::{BtrfsRaidMode, InstallerState, NvidiaChoice};
use crate::util::password::SecretString;
use crate::util::process::privileged_command;
use anyhow::{bail, Context, Result};
use std::io::Write;
use std::process::Stdio;
use std::sync::mpsc::Sender;

pub const TARGET_ROOT: &str = "/mnt";
pub const PACKAGES_LIST: &str = "/etc/clipsneko-installer/packages.list";

/// Immutable installation choices transferred from wizard state to the
/// worker thread. The password remains non-Debug and is consumed separately.
pub struct InstallConfig {
    pub target_locale: String,
    pub target_locales: Vec<String>,
    pub keymap: String,
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
            tracing::info!(program, output = %String::from_utf8_lossy(&output.stdout));
        }
        if !output.stderr.is_empty() {
            tracing::info!(program, output = %String::from_utf8_lossy(&output.stderr));
        }
        if !output.status.success() {
            bail!(
                "{program} exited with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
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
    pacstrap::install_packages(&mut runner, &config, PACKAGES_LIST)?;
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
