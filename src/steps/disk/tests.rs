use super::*;

fn part(name: &str, size: u64, fstype: Option<&str>) -> BlockDevice {
    BlockDevice {
        name: name.to_string(),
        kind: "part".to_string(),
        fstype: fstype.map(String::from),
        size,
        pttype: None,
        parttype: None,
        partlabel: None,
        children: None,
    }
}

fn state_with(esp: Option<&str>, targets: &[&str]) -> InstallerState {
    let mut s = InstallerState::default();
    s.disk.esp_partition = esp.map(String::from);
    s.disk.target_partitions = targets.iter().copied().map(String::from).collect();
    s
}

#[test]
fn compute_wipe_list_empty_no_esp_no_targets() {
    let parts = vec![part("sda1", 1, Some("vfat"))];
    let state = state_with(None, &[]);
    assert!(compute_wipe_list(&parts, &state).is_empty());
}

#[test]
fn compute_wipe_list_vfat_esp_not_wiped() {
    let parts = vec![part("sda1", 512 * 1024 * 1024, Some("vfat"))];
    let state = state_with(Some("sda1"), &[]);
    assert!(compute_wipe_list(&parts, &state).is_empty());
}

#[test]
fn compute_wipe_list_non_vfat_esp_wiped() {
    let parts = vec![part("sda1", 512 * 1024 * 1024, Some("ext4"))];
    let state = state_with(Some("sda1"), &[]);
    let wipes = compute_wipe_list(&parts, &state);
    assert_eq!(wipes.len(), 1);
    assert_eq!(wipes[0].0, "sda1");
}

#[test]
fn compute_wipe_list_target_with_fstype_wiped() {
    let parts = vec![part("sda2", 21 * 1024 * 1024 * 1024, Some("ext4"))];
    let state = state_with(None, &["sda2"]);
    let wipes = compute_wipe_list(&parts, &state);
    assert_eq!(wipes.len(), 1);
    assert_eq!(wipes[0].0, "sda2");
}

#[test]
fn compute_wipe_list_target_without_fstype_not_wiped() {
    let parts = vec![part("sda2", 21 * 1024 * 1024 * 1024, None)];
    let state = state_with(None, &["sda2"]);
    assert!(compute_wipe_list(&parts, &state).is_empty());
}

#[test]
fn compute_wipe_list_both_esp_and_target_wiped() {
    let parts = vec![
        part("sda1", 512 * 1024 * 1024, Some("ext4")), // non-vfat ESP
        part("sda2", 21 * 1024 * 1024 * 1024, Some("btrfs")), // target with fs
    ];
    let state = state_with(Some("sda1"), &["sda2"]);
    let wipes = compute_wipe_list(&parts, &state);
    assert_eq!(wipes.len(), 2);
    assert!(wipes.iter().any(|(n, _)| n == "sda1"));
    assert!(wipes.iter().any(|(n, _)| n == "sda2"));
}

#[test]
fn compute_wipe_list_part_not_in_snapshot_skipped() {
    let parts = vec![part("sda1", 100, Some("vfat"))];
    let state = state_with(Some("sda3"), &["sda4"]); // not in parts
    assert!(compute_wipe_list(&parts, &state).is_empty());
}

#[test]
fn compute_wipe_list_pure_vfat_esp_with_vfat_target_no_wipe() {
    // ESP is vfat (no wipe) and a target with vfat (already vfat, not wiped only
    // if Target's rule treats empty fstype-only — but a vfstype *is* non-empty,
    // so Target is wiped regardless of being vfat). Only ESP path is special.
    let parts = vec![
        part("sda1", 512 * 1024 * 1024, Some("vfat")),
        part("sda2", 21 * 1024 * 1024 * 1024, None),
    ];
    let state = state_with(Some("sda1"), &["sda2"]);
    assert!(compute_wipe_list(&parts, &state).is_empty());
}

// When called outside an `activate` (default state), cfdisk_command must prefix
// sudo: this depends on is_root(); in the test environment we expect non-root so
// we can assert the sudo path. We test the *shape* of the command (program is
// cfdisk and disk path is /dev/<disk>) defensively.
#[test]
fn cfdisk_command_paths_to_dev_disk() {
    if is_root() {
        let (prog, args) = cfdisk_command("sda");
        assert_eq!(prog, "cfdisk");
        assert_eq!(args, vec!["/dev/sda".to_string()]);
    } else {
        let (prog, args) = cfdisk_command("sda");
        assert_eq!(prog, "sudo");
        assert_eq!(
            args,
            vec![
                "--".to_string(),
                "cfdisk".to_string(),
                "/dev/sda".to_string()
            ]
        );
    }
}
