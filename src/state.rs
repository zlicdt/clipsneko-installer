//! Installer state — the single source of truth for every choice the user
//! makes across the wizard. Each step reads from and writes to this struct.
//!
//! For now the fields are unused (the stages are stubs); they are declared
//! up front so the data model is documented and ready for the real step
//! logic. `#[allow(dead_code)]` silences the linter until each step lands.

#![allow(dead_code)]

use crate::i18n::UiLang;

/// All wizard choices. `Option` fields mean "not yet answered".
#[derive(Debug, Default)]
pub struct InstallerState {
    pub ui_lang: Option<UiLang>,
    pub keymap: Option<String>,
    pub network_ok: bool,
    pub mirror_lines: Vec<String>,
    pub disk: DiskState,
    pub kernel: Option<KernelChoice>,
    pub nvidia: NvidiaChoice,
    pub timezone: Option<String>,
    pub user: Option<UserInfo>,
    pub hostname: Option<String>,
}

/// Partition / mount role assignments produced by the disk step.
#[derive(Debug, Default)]
pub struct DiskState {
    pub main_disk: Option<String>,
    pub esp_partition: Option<String>,
    pub root_partition: Option<String>,
    /// (partition device, mount point) pairs for additional mounts such as
    /// `/home` on a second disk.
    pub extra_mounts: Vec<(String, String)>,
}

/// Kernel package chosen in step 6.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KernelChoice {
    Linux,
    LinuxLts,
    LinuxZen,
    LinuxHardened,
}

/// Nvidia package chosen in step 7.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NvidiaChoice {
    #[default]
    None,
    Nvidia,
    NvidiaDkms,
    NvidiaOpenDkms,
    NvidiaLts,
}

/// User account info collected in step 9. The password itself is never
/// stored here; `password_set` only records that a password was confirmed.
#[derive(Debug, Default)]
pub struct UserInfo {
    pub username: String,
    pub gecos: String,
    pub password_set: bool,
}
