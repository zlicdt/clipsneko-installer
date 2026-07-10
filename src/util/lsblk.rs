//! Typed parser and helpers for the fixed `lsblk` schema used by the disk UI.

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::process::Command;

/// GPT EFI System Partition type UUID.
pub const ESP_PARTTYPE: &str = "c12a7328-f81f-11d2-ba4b-00a0c93ec93b";

/// Top-level `lsblk -J` envelope.
#[derive(Debug, Clone, Deserialize)]
pub struct LsblkRoot {
    pub blockdevices: Vec<BlockDevice>,
}

/// One block-device entry from the fixed lsblk column set.
#[derive(Debug, Clone, Deserialize)]
pub struct BlockDevice {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub fstype: Option<String>,
    pub size: u64,
    #[serde(default)]
    pub parttype: Option<String>,
    #[serde(default)]
    pub partlabel: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub tran: Option<String>,
    #[serde(default)]
    pub rm: bool,
    #[serde(default)]
    pub ro: bool,
    #[serde(default)]
    pub mountpoints: Vec<Option<String>>,
    #[serde(default)]
    pub children: Option<Vec<BlockDevice>>,
}

/// Run lsblk with only the columns required by the installer. Command
/// availability, exit success, and this JSON schema are Live ISO invariants;
/// violations are returned as fatal errors rather than an empty disk list.
pub fn list_devices() -> Result<Vec<BlockDevice>> {
    let output = Command::new("lsblk")
        .args([
            "-J",
            "-b",
            "-o",
            "NAME,TYPE,FSTYPE,SIZE,PARTTYPE,PARTLABEL,MODEL,TRAN,RM,RO,MOUNTPOINTS",
        ])
        .output()
        .context("running lsblk")?;
    if !output.status.success() {
        bail!(
            "lsblk failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(parse_lsblk(&output.stdout)?.blockdevices)
}

/// Parse lsblk JSON bytes using the fixed Live ISO schema.
pub fn parse_lsblk(stdout: &[u8]) -> serde_json::Result<LsblkRoot> {
    serde_json::from_slice(stdout)
}

/// Return physical disk entries, excluding zram pseudo-disks.
pub fn flat_disks(devs: &[BlockDevice]) -> Vec<&BlockDevice> {
    devs.iter()
        .filter(|device| device.kind == "disk" && !device.name.starts_with("zram"))
        .collect()
}

/// Recursively return every partition entry.
pub fn flat_parts(devs: &[BlockDevice]) -> Vec<&BlockDevice> {
    let mut out = Vec::new();
    for device in devs {
        walk_parts(device, &mut out);
    }
    out
}

fn walk_parts<'a>(device: &'a BlockDevice, out: &mut Vec<&'a BlockDevice>) {
    if device.kind == "part" {
        out.push(device);
    }
    if let Some(children) = device.children.as_ref() {
        for child in children {
            walk_parts(child, out);
        }
    }
}

/// Whether a disk or any descendant is mounted as Archiso's live boot media.
pub fn is_live_media_disk(device: &BlockDevice) -> bool {
    device.mountpoints.iter().flatten().any(|mountpoint| {
        mountpoint == "/run/archiso/bootmnt" || mountpoint.starts_with("/run/archiso/")
    }) || device
        .children
        .as_deref()
        .is_some_and(|children| children.iter().any(is_live_media_disk))
}

/// Whether a partition carries the GPT EFI System Partition type.
pub fn is_esp_partition(device: &BlockDevice) -> bool {
    device
        .parttype
        .as_deref()
        .is_some_and(|parttype| parttype.eq_ignore_ascii_case(ESP_PARTTYPE))
}

/// Format bytes with compact binary units.
pub fn human_size(bytes: u64) -> String {
    const UNITS: &[(&str, u64)] = &[
        ("T", 1024_u64.pow(4)),
        ("G", 1024_u64.pow(3)),
        ("M", 1024_u64.pow(2)),
        ("K", 1024_u64),
    ];
    for (suffix, scale) in UNITS {
        if bytes >= *scale {
            let value = bytes as f64 / *scale as f64;
            return format!("{value:.1}{suffix}");
        }
    }
    format!("{bytes}B")
}

/// Minimum usable Target capacity; validation requires strictly more.
pub const TARGET_MIN_BYTES: u64 = 20 * 1024 * 1024 * 1024;

#[cfg(test)]
mod tests;
