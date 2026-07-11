use super::*;

/// Minimal two-disk fixture: `sda` (single GPT disk with an ESP + a root
/// part on a btrfs fs) and `sdb` (empty MBR disk). Sizes are given as bare
/// numbers in the same way `lsblk -b` emits them.
const FIXTURE: &str = r#"{
  "blockdevices": [
    {
      "name": "sda",
      "type": "disk",
      "fstype": null,
      "size": 500107862016,
      "parttype": null,
      "partlabel": null,
      "children": [
        {
          "name": "sda1",
          "type": "part",
          "fstype": "vfat",
          "size": 536870912,
          "parttype": "c12a7328-f81f-11d2-ba4b-00a0c93ec93b",
          "partlabel": "EFI System Partition",
          "children": null
        },
        {
          "name": "sda2",
          "type": "part",
          "fstype": "btrfs",
          "size": 499658414080,
          "parttype": "0fc63daf-8483-4772-8e79-3d69d8477de4",
          "partlabel": "root",
          "children": null
        }
      ]
    },
    {
      "name": "sdb",
      "type": "disk",
      "fstype": null,
      "size": 1000204886016,
      "parttype": null,
      "partlabel": null,
      "children": null
    },
    {
      "name": "sr0",
      "type": "rom",
      "fstype": "iso9660",
      "size": 1048576,
      "parttype": null,
      "partlabel": null,
      "children": null
    }
  ]
}
"#;

#[test]
fn parse_basic_tree() {
    let root = parse_lsblk(FIXTURE.as_bytes()).expect("fixture should parse");
    assert_eq!(root.blockdevices.len(), 3);
    assert_eq!(root.blockdevices[0].name, "sda");
    assert_eq!(root.blockdevices[0].kind, "disk");
    assert_eq!(root.blockdevices[2].kind, "rom");
}

#[test]
fn parse_null_fstype_becomes_none() {
    let root = parse_lsblk(FIXTURE.as_bytes()).unwrap();
    assert!(root.blockdevices[0].fstype.is_none());
    assert_eq!(
        root.blockdevices[0].children.as_ref().unwrap()[0]
            .fstype
            .as_deref(),
        Some("vfat")
    );
}

#[test]
fn parse_size_number() {
    let root = parse_lsblk(FIXTURE.as_bytes()).unwrap();
    assert_eq!(root.blockdevices[0].size, 500107862016);
    let esp = &root.blockdevices[0].children.as_ref().unwrap()[0];
    assert_eq!(esp.size, 536870912);
}

#[test]
fn parse_esp_parttype() {
    let root = parse_lsblk(FIXTURE.as_bytes()).unwrap();
    let esp = &root.blockdevices[0].children.as_ref().unwrap()[0];
    assert_eq!(esp.parttype.as_deref(), Some(ESP_PARTTYPE));
}

#[test]
fn parse_invalid_json_returns_error() {
    assert!(parse_lsblk(b"not json").is_err());
    assert!(parse_lsblk(b"").is_err());
}

#[test]
fn flat_disks_skips_roms_and_parts() {
    let root = parse_lsblk(FIXTURE.as_bytes()).unwrap();
    let disks = flat_disks(&root.blockdevices);
    assert_eq!(disks.len(), 2);
    assert_eq!(disks[0].name, "sda");
    assert_eq!(disks[1].name, "sdb");
}

#[test]
fn flat_parts_collects_all_partitions() {
    let root = parse_lsblk(FIXTURE.as_bytes()).unwrap();
    let parts = flat_parts(&root.blockdevices);
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0].name, "sda1");
    assert_eq!(parts[1].name, "sda2");
    // sdb has no partitions and rom is not a part.
    assert!(parts.iter().all(|p| p.kind == "part"));
}

#[test]
fn flat_parts_empty_tree() {
    let root = parse_lsblk(br#"{"blockdevices": []}"#).unwrap();
    assert!(flat_parts(&root.blockdevices).is_empty());
    assert!(flat_disks(&root.blockdevices).is_empty());
}

#[test]
fn flat_parts_handles_nested_partitions() {
    // Some `lsblk` builds surface partitions as children of partitions (e.g.
    // LUKS containers); walk_parts descends through all of them.
    let nested = r#"{
      "blockdevices": [
        {"name":"sda","type":"disk","size":0,
         "children":[
           {"name":"sda1","type":"part","size":0,"children":null},
           {"name":"sda2","type":"part","size":0,
            "children":[{"name":"crypt0","type":"crypt","size":0,"children":null}]}
         ]}
      ]
    }"#;
    let root = parse_lsblk(nested.as_bytes()).unwrap();
    let parts = flat_parts(&root.blockdevices);
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0].name, "sda1");
    assert_eq!(parts[1].name, "sda2");
}

#[test]
fn parent_disks_are_derived_from_the_tree_without_name_guessing() {
    let json = br#"{"blockdevices":[
      {"name":"nvme0n1","type":"disk","size":0,"children":[
        {"name":"nvme0n1p1","type":"part","size":0}
      ]},
      {"name":"mmcblk0","type":"disk","size":0,"children":[
        {"name":"mmcblk0p1","type":"part","size":0}
      ]}
    ]}"#;
    let root = parse_lsblk(json).unwrap();
    let selected = vec![
        "mmcblk0p1".to_string(),
        "nvme0n1p1".to_string(),
        "nvme0n1p1".to_string(),
    ];

    assert_eq!(
        parent_disks_for_partitions(&root.blockdevices, &selected),
        ["nvme0n1", "mmcblk0"]
    );
}

#[test]
fn human_size_formats_cleanly() {
    assert_eq!(human_size(0), "0B");
    assert_eq!(human_size(512), "512B");
    assert_eq!(human_size(1024), "1.0K");
    assert_eq!(human_size(536870912), "512.0M");
    assert_eq!(human_size(500107862016), "465.8G");
    assert_eq!(human_size(1024_u64.pow(4)), "1.0T");
}

#[test]
fn target_min_bytes_is_20_gib() {
    assert_eq!(TARGET_MIN_BYTES, 20 * 1024 * 1024 * 1024);
}

#[test]
fn flat_disks_excludes_zram() {
    let json = br#"{"blockdevices":[
      {"name":"zram0","type":"disk","size":4294967296},
      {"name":"sda","type":"disk","size":500107862016}
    ]}"#;
    let root = parse_lsblk(json).unwrap();
    let disks = flat_disks(&root.blockdevices);
    assert_eq!(disks.len(), 1);
    assert_eq!(disks[0].name, "sda");
}

#[test]
fn detects_live_media_mount_on_child_partition() {
    let json = br#"{"blockdevices":[{
      "name":"sda","type":"disk","size":500107862016,
      "children":[{
        "name":"sda1","type":"part","size":1000000000,
        "mountpoints":["/run/archiso/bootmnt"]
      }]
    }]}"#;
    let root = parse_lsblk(json).unwrap();
    assert!(is_live_media_disk(&root.blockdevices[0]));
}
