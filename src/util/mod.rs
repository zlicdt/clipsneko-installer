//! Utility helpers shared across steps and the installer stage.
//!
//! Currently houses the subprocess / privilege helpers (`process` module).
//! Future M1/M2/M3 util modules (`lsblk`, `geoip`, `password`, `locale_list`)
//! will live here too.

pub mod process;
