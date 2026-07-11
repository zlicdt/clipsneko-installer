//! Static package loading, dynamic package derivation, pacstrap, and fstab.

use super::{CommandRunner, InstallConfig, TARGET_ROOT};
use anyhow::{bail, Context, Result};
use std::collections::HashSet;

/// Parse the authoritative one-package-per-line runtime file. Empty lines and
/// comment lines are ignored so packaging may keep the list readable.
pub fn parse_packages(contents: &str) -> Vec<String> {
    contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect()
}

/// Combine static and wizard-derived packages while preserving first-seen
/// order. This avoids passing duplicate packages if the static list already
/// contains a derived dependency.
pub fn package_set(static_packages: &[String], config: &InstallConfig) -> Vec<String> {
    let dynamic = [
        Some(config.kernel_package.as_str()),
        Some(config.headers_package.as_str()),
        Some("linux-firmware"),
        config.nvidia.package_name(),
    ];
    let mut seen = HashSet::new();
    static_packages
        .iter()
        .map(String::as_str)
        .chain(dynamic.into_iter().flatten())
        .filter(|package| seen.insert((*package).to_string()))
        .map(str::to_string)
        .collect()
}

/// Load packages.list and run pacstrap with `-P` so the Live ISO's pacman
/// configuration and ClipsNeko repository are copied into the target.
pub fn install_packages(
    runner: &mut dyn CommandRunner,
    config: &InstallConfig,
    packages_path: &str,
) -> Result<()> {
    let contents = std::fs::read_to_string(packages_path)
        .with_context(|| format!("reading {packages_path}"))?;
    let packages = package_set(&parse_packages(&contents), config);
    let mut args = vec!["-P".to_string(), TARGET_ROOT.to_string()];
    args.extend(packages);
    runner.run("pacstrap", &args, None)?;
    Ok(())
}

/// Require genfstab to contain compressed entries for both fixed btrfs
/// subvolumes. `compress=zstd` may be normalized by the kernel to a default
/// level such as `compress=zstd:3`; the installer preserves that output.
pub fn validate_fstab(contents: &str) -> Result<()> {
    let mut root = false;
    let mut home = false;
    for line in contents.lines().filter(|line| !line.starts_with('#')) {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 4 || fields[2] != "btrfs" {
            continue;
        }
        let options: Vec<&str> = fields[3].split(',').collect();
        let compressed = options
            .iter()
            .any(|option| *option == "compress=zstd" || option.starts_with("compress=zstd:"));
        if compressed && options.contains(&"subvol=/@") {
            root = true;
        }
        if compressed && options.contains(&"subvol=/@home") {
            home = true;
        }
    }
    if !root || !home {
        bail!("genfstab did not produce compressed @ and @home btrfs entries");
    }
    Ok(())
}

/// Generate fstab, validate its btrfs options, and append it without invoking
/// a shell. Privileged tee provides the `>>` behavior under the Live ISO user.
pub fn generate_fstab(runner: &mut dyn CommandRunner) -> Result<()> {
    let output = runner.run(
        "genfstab",
        &["-U".to_string(), TARGET_ROOT.to_string()],
        None,
    )?;
    let contents = std::str::from_utf8(&output.stdout).context("genfstab output is not UTF-8")?;
    validate_fstab(contents)?;
    runner.run(
        "tee",
        &["-a".to_string(), format!("{TARGET_ROOT}/etc/fstab")],
        Some(&output.stdout),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{BtrfsRaidMode, NvidiaChoice};
    use crate::util::password::SecretString;

    fn config() -> InstallConfig {
        InstallConfig {
            target_locale: "en_US.UTF-8".to_string(),
            keymap: "us".to_string(),
            kernel_package: "linux-zen".to_string(),
            headers_package: "linux-zen-headers".to_string(),
            nvidia: NvidiaChoice::NvidiaOpenDkms,
            timezone: "Asia/Shanghai".to_string(),
            username: "user".to_string(),
            password: SecretString::new("secret".to_string()),
            hostname: "host".to_string(),
            esp_partition: "sda1".to_string(),
            esp_needs_format: false,
            target_partitions: vec!["sda2".to_string()],
            raid_mode: None::<BtrfsRaidMode>,
        }
    }

    #[test]
    fn packages_preserve_static_order_and_add_dynamic_without_duplicates() {
        let static_packages = parse_packages("base\n# note\nlinux-firmware\nbase\n");
        assert_eq!(
            package_set(&static_packages, &config()),
            [
                "base",
                "linux-firmware",
                "linux-zen",
                "linux-zen-headers",
                "nvidia-open-dkms"
            ]
        );
    }

    #[test]
    fn fstab_accepts_implicit_and_kernel_normalized_zstd_levels() {
        let fstab = "UUID=a / btrfs rw,compress=zstd,subvol=/@ 0 0\n\
                     UUID=a /home btrfs rw,compress=zstd:3,subvol=/@home 0 0\n";
        validate_fstab(fstab).unwrap();
    }

    #[test]
    fn fstab_rejects_missing_compression_or_subvolume() {
        let fstab = "UUID=a / btrfs rw,subvol=/@ 0 0\n\
                     UUID=a /home btrfs rw,compress=zstd,subvol=/@home 0 0\n";
        assert!(validate_fstab(fstab).is_err());
    }
}
