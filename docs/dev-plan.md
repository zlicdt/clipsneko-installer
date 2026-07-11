# Development Plan

A milestone-based roadmap for the ClipsNeko Linux installer. Milestones are
ordered; each lists its scope, deliverables, acceptance criteria, and the
unit-testable pure-logic surface. The current state of work lives in
`docs/dev-prog.md` (Done / Not done running lists); this file is the
high-level plan those items roll up into.

## Definition of Done (per milestone)

Every milestone is considered done only when **all** of the following hold:

- `cargo fmt --check` green.
- `cargo clippy -- -D warnings` green.
- `cargo build` green.
- `cargo test` green — unit tests for the milestone's pure-logic modules
  (validation, parsing, argument construction, string manipulation). Code
  that shells out to system tools is kept thin: the pure logic is factored
  into a testable function and the `Command::new(...)` wrapper just calls it.
- Manual run test on a test VM or the ClipsNeko Live ISO for the milestone's
  runtime behavior.
- Every focused bordered body control renders both its border and title in bold
  white. Moving focus to a footer button restores the body border to its
  default style without changing the footer's existing focus treatment. All
  visible borders, including dialogs and informational containers, use the
  shared rounded-corner block constructor.
- `po/clipsneko-installer.pot` and both `.po` files are consistent with
  every `t!()` string added or changed.
- `docs/dev-prog.md` updated: finished items moved from "Not done" to "Done".

## Milestone status

| Milestone | Title                              | Status |
|-----------|------------------------------------|--------|
| M0        | Foundation                         | Done   |
| M1        | Pre-install config wizard          | Pending |
| M2        | Disk partitioning                  | Pending |
| M3        | Selection & identity               | Pending |
| M4a       | Install — partition, format, mount | Pending |
| M4b       | Install — pacstrap & chroot config | Pending |
| M4c       | Install — bootloader & finalize    | Pending |
| M5        | Postinstall hook & polish          | Pending |

---

## M0 — Foundation (Done)

Scaffolding, i18n bootstrap, logging, and the linear wizard shell with all
12 steps stubbed. See the "Done" list in `docs/dev-prog.md` for the full
record. Nothing further is needed here.

---

## M1 — Pre-install config wizard

Turn the first four wizard steps from stubs into real, state-populating
screens. After M1 the user can pick a UI language, set the keyboard, get
online, and configure mirrors — everything required before any disk work.

### Deliverables

- `src/steps/language.rs` — two coordinated lists: the seven supported UI
  languages (`en`, `zh_CN`, `zh_TW`, `ja`, `de`, `ko`, and `ru`) for the
  installer, and a multi-select list of every UTF-8 target locale parsed from
  locale.gen. Apply UI language live through `set_language()` and add its
  matching locale to the target set. Space toggles target locales but cannot
  remove the last selection; L enables the highlighted locale if necessary and
  chooses it as the default `LANG`, while Enter advances without changing
  locale choices. Persist `state.ui_lang`, `state.target_locales`, and
  `state.target_locale`.
- `src/steps/keyboard.rs` — list keymaps from `localectl list-keymaps`;
  `loadkeys` immediately on selection; persist `state.keymap`.
- `src/steps/network.rs` — suspend ratatui, run `nmtui` full-screen, resume;
  on return verify connectivity with `curl -sI http://ip-api.com/json`;
  allow re-launching nmtui on failure; set `state.network_ok`.
- `src/steps/mirror.rs` — parse `/etc/pacman.d/mirrorlist` into region
  blocks (`## <Region>` + `Server =` lines); show a single-select region
  list (Up/Down/j/k) and a manual `Server =` URL input field below it
  (Tab toggles focus). On Next: if the input field is non-empty, use it as
  the sole mirror; otherwise move the selected region's `Server =` lines
  to the top of the file. Rewrite the mirrorlist, validate with
  `pacman -Sy` (exit 0 = ok); on failure show a modal error dialog and
  retry. Keeping a manual entry as the sole active server ensures validation
  cannot silently fall back to a stock mirror. Store the chosen source lines
  in `state.mirror_lines`.
- `src/util/locale_list.rs` — parse commented and enabled `/etc/locale.gen`
  entries into the available UTF-8 target-locale list.
- `src/util/process.rs` — suspend-ratatui/run-subprocess/resume helper
  shared by interactive tools such as `nmtui` and `cfdisk`. Non-interactive
  commands such as `pacman -Sy` capture output without leaving the TUI.
- Sample runtime package file in the repo: `config/packages.list`. The user's
  PKGBUILD installs it to `/etc/clipsneko-installer/packages.list`; the
  installer exits if it is missing at startup. Repository configuration comes
  from the Live ISO's existing `pacman.conf`, not a separate runtime file.

### Acceptance

- Language picker switches the whole UI live among all seven supported
  languages and automatically adds the matching target locale. The target
  picker initially contains `en_US.UTF-8`, supports multiple selections,
  prevents removing the final selection, and records one selected locale as
  the default `LANG`; every release `.mo` is visually verified.
- Keyboard list loads from `localectl`; selecting one runs `loadkeys` and
  the effect is visible in the next text input.
- `nmtui` opens full-screen, returns to the wizard, and the connectivity
  check correctly reports online/offline.
- Region list loads from `/etc/pacman.d/mirrorlist`; selecting a region
  reorders the file with that region's mirrors on top; manual-entry path
  accepts a `Server =` line; `pacman -Sy` validation succeeds/fails as
  appropriate and the UI lets the user retry on failure.

### Unit tests

- `locale_list` — parse commented/enabled UTF-8 entries, ignore prose, legacy
  charmaps, duplicates, and blank lines.
- `util::process` — the suspend/resume bookkeeping (not the actual spawn);
  e.g. the helper restores raw mode even if the subprocess errors.
- Mirror `Server =` line format validation (regex / structure, not the
  network fetch).

### Dependencies

M0 (done).

---

## M2 — Disk partitioning

The disk step (step 5). Two sub-pages inside the step: a disk picker (run
cfdisk against one or more disks) and a partition role picker (assign ESP
and Target roles; ESP single-select, Target multi-select enabling btrfs
RAID at format time; unified wipe-warning dialog on Next).

### Deliverables

- `src/steps/disk.rs` — two sub-pages:
  - *Sub-page A (disk picker):* parse the fixed lsblk JSON columns into a
    device/model/transport/size/status table; exclude zram, disable the Live ISO
    backing disk and read-only disks, but allow other removable disks. Enter
    opens `cfdisk /dev/<disk>` full-screen. Returning clears all assignments,
    runs `partprobe`, and refreshes lsblk; the on-screen Next button advances to
    sub-page B.
  - *Sub-page B (partition role picker):* list every partition of type
    `part` on every disk from the latest `lsblk` (name / size / current
    FSTYPE / label / role); protected Live-media partitions cannot be assigned.
    Enter chooses explicit ESP / Target / Unassigned; roles are mutually
    exclusive and ESP requires the GPT ESP type. Multiple Targets require a
    RAID0/RAID1 data-profile choice and a conservative usable-capacity check
    strictly above 20 GiB. Pressing Next shows a blocking confirmation listing
    every Target plus a non-vfat ESP; a pure-vfat ESP is reused.
- `src/util/lsblk.rs` — parse the fixed lsblk JSON schema into a typed device
  tree; expose physical disks (excluding zram), partitions, Live-media
  detection, ESP-type validation, and byte-size formatting.

### Acceptance

- Sub-page A excludes zram, visibly disables Live/read-only disks, and shows
  model/transport/size/status for every remaining disk candidate.
- Enter opens `cfdisk` only on an enabled disk; returning clears assignments
  before partprobe + lsblk refresh. Non-zero partprobe is a retryable modal.
- Sub-page B lists all partitions on every disk after the latest re-read.
- Enter pops the explicit ESP/Target/Unassigned dialog; protected partitions
  are rejected, ESP type is validated, and assigned state is visible.
- Multiple Targets require RAID0/RAID1 selection. Profile-adjusted usable
  capacity must be > 20 GiB.
- On Next, every Target and any non-vfat ESP appears in one wipe-warning
  dialog; confirming leaves the step, cancelling returns to the table.
- A pure-vfat ESP is neither reformatted nor warned about.

### Unit tests

- `util::lsblk` — fixed-schema parsing, zram exclusion, tree flattening,
  Live-media detection, sizes, and ESP parttype UUID.
- Role mutual exclusion, protected partitions, post-refresh reconciliation,
  usable RAID0/RAID1 capacity, strict 20 GiB boundary, and wipe-list contents.

### Dependencies

M0 (disk step does not require M1 to be complete, but running it
end-to-end needs network from M1 for a fully populated state).

---

## M3 — Selection & identity

Steps 6-11: kernel, NVIDIA, timezone, user, hostname, confirm. After M3
the wizard holds a complete, validated pre-install configuration and the
confirm screen can show a full summary.

### Deliverables

- `src/steps/kernel.rs` — single-select from `linux` / `linux-lts` /
  `linux-zen` / `linux-hardened`, defaulting to `linux-zen`; always derive the
  matching headers package for installation.
- `src/steps/nvidia.rs` — "no NVIDIA" or one package from the current
  `nvidia-open` / `nvidia-open-lts` / `nvidia-open-dkms` matrix, filtered by
  the chosen kernel (see `design.md` §4 step 7). Incompatible choices remain
  visible but dimmed and are skipped by navigation. A saved choice made
  incompatible by changing the kernel resets to `nvidia-open-dkms` on entry.
  Kernel headers are already an unconditional part of the selected kernel's
  install package set.
- `src/steps/timezone.rs` — GeoIP timezone default with UTC fallback; a
  two-list picker populated by `timedatectl list-timezones`. The first list is
  the ten supported geographic regions plus direct UTC; the second contains
  full timezone names and is disabled for UTC. Legacy top-level aliases and
  `Etc/*` are excluded, and there is no manual text input.
- `src/steps/user.rs` — centered, bordered username
  (`^[a-z_][a-z0-9_-]*$`), password, and confirmation form with no GECOS
  field. A live strength bar is advisory: every non-empty password is accepted
  when confirmation matches. The step writes account metadata to `state.user`
  and keeps the confirmed password only in a non-Debug `SecretString` that
  zeroizes on Drop.
- `src/steps/hostname.rs` — centered, bordered single-input form validated as
  one ASCII DNS label by
  `^[A-Za-z0-9]([A-Za-z0-9-]{0,61}[A-Za-z0-9])?$`. Uppercase letters are
  accepted and preserved, while FQDNs are rejected; Enter and
  footer Next commit a valid value, and re-entry restores it.
- `src/steps/confirm.rs` — scrollable summary of locale, keyboard, kernel,
  NVIDIA, hostname, timezone, username, affected disks, ESP, every Target,
  and any RAID profile, while never reading or rendering the password.
  Device-to-disk relationships are derived from the latest lsblk tree in the
  disk step and saved explicitly. The page remains linear Back/Next, and a
  final blocking dialog defaults to Cancel and requires explicitly selecting
  Install before handing off to the install step.
- `src/util/geoip.rs` — ip-api.com fetch + JSON parse.
- `src/util/password.rs` — strength heuristic (length, char classes,
  common-password check) returning a weak/fair/good/strong level; secret
  wrapper backed by the justified `zeroize` crate.

### Acceptance

- Kernel single-select defaults to `linux-zen`, records the choice in
  `state.kernel`, and maps every choice to its matching headers package.
- NVIDIA variant list is correctly filtered by the chosen kernel;
  incompatible options are visible but not selectable.
- Timezone defaults to a supported GeoIP result when online and UTC otherwise.
  Up/Down navigates the focused list; Right/Enter enters a geographic region,
  Left returns to the region list, and Enter applies a concrete timezone or
  direct UTC. The UTC choice visibly disables the second list, and Tab reaches
  both lists and the footer in order.
- Username validation rejects invalid names live; the password strength bar
  updates as the user types; empty passwords and confirmation mismatches block
  Next, while a matching non-empty weak password remains valid.
- Hostname validation rejects invalid input live, and the install stage writes
  the committed value to `/etc/hostname` plus its `127.0.1.1` mapping in
  `/etc/hosts`.
- Confirm screen shows every requested choice and every affected disk and
  partition without exposing the password; long summaries scroll. The
  blocking dialog defaults to Cancel and appears before the install step.

### Unit tests

- Kernel and unconditional matching-headers package derivation for all four
  choices.
- Timezone output parsing and filtering, GeoIP/default restoration, UTC
  fallback, two-list focus/navigation, direct UTC behavior, footer commit,
  and translated rendering.
- NVIDIA-kernel compatibility matrix (all kernels × current open variants).
- Username regex (valid/invalid boundary cases).
- Hostname regex (length, leading/trailing hyphen, uppercase acceptance and
  preservation).
- Password strength heuristic (empty, short, all-lower, mixed, common
  password from a small fixture list).
- `util::geoip` JSON parse on a fixture response (and a malformed one).
- Confirmation summary completeness, affected-disk derivation, password
  exclusion, scrolling, and explicit final-dialog acceptance.

### Dependencies

M2 (confirm reads disk state; NVIDIA reads kernel state).

---

## M4 — Install stage

Split into three sub-milestones. Each builds on the previous; none is
usable standalone for a full install until M4c lands.

### M4a — Partition, format, mount

#### Deliverables

- `src/installer/partition.rs` — format & mount:
  - If a single Target partition was chosen: `mkfs.btrfs -f <part>`. If two
    or more, use the data RAID mode already stored by the disk step (`raid0`
    or `raid1`; metadata fixed at `raid1`), then
    `mkfs.btrfs -f -d <mode> -m raid1 <part1> <part2> ...`.
  - Create subvolumes `@`, `@home`; remount root with
    `-o compress=zstd,subvol=@`; mount `@home` with implicit-level zstd
    compression at `/mnt/home`.
  - ESP: `mkfs.vfat -F32` only if not already vfat; mount at `/mnt/boot/efi`.
  - (No extra-partition mapping in v0.1.)

#### Acceptance

- A single Target partition is formatted with `mkfs.btrfs -f`; two or more
  Targets are formatted with `mkfs.btrfs -f -d <mode> -m raid1 ...` using the
  profile validated in M2.
- Root is mounted with `compress=zstd,subvol=@`; `@home` uses
  `compress=zstd,subvol=@home` at `/mnt/home`.
- Existing-vfat ESP is mounted without reformat; non-vfat ESP is formatted
  then mounted, at `/mnt/boot/efi`.

#### Unit tests

- Single Target vs multi-Target btrfs command argument construction.
- RAID argument list construction for `raid0` and `raid1` data modes.
- Btrfs mount-option string construction (`compress=zstd,subvol=@` for root,
  `compress=zstd,subvol=@home` for home).
- Subvolume path computation (`/mnt` vs `/mnt/home` given subvol names).
- ESP reformat decision (reused from M2's test, applied at format time).

#### Dependencies

M3 (full state).

### M4b — Pacstrap & chroot config

#### Deliverables

- `src/installer/pacstrap.rs` — construct and run `pacstrap -P` from state:
  the authoritative static `packages.list` contents plus the chosen kernel,
  its matching headers, linux-firmware, and the chosen NVIDIA package. The Live
  ISO's existing `pacman.conf` already contains the ClipsNeko repository, and `-P`
  copies `pacman.conf` plus `pacman.d` to the target;
  run `genfstab -U /mnt >> /mnt/etc/fstab`; ensure both btrfs subvolume entries
  carry zstd compression while preserving a kernel-normalized default such as
  `compress=zstd:3` rather than rewriting or selecting a level.
- `src/installer/chroot.rs` — under `arch-chroot /mnt`: timezone symlink +
  `hwclock --systohc`; make the selected locales the exact enabled UTF-8 set
  in `/etc/locale.gen` → `locale-gen`; write the selected default `LANG` to
  `/etc/locale.conf` and write `/etc/vconsole.conf`; `/etc/hostname` +
  `/etc/hosts`;
  `useradd -m -G wheel -s /bin/zsh`; pipe credentials to
  `chpasswd` through stdin, then immediately zeroize the in-memory secret;
  uncomment `%wheel ALL=(ALL:ALL) ALL` in `/etc/sudoers`; rely on the pacman
  configuration copied by `pacstrap -P`;
  remove `kms` from `HOOKS` in `/etc/mkinitcpio.conf` if NVIDIA was
  installed; `mkinitcpio -P`. The static `base-devel` package supplies `sudo`
  for the sudoers configuration; it is not added dynamically.

#### Acceptance

- `pacstrap -P` installs exactly the static packages plus packages derived
  from state, and copies the Live ISO's pacman configuration to the target.
- `/mnt/etc/fstab` is generated and both btrfs subvolume entries carry zstd
  compression, with any kernel-normalized default level preserved.
- Inside the chroot: timezone, locale, vconsole, hostname, hosts, user
  creation, sudoers, copied pacman configuration, and mkinitcpio
  (with `kms` removed when NVIDIA is chosen) are all applied.

#### Unit tests

- `pacstrap` argument list construction from a fixture `InstallerState`
  and `packages.list`.
- `mkinitcpio.conf` HOOKS `kms`-removal (string edit, idempotent).
- `/etc/locale.gen` editing (enable a set of locales).
- `chpasswd` stdin construction never exposes the secret through command
  arguments, Debug, tracing, or logs; success and Drop both zeroize it.
- `/etc/sudoers` `%wheel` uncomment logic (string edit).
- `genfstab` validation accepts implicit `compress=zstd` and a normalized
  default such as `compress=zstd:3` on both btrfs subvolume lines.

#### Dependencies

M4a.

### M4c — Bootloader & finalize

#### Deliverables

- `src/installer/bootloader.rs` —
  `grub-install --target=x86_64-efi --efi-directory=/boot/efi --bootloader-id=clipsneko`;
  `grub-mkconfig -o /boot/grub/grub.cfg`;
  `systemctl enable NetworkManager` (inside the chroot);
  run the pipeline in a background worker with a responsive spinner and locked
  Back/Esc/Ctrl+C paths; on failure stop without rollback and show Return/View
  Log actions; prompt "Reboot now?" with Reboot focused by default. Reboot runs
  privileged `umount -R /mnt` then `reboot`; not-now exits to the launching
  shell and deliberately leaves the target mounted.

#### Acceptance

- GRUB is installed to the ESP with the `clipsneko` bootloader ID.
- `grub.cfg` is generated and references the btrfs root subvolume.
- `NetworkManager.service` is enabled on the target.
- The reboot prompt defaults to Reboot; reboot unmounts and restarts, while
  not-now returns to a usable live shell with `/mnt` preserved for inspection.
- Destructive work cannot be interrupted through wizard Back/Esc/Ctrl+C. A
  failure stops without rollback and its dialog exposes Return and View Log.

#### Unit tests

- `grub-install` argument list construction.
- `grub-mkconfig` output path.
- Reboot-decision state machine (default reboot → umount+reboot, not-now →
  shell with mounts preserved), install navigation lock, and failure/log dialog.

#### Dependencies

M4b.

---

## M5 — Postinstall hook & polish

The deferred "run a script as the new user inside the chroot" hook, plus
the small polish items that were open since M0. **The postinstall hook is
blocked on user direction** — its design (script path, package that
installs it, invocation method, HOME/XDG env injection) must be decided
with the user before this milestone can start.

### Deliverables

- `src/installer/postinstall.rs` — the user-mode script hook, per the
  user's spec (blocked).
- F1 help screen content and rendering.
- Final end-to-end install test on a test VM: a full run from language
  pick through reboot produces a bootable ClipsNeko system.

### Acceptance

- The postinstall hook runs the specified script as the new user inside
  the chroot, with the agreed env, and its output is captured to the log.
- F1 shows a help screen listing all keybindings.
- A full end-to-end install on a VM boots into a working system with the
  created user, zsh shell, NetworkManager, and GRUB; the installer leaves the
  target root-account policy unchanged.

### Unit tests

- Depends on the postinstall hook's design — at minimum, the argument/env
  construction for the `runuser` (or equivalent) invocation.

### Dependencies

M4c; postinstall hook blocked on user direction.

---

## Open items (need user decision)

- **F1 help screen content** — what to show (keybindings only? per-step
  help? both?).
- **Postinstall hook** (M5 blocker) — script path on disk, package that
  installs it, invocation (`runuser -u <user> --` vs systemd user unit),
  HOME/XDG env injection.
