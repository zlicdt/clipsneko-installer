use super::*;

fn part(name: &str, size: u64, fstype: Option<&str>) -> BlockDevice {
    BlockDevice {
        name: name.to_string(),
        kind: "part".to_string(),
        fstype: fstype.map(String::from),
        size,
        parttype: None,
        partlabel: None,
        model: None,
        tran: None,
        rm: false,
        ro: false,
        mountpoints: Vec::new(),
        children: None,
    }
}

fn esp_part(name: &str, fstype: Option<&str>) -> BlockDevice {
    let mut device = part(name, 512 * 1024 * 1024, fstype);
    device.parttype = Some(lsblk::ESP_PARTTYPE.to_string());
    device
}

fn disk(name: &str, children: Vec<BlockDevice>) -> BlockDevice {
    BlockDevice {
        name: name.to_string(),
        kind: "disk".to_string(),
        fstype: None,
        size: 64 * 1024_u64.pow(3),
        parttype: None,
        partlabel: None,
        model: Some("Test Disk".to_string()),
        tran: Some("sata".to_string()),
        rm: false,
        ro: false,
        mountpoints: Vec::new(),
        children: Some(children),
    }
}

fn state_with(esp: Option<&str>, targets: &[&str]) -> InstallerState {
    let mut state = InstallerState::default();
    state.disk.esp_partition = esp.map(String::from);
    state.disk.target_partitions = targets.iter().copied().map(String::from).collect();
    state
}

#[test]
fn wipe_list_contains_every_target_even_without_fstype() {
    let parts = vec![
        part("sda2", 21 * 1024 * 1024 * 1024, None),
        part("sdb1", 21 * 1024 * 1024 * 1024, Some("ext4")),
    ];
    let state = state_with(None, &["sda2", "sdb1"]);
    let wipes = compute_wipe_list(&parts, &state);
    assert_eq!(wipes.len(), 2);
    assert_eq!(wipes[0].0, "sda2");
    assert_eq!(wipes[1].0, "sdb1");
}

#[test]
fn vfat_esp_is_reused_but_non_vfat_esp_is_wiped() {
    let vfat_parts = vec![esp_part("sda1", Some("vfat"))];
    assert!(compute_wipe_list(&vfat_parts, &state_with(Some("sda1"), &[])).is_empty());

    let ext4_parts = vec![esp_part("sda1", Some("ext4"))];
    let wipes = compute_wipe_list(&ext4_parts, &state_with(Some("sda1"), &[]));
    assert_eq!(wipes.len(), 1);
    assert_eq!(wipes[0].0, "sda1");
}

#[test]
fn wipe_list_skips_names_absent_from_snapshot() {
    let parts = vec![esp_part("sda1", Some("vfat"))];
    let state = state_with(Some("missing-esp"), &["missing-target"]);
    assert!(compute_wipe_list(&parts, &state).is_empty());
}

#[test]
fn assigning_esp_requires_gpt_esp_type_and_removes_target_role() {
    let mut step = DiskStep::new();
    step.parts = vec![esp_part("sda1", Some("vfat"))];
    step.role_dialog.part = "sda1".to_string();
    let mut state = state_with(None, &["sda1"]);

    step.apply_role(RoleOption::Esp, &mut state);

    assert_eq!(state.disk.esp_partition.as_deref(), Some("sda1"));
    assert!(state.disk.target_partitions.is_empty());
}

#[test]
fn assigning_esp_rejects_non_esp_partition() {
    let mut step = DiskStep::new();
    step.parts = vec![part("sda1", 512 * 1024 * 1024, Some("vfat"))];
    step.role_dialog.part = "sda1".to_string();
    let mut state = InstallerState::default();

    step.apply_role(RoleOption::Esp, &mut state);

    assert!(state.disk.esp_partition.is_none());
    assert!(step.error_dialog.visible);
}

#[test]
fn assigning_target_clears_same_partition_from_esp() {
    let mut step = DiskStep::new();
    step.role_dialog.part = "sda1".to_string();
    let mut state = state_with(Some("sda1"), &[]);

    step.apply_role(RoleOption::Target, &mut state);

    assert!(state.disk.esp_partition.is_none());
    assert_eq!(state.disk.target_partitions, vec!["sda1"]);
}

#[test]
fn unassigned_clears_both_roles() {
    let mut step = DiskStep::new();
    step.role_dialog.part = "sda1".to_string();
    let mut state = state_with(Some("sda1"), &["sda1"]);

    step.apply_role(RoleOption::Unassigned, &mut state);

    assert!(state.disk.esp_partition.is_none());
    assert!(state.disk.target_partitions.is_empty());
}

#[test]
fn reconciliation_removes_missing_and_overlapping_roles() {
    let parts = vec![
        esp_part("sda1", Some("vfat")),
        part("sda2", 21 * 1024 * 1024 * 1024, None),
    ];
    let mut state = state_with(Some("sda1"), &["sda1", "sda2", "missing"]);
    state.disk.raid_mode = Some(BtrfsRaidMode::Raid1);

    reconcile_assignments(&parts, &[], &mut state);

    assert_eq!(state.disk.esp_partition.as_deref(), Some("sda1"));
    assert_eq!(state.disk.target_partitions, vec!["sda2"]);
    assert!(state.disk.raid_mode.is_none());
}

#[test]
fn live_media_partitions_are_protected_and_reconciled_out() {
    let mut live_part = esp_part("sda1", Some("vfat"));
    live_part.mountpoints = vec![Some("/run/archiso/bootmnt".to_string())];
    let disks = vec![disk("sda", vec![live_part.clone()])];
    let protected = protected_partition_names(&disks);
    let mut state = state_with(Some("sda1"), &["sda1"]);

    reconcile_assignments(&[live_part], &protected, &mut state);

    assert_eq!(protected, vec!["sda1"]);
    assert!(state.disk.esp_partition.is_none());
    assert!(state.disk.target_partitions.is_empty());
    assert!(!disk_is_selectable(&disks[0]));
}

#[test]
fn roles_require_existing_gpt_esp_and_existing_distinct_targets() {
    let mut step = DiskStep::new();
    step.phase = Phase::PartitionAssign;
    step.parts = vec![
        esp_part("sda1", Some("vfat")),
        part("sda2", 21 * 1024 * 1024 * 1024, None),
        part("sda3", 512 * 1024 * 1024, Some("vfat")),
    ];

    assert!(step.roles_valid(&state_with(Some("sda1"), &["sda2"])));
    assert!(!step.roles_valid(&state_with(Some("missing"), &["sda2"])));
    assert!(!step.roles_valid(&state_with(Some("sda3"), &["sda2"])));
    assert!(!step.roles_valid(&state_with(Some("sda1"), &["missing"])));
    assert!(!step.roles_valid(&state_with(Some("sda1"), &["sda1"])));
}

#[test]
fn usable_capacity_single_and_raid_profiles() {
    let gib = 1024_u64.pow(3);
    assert_eq!(usable_capacity(&[21 * gib], None), Some(21 * gib));
    assert_eq!(
        usable_capacity(&[11 * gib, 30 * gib], Some(BtrfsRaidMode::Raid0)),
        Some(22 * gib)
    );
    assert_eq!(
        usable_capacity(&[11 * gib, 30 * gib], Some(BtrfsRaidMode::Raid1)),
        Some(11 * gib)
    );
    assert_eq!(
        usable_capacity(&[10 * gib, 10 * gib, 30 * gib], Some(BtrfsRaidMode::Raid1)),
        Some(20 * gib)
    );
    assert_eq!(usable_capacity(&[11 * gib, 11 * gib], None), None);
}

#[test]
fn capacity_threshold_is_strictly_greater_than_twenty_gib() {
    assert_eq!(lsblk::TARGET_MIN_BYTES, 20 * 1024_u64.pow(3));
    assert!(usable_capacity(&[lsblk::TARGET_MIN_BYTES], None).unwrap() <= lsblk::TARGET_MIN_BYTES);
    assert!(
        usable_capacity(&[lsblk::TARGET_MIN_BYTES + 1], None).unwrap() > lsblk::TARGET_MIN_BYTES
    );
}

#[test]
fn cfdisk_command_paths_to_dev_disk() {
    let (program, args) = cfdisk_command("sda");
    if is_root() {
        assert_eq!(program, "cfdisk");
        assert_eq!(args, vec!["/dev/sda"]);
    } else {
        assert_eq!(program, "sudo");
        assert_eq!(args, vec!["--", "cfdisk", "/dev/sda"]);
    }
}

#[test]
fn disk_table_renders_device_model_and_size_at_minimum_width() {
    let mut step = DiskStep::new();
    step.disks = vec![disk("nvme0n1", Vec::new())];
    let state = InstallerState::default();
    let backend = ratatui::backend::TestBackend::new(60, 12);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| step.render(frame, frame.area(), &state, true))
        .unwrap();
    let rendered: String = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect();

    assert!(rendered.contains("nvme0n1"));
    assert!(rendered.contains("Test Disk"));
    assert!(rendered.contains("64.0G"));
}
