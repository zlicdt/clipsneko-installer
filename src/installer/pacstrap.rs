//! Static package loading, dynamic package derivation, pacstrap, and fstab.

use super::pacstrap_progress::PacstrapProgressParser;
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
/// contains a derived dependency. `microcode_package` comes from the live
/// CPU vendor detection and is `None` for unknown vendors.
pub fn package_set(
    static_packages: &[String],
    config: &InstallConfig,
    microcode_package: Option<&str>,
) -> Vec<String> {
    let dynamic = [
        Some(config.kernel_package.as_str()),
        Some(config.headers_package.as_str()),
        Some("linux-firmware"),
        microcode_package,
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

/// Construct the fixed `pacstrap` invocation used by the installer. The
/// package list is dynamic, but the `-P` configuration-copy flag and target
/// root never change.
fn pacstrap_args(packages: &[String]) -> Vec<String> {
    let mut args = vec!["-P".to_string(), TARGET_ROOT.to_string()];
    args.extend(packages.iter().cloned());
    args
}

/// Load every static package file and run pacstrap with `-P` so the Live
/// ISO's pacman configuration and ClipsNeko repository are copied into the
/// target. Files are read in the given order and their contents concatenated
/// before deduplication, so the base set keeps its file order.
///
/// `pacstrap` is pinned to the C locale so its progress markers are
/// parseable regardless of the installer UI language or the launching
/// environment. `on_progress` receives cumulative package-stage percentages
/// in `[PACKAGE_PROGRESS_START, PACKAGE_PROGRESS_END]` as output is parsed.
pub fn install_packages(
    runner: &mut dyn CommandRunner,
    config: &InstallConfig,
    package_files: &[&str],
    on_progress: &mut dyn FnMut(u16),
) -> Result<()> {
    let mut static_packages = Vec::new();
    for path in package_files {
        let contents = std::fs::read_to_string(path).with_context(|| format!("reading {path}"))?;
        static_packages.extend(parse_packages(&contents));
    }
    let packages = package_set(
        &static_packages,
        config,
        crate::util::cpuinfo::microcode_package(),
    );
    let args = pacstrap_args(&packages);
    let mut parser = PacstrapProgressParser::new();
    runner
        .run_streaming("pacstrap", &args, None, &[("LC_ALL", "C")], &mut |chunk| {
            for fraction in parser.push(chunk) {
                on_progress(super::package_percent(fraction));
            }
        })
        .context("pacstrap failed")?;
    on_progress(super::package_percent(parser.finish()));
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
    use crate::installer::{CommandOutput, PACKAGE_PROGRESS_END, PACKAGE_PROGRESS_START};
    use crate::state::{BtrfsRaidMode, NvidiaChoice};
    use crate::util::password::SecretString;

    fn config() -> InstallConfig {
        InstallConfig {
            target_locale: "en_US.UTF-8".to_string(),
            target_locales: vec!["en_US.UTF-8".to_string()],
            keymap: "us".to_string(),
            system_type: crate::state::SystemType::Hyprland,
            dev_tools: false,
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
            package_set(&static_packages, &config(), Some("intel-ucode")),
            [
                "base",
                "linux-firmware",
                "linux-zen",
                "linux-zen-headers",
                "intel-ucode",
                "nvidia-open-dkms"
            ]
        );
    }

    #[test]
    fn unknown_cpu_vendor_adds_no_microcode_package() {
        let static_packages = parse_packages("base\n");
        assert_eq!(
            package_set(&static_packages, &config(), None),
            [
                "base",
                "linux-zen",
                "linux-zen-headers",
                "linux-firmware",
                "nvidia-open-dkms"
            ]
        );
    }

    #[test]
    fn statically_listed_microcode_is_not_duplicated() {
        let static_packages = parse_packages("base\namd-ucode\n");
        assert_eq!(
            package_set(&static_packages, &config(), Some("amd-ucode")),
            [
                "base",
                "amd-ucode",
                "linux-zen",
                "linux-zen-headers",
                "linux-firmware",
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

    #[test]
    fn pacstrap_args_start_with_p_flag_and_target_root() {
        let args = pacstrap_args(&["base".to_string(), "linux".to_string()]);
        assert_eq!(args, ["-P", TARGET_ROOT, "base", "linux"]);
    }

    struct StreamingRunner {
        program: Option<String>,
        args: Vec<String>,
        envs: Vec<(String, String)>,
        chunks: Vec<Vec<u8>>,
    }

    impl CommandRunner for StreamingRunner {
        fn run(
            &mut self,
            _program: &str,
            _args: &[String],
            _stdin: Option<&[u8]>,
        ) -> Result<CommandOutput> {
            Ok(CommandOutput { stdout: Vec::new() })
        }

        fn run_streaming(
            &mut self,
            program: &str,
            args: &[String],
            _stdin: Option<&[u8]>,
            envs: &[(&str, &str)],
            on_output: &mut dyn FnMut(&[u8]),
        ) -> Result<CommandOutput> {
            self.program = Some(program.to_string());
            self.args = args.to_vec();
            self.envs = envs
                .iter()
                .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
                .collect();
            for chunk in &self.chunks {
                on_output(chunk);
            }
            Ok(CommandOutput { stdout: Vec::new() })
        }
    }

    #[test]
    fn install_packages_streams_with_c_locale_and_reports_final_boundary() {
        let path = std::env::temp_dir().join(format!(
            "clipsneko-installer-packages-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&path, "base\nlinux-firmware\n").unwrap();

        let mut runner = StreamingRunner {
            program: None,
            args: Vec::new(),
            envs: Vec::new(),
            chunks: vec![
                b"Packages (2) base linux-firmware\n:: Retrieving packages...\n".to_vec(),
                b"base.pkg.tar.zst downloading...\n".to_vec(),
                b":: Processing package changes...\ninstalling base...\n".to_vec(),
            ],
        };
        let mut percents = Vec::new();
        install_packages(
            &mut runner,
            &config(),
            &[path.to_str().unwrap()],
            &mut |percent| percents.push(percent),
        )
        .unwrap();
        std::fs::remove_file(path).unwrap();

        assert_eq!(runner.program.as_deref(), Some("pacstrap"));
        assert_eq!(runner.envs, [("LC_ALL".to_string(), "C".to_string())]);
        assert_eq!(runner.args[0], "-P");
        assert_eq!(runner.args[1], TARGET_ROOT);
        assert!(runner.args.contains(&"base".to_string()));

        let mut previous = PACKAGE_PROGRESS_START;
        for percent in &percents {
            assert!(*percent >= previous);
            assert!(*percent <= PACKAGE_PROGRESS_END);
            previous = *percent;
        }
        assert_eq!(percents.last(), Some(&PACKAGE_PROGRESS_END));
    }
}
