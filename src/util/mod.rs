//! Utility helpers shared across steps and the installer stage.
//!
//! Currently houses the subprocess / privilege helpers (`process` module) and
//! the `lsblk` JSON parser. Future M3 util modules (`geoip`, `password`,
//! `locale_list`) will live here too.

pub mod lsblk;
pub mod process;
