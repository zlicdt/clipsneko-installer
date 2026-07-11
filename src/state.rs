//! Installer state — the single source of truth for every choice the user
//! makes across the wizard. Each step reads from and writes to this struct.
//!
//! Fields for later install stages remain declared before their consumers are
//! implemented. `#[allow(dead_code)]` silences the linter until those stages
//! land.

#![allow(dead_code)]

use crate::i18n::UiLang;
use crate::util::password::SecretString;

/// All wizard choices. `Option` fields mean "not yet answered".
#[derive(Default)]
pub struct InstallerState {
    pub ui_lang: Option<UiLang>,
    pub target_locale: Option<String>,
    pub keymap: Option<String>,
    pub network_ok: bool,
    pub mirror_lines: Vec<String>,
    pub disk: DiskState,
    pub kernel: Option<KernelChoice>,
    pub nvidia: NvidiaChoice,
    pub timezone: Option<String>,
    pub user: Option<UserInfo>,
    pub user_password: Option<SecretString>,
    pub hostname: Option<String>,
}

/// Partition role assignments produced by the disk step. Only ESP and one
/// or more Target partitions are tracked — there is no extra-partition /
/// extra-mount mapping in v0.1 (see `design.md` §4 step 5). Two or more Target
/// partitions enable btrfs RAID at format time (see `design.md` §5).
#[derive(Debug, Default)]
pub struct DiskState {
    pub esp_partition: Option<String>,
    pub target_partitions: Vec<String>,
    pub raid_mode: Option<BtrfsRaidMode>,
}

/// Btrfs data profile used when multiple Target partitions are selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BtrfsRaidMode {
    Raid0,
    Raid1,
}

/// Kernel package chosen in step 6.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KernelChoice {
    Linux,
    LinuxLts,
    #[default]
    LinuxZen,
    LinuxHardened,
}

impl KernelChoice {
    /// All kernel choices in their UI display order.
    pub const ALL: [Self; 4] = [
        Self::Linux,
        Self::LinuxLts,
        Self::LinuxZen,
        Self::LinuxHardened,
    ];

    /// Kernel package passed to pacstrap for this choice.
    pub const fn package_name(self) -> &'static str {
        match self {
            Self::Linux => "linux",
            Self::LinuxLts => "linux-lts",
            Self::LinuxZen => "linux-zen",
            Self::LinuxHardened => "linux-hardened",
        }
    }

    /// Matching headers package installed with this kernel.
    pub const fn headers_package_name(self) -> &'static str {
        match self {
            Self::Linux => "linux-headers",
            Self::LinuxLts => "linux-lts-headers",
            Self::LinuxZen => "linux-zen-headers",
            Self::LinuxHardened => "linux-hardened-headers",
        }
    }
}

/// NVIDIA package chosen in step 7.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NvidiaChoice {
    None,
    NvidiaOpen,
    #[default]
    NvidiaOpenDkms,
    NvidiaOpenLts,
}

impl NvidiaChoice {
    /// All NVIDIA choices in their UI display order.
    pub const ALL: [Self; 4] = [
        Self::None,
        Self::NvidiaOpen,
        Self::NvidiaOpenLts,
        Self::NvidiaOpenDkms,
    ];

    /// Package added to pacstrap, or `None` when no NVIDIA driver is wanted.
    pub const fn package_name(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::NvidiaOpen => Some("nvidia-open"),
            Self::NvidiaOpenDkms => Some("nvidia-open-dkms"),
            Self::NvidiaOpenLts => Some("nvidia-open-lts"),
        }
    }

    /// Whether this choice supports the selected kernel.
    pub const fn is_compatible_with(self, kernel: KernelChoice) -> bool {
        match self {
            Self::None | Self::NvidiaOpenDkms => true,
            Self::NvidiaOpen => matches!(kernel, KernelChoice::Linux),
            Self::NvidiaOpenLts => matches!(kernel, KernelChoice::LinuxLts),
        }
    }
}

/// Non-secret user account info collected in step 9. The password lives in a
/// separate non-Debug, zeroizing secret wrapper until chpasswd consumes it;
/// `password_set` only records that confirmation succeeded.
#[derive(Debug, Default)]
pub struct UserInfo {
    pub username: String,
    pub password_set: bool,
}
