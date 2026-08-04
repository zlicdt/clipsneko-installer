//! CPU vendor detection used to add the matching microcode package to the
//! pacstrap set. The live environment's CPU is the target system's CPU, so
//! `/proc/cpuinfo` is authoritative here.

/// Map a `/proc/cpuinfo` `vendor_id` value to its microcode package.
fn package_for_vendor(vendor: &str) -> Option<&'static str> {
    match vendor {
        "GenuineIntel" => Some("intel-ucode"),
        "AuthenticAMD" => Some("amd-ucode"),
        _ => None,
    }
}

/// Find the microcode package matching the vendor recorded in the given
/// `/proc/cpuinfo` contents. Returns `None` for unknown vendors.
pub fn microcode_for_cpuinfo(contents: &str) -> Option<&'static str> {
    for line in contents.lines() {
        if let Some((key, value)) = line.split_once(':') {
            if key.trim() == "vendor_id" {
                return package_for_vendor(value.trim());
            }
        }
    }
    None
}

/// Detect the live environment's CPU vendor and return the microcode package
/// the target should receive. An unreadable cpuinfo or an unknown vendor is
/// not fatal: the install continues without microcode updates and the
/// decision is logged instead.
pub fn microcode_package() -> Option<&'static str> {
    let package = std::fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|contents| microcode_for_cpuinfo(&contents));
    match package {
        Some(name) => tracing::info!(package = name, "detected CPU microcode package"),
        None => tracing::warn!("CPU vendor unknown; installing no microcode package"),
    }
    package
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intel_and_amd_vendors_map_to_their_microcode_packages() {
        assert_eq!(
            microcode_for_cpuinfo("processor\t: 0\nvendor_id\t: GenuineIntel\ncpu family\t: 6\n"),
            Some("intel-ucode")
        );
        assert_eq!(
            microcode_for_cpuinfo("processor\t: 0\nvendor_id\t: AuthenticAMD\ncpu family\t: 25\n"),
            Some("amd-ucode")
        );
    }

    #[test]
    fn unknown_or_missing_vendor_yields_no_package() {
        assert_eq!(microcode_for_cpuinfo("vendor_id\t: GenuineTselik\n"), None);
        assert_eq!(microcode_for_cpuinfo("processor\t: 0\n"), None);
        assert_eq!(microcode_for_cpuinfo(""), None);
    }
}
