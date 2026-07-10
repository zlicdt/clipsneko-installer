//! `lsblk -J -O -b` JSON parser and helpers.
//!
//! The disk step shells out to `lsblk` to enumerate block devices and
//! partitions. JSON output (the `-J` flag) is parsed into a typed
//! `BlockDevice` tree via `serde`. The `-O` flag requests every column
//! `lsblk` knows; the `-b` flag makes `size` a byte count (number) instead of
//! a human-readable string so thresholds like "Target total > 20 GiB" can be
//! computed exactly.
//!
//! Only the fields the installer cares about are captured; everything else
//! `lsblk` emits is dropped on the floor by virtue of not being declared on
//! the struct (`serde` ignores unknown fields by default).
//!
//! The shell-out wrapper (`list_devices`) is a thin `Command` shell and is
//! not unit-tested; the pure parsing/flattening helpers are.

use serde::Deserialize;
use std::process::Command;

/// ESP partition-type UUID (`C12A7328-F81F-11D2-BA4B-00A0C93EC93B` as stored
/// by `lsblk` in lowercase). A partition with this `parttype` is the EFI
/// System Partition per the GPT spec. Held as a named constant for parity with
/// the GPT spec and for tests; not yet consulted at runtime (the disk step
/// asks the user to assign roles by hand), hence `allow(dead_code)`.
#[allow(dead_code)]
pub const ESP_PARTTYPE: &str = "c12a7328-f81f-11d2-ba4b-00a0c93ec93b";

/// Top-level `lsblk -J` envelope: `{ "blockdevices": [ ... ] }`.
#[derive(Debug, Clone, Deserialize)]
pub struct LsblkRoot {
    pub blockdevices: Vec<BlockDevice>,
}

/// One entry in the `lsblk` tree. Disks have `children` partitions; partitions
/// do not. Fields marked `Option` are emitted as `null` by `lsblk` when the
/// value is missing, which serde turns into `None`. Some fields (`pttype`,
/// `parttype`, `partlabel`) are kept because the GPT spec / future steps may
/// use them but are not consulted by the M2 disk step; they are marked
/// `allow(dead_code)` to silence the linter outside the test build.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct BlockDevice {
    pub name: String,
    /// `lsblk` reports device kind here (`disk`, `part`, `rom`, `lvm`, ...).
    /// Renamed away from the Rust keyword `type`.
    #[serde(rename = "type")]
    pub kind: String,
    /// Filesystem type currently on the device (`vfat`, `btrfs`, ...), or
    /// `None` when the device carries no filesystem.
    #[serde(default)]
    pub fstype: Option<String>,
    /// Device size in bytes. `lsblk -b` emits a bare number; old/forked
    /// `lsblk` builds sometimes quote it as a string — both are accepted.
    #[serde(default, deserialize_with = "de_flex_u64")]
    pub size: u64,
    /// Partition-table type on a disk (`gpt`, `dos`, ...), `None` on parts.
    #[serde(default)]
    pub pttype: Option<String>,
    /// Partition-type UUID on a partition; the ESP UUID is `ESP_PARTTYPE`.
    #[serde(default)]
    pub parttype: Option<String>,
    /// Optional GPT partition label.
    #[serde(default)]
    pub partlabel: Option<String>,
    /// Nested partitions for a disk; `None` for leaves.
    #[serde(default)]
    pub children: Option<Vec<BlockDevice>>,
}

/// Deserialize a `u64` that `lsblk` may serialize as either a JSON number or a
/// JSON string. `-b` makes `size` numeric so the number path is the common
/// one; the string path is defensive for forked `lsblk` builds.
fn de_flex_u64<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v = serde_json::Value::deserialize(deserializer)?;
    match v {
        serde_json::Value::Number(n) => n
            .as_u64()
            .ok_or_else(|| serde::de::Error::custom("size number not a u64")),
        serde_json::Value::String(s) => s
            .parse::<u64>()
            .map_err(|_| serde::de::Error::custom("size string not a u64")),
        serde_json::Value::Null => Ok(0),
        _ => Err(serde::de::Error::custom("size unexpected JSON type")),
    }
}

/// Run `lsblk -J -O -b` and parse the JSON into the typed tree. Returns an
/// empty tree on a spawn/parse failure (the disk step treats "no devices" the
/// same as "command failed"); the failure is logged via `tracing`.
pub fn list_devices() -> Vec<BlockDevice> {
    let out = Command::new("lsblk").args(["-J", "-O", "-b"]).output();
    match out {
        Ok(o) if o.status.success() => match parse_lsblk(&String::from_utf8_lossy(&o.stdout)) {
            Some(root) => root.blockdevices,
            None => {
                tracing::error!("lsblk JSON parse failed");
                Vec::new()
            }
        },
        Ok(o) => {
            tracing::warn!(
                "lsblk exited non-zero: {}; stderr: {}",
                o.status,
                String::from_utf8_lossy(&o.stderr).trim()
            );
            Vec::new()
        }
        Err(e) => {
            tracing::error!("lsblk spawn failed: {e}");
            Vec::new()
        }
    }
}

/// Parse `lsblk -J -O -b` stdout into the typed envelope. Returns `None` on
/// a JSON deserialization error.
pub fn parse_lsblk(stdout: &str) -> Option<LsblkRoot> {
    serde_json::from_str(stdout)
        .map_err(|e| tracing::warn!("lsblk parse error: {e}"))
        .ok()
}

/// Flatten the tree into a list of references to every device of kind `disk`.
pub fn flat_disks(devs: &[BlockDevice]) -> Vec<&BlockDevice> {
    devs.iter().filter(|d| d.kind == "disk").collect()
}

/// Recursively flatten the tree, returning references to every device of kind
/// `part` regardless of which disk it descends from.
pub fn flat_parts(devs: &[BlockDevice]) -> Vec<&BlockDevice> {
    let mut out = Vec::new();
    for d in devs {
        walk_parts(d, &mut out);
    }
    out
}

fn walk_parts<'a>(dev: &'a BlockDevice, out: &mut Vec<&'a BlockDevice>) {
    if dev.kind == "part" {
        out.push(dev);
    }
    if let Some(children) = dev.children.as_ref() {
        for c in children {
            walk_parts(c, out);
        }
    }
}

/// Format a byte count as a compact human-readable size string: uses binary
/// units (1024-based) with a single decimal place, e.g. `466.9G`,
/// `931.5G`, `1.0T`. Pure helper used for the disk/partition list rows.
pub fn human_size(bytes: u64) -> String {
    const UNITS: &[(&str, u64)] = &[
        ("T", 1024_u64.pow(4)),
        ("G", 1024_u64.pow(3)),
        ("M", 1024_u64.pow(2)),
        ("K", 1024_u64),
    ];
    for (suffix, scale) in UNITS {
        if bytes >= *scale {
            let v = bytes as f64 / *scale as f64;
            return format!("{v:.1}{suffix}");
        }
    }
    format!("{bytes}B")
}

/// Threshold (bytes) below which the Target partition total is rejected. 20
/// GiB = 20 * 1024^3 bytes.
pub const TARGET_MIN_BYTES: u64 = 20 * 1024 * 1024 * 1024;

#[cfg(test)]
mod tests;
