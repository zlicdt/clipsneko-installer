//! Btrfs/ESP formatting and the fixed subvolume mount layout.

use super::{device_path, CommandRunner, InstallConfig, TARGET_ROOT};
use crate::state::BtrfsRaidMode;
use anyhow::{Context, Result};

const ROOT_OPTIONS: &str = "compress=zstd,subvol=@";
const HOME_OPTIONS: &str = "compress=zstd,subvol=@home";

/// Construct the mkfs.btrfs arguments for the selected targets.
pub fn btrfs_mkfs_args(config: &InstallConfig) -> Result<Vec<String>> {
    let mut args = vec!["-f".to_string()];
    if config.target_partitions.len() > 1 {
        let profile = match config.raid_mode.context("btrfs RAID mode is missing")? {
            BtrfsRaidMode::Raid0 => "raid0",
            BtrfsRaidMode::Raid1 => "raid1",
        };
        args.extend([
            "-d".to_string(),
            profile.to_string(),
            "-m".to_string(),
            "raid1".to_string(),
        ]);
    }
    args.extend(
        config
            .target_partitions
            .iter()
            .map(|part| device_path(part)),
    );
    Ok(args)
}

/// Format every Target as one btrfs filesystem and format the ESP only when
/// the disk step recorded that its current filesystem is not vfat.
pub fn format_targets(runner: &mut dyn CommandRunner, config: &InstallConfig) -> Result<()> {
    runner.run("mkfs.btrfs", &btrfs_mkfs_args(config)?, None)?;
    if config.esp_needs_format {
        runner.run(
            "mkfs.vfat",
            &["-F32".to_string(), device_path(&config.esp_partition)],
            None,
        )?;
    }
    Ok(())
}

/// Create `@` and `@home`, then mount root, home, and the ESP below `/mnt`.
pub fn mount_layout(runner: &mut dyn CommandRunner, config: &InstallConfig) -> Result<()> {
    let root_device = device_path(
        config
            .target_partitions
            .first()
            .context("target partition selection is empty")?,
    );
    runner.run("mkdir", &["-p".to_string(), TARGET_ROOT.to_string()], None)?;
    runner.run(
        "mount",
        &[root_device.clone(), TARGET_ROOT.to_string()],
        None,
    )?;
    for subvolume in ["@", "@home"] {
        runner.run(
            "btrfs",
            &[
                "subvolume".to_string(),
                "create".to_string(),
                format!("{TARGET_ROOT}/{subvolume}"),
            ],
            None,
        )?;
    }
    runner.run("umount", &[TARGET_ROOT.to_string()], None)?;
    runner.run(
        "mount",
        &[
            "-o".to_string(),
            ROOT_OPTIONS.to_string(),
            root_device.clone(),
            TARGET_ROOT.to_string(),
        ],
        None,
    )?;
    runner.run(
        "mkdir",
        &[
            "-p".to_string(),
            format!("{TARGET_ROOT}/home"),
            format!("{TARGET_ROOT}/boot/efi"),
        ],
        None,
    )?;
    runner.run(
        "mount",
        &[
            "-o".to_string(),
            HOME_OPTIONS.to_string(),
            root_device,
            format!("{TARGET_ROOT}/home"),
        ],
        None,
    )?;
    runner.run(
        "mount",
        &[
            device_path(&config.esp_partition),
            format!("{TARGET_ROOT}/boot/efi"),
        ],
        None,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::NvidiaChoice;
    use crate::util::password::SecretString;

    #[derive(Default)]
    struct RecordingRunner {
        programs: Vec<String>,
    }

    impl CommandRunner for RecordingRunner {
        fn run(
            &mut self,
            program: &str,
            _args: &[String],
            _stdin: Option<&[u8]>,
        ) -> Result<super::super::CommandOutput> {
            self.programs.push(program.to_string());
            Ok(super::super::CommandOutput { stdout: Vec::new() })
        }
    }

    fn config(targets: &[&str], raid_mode: Option<BtrfsRaidMode>) -> InstallConfig {
        InstallConfig {
            target_locale: "en_US.UTF-8".to_string(),
            keymap: "us".to_string(),
            kernel_package: "linux-zen".to_string(),
            headers_package: "linux-zen-headers".to_string(),
            nvidia: NvidiaChoice::None,
            timezone: "UTC".to_string(),
            username: "user".to_string(),
            password: SecretString::new("secret".to_string()),
            hostname: "host".to_string(),
            esp_partition: "sda1".to_string(),
            esp_needs_format: false,
            target_partitions: targets.iter().map(|value| (*value).to_string()).collect(),
            raid_mode,
        }
    }

    #[test]
    fn single_target_mkfs_has_no_raid_arguments() {
        assert_eq!(
            btrfs_mkfs_args(&config(&["sda2"], None)).unwrap(),
            ["-f", "/dev/sda2"]
        );
    }

    #[test]
    fn multi_target_uses_selected_data_and_raid1_metadata() {
        assert_eq!(
            btrfs_mkfs_args(&config(
                &["nvme0n1p2", "nvme1n1p2"],
                Some(BtrfsRaidMode::Raid0)
            ))
            .unwrap(),
            [
                "-f",
                "-d",
                "raid0",
                "-m",
                "raid1",
                "/dev/nvme0n1p2",
                "/dev/nvme1n1p2"
            ]
        );
    }

    #[test]
    fn mount_options_do_not_specify_a_compression_level() {
        assert_eq!(ROOT_OPTIONS, "compress=zstd,subvol=@");
        assert_eq!(HOME_OPTIONS, "compress=zstd,subvol=@home");
    }

    #[test]
    fn esp_format_decision_is_applied_at_format_time() {
        let mut runner = RecordingRunner::default();
        let mut reused = config(&["sda2"], None);
        format_targets(&mut runner, &reused).unwrap();
        assert_eq!(runner.programs, ["mkfs.btrfs"]);

        runner.programs.clear();
        reused.esp_needs_format = true;
        format_targets(&mut runner, &reused).unwrap();
        assert_eq!(runner.programs, ["mkfs.btrfs", "mkfs.vfat"]);
    }
}
