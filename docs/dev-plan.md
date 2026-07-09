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

- `src/steps/language.rs` — list `UiLang::En` / `UiLang::ZhCn` via
  `UiLang::label()`; on change call `set_language()` so the rest of the UI
  re-translates live; write `state.ui_lang`.
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
  retry. Store the chosen source lines in `state.mirror_lines`.
- `src/util/locale_list.rs` — parse `/etc/locale.gen` for the full locale
  list (used later in M4b; built here because the language step surfaces
  locale concepts).
- `src/util/process.rs` — suspend-ratatui/run-subprocess/resume helper
  shared by `nmtui`, `cfdisk`, `pacman -Sy`.
- Sample runtime config files in the repo: `config/packages.list` and
  `config/repo.conf` — these are the templates the user's PKGBUILD will
  install to `/etc/clipsneko-installer/`. Needed from M1 onward because the
  installer exits if they are missing at startup.

### Acceptance

- Language picker switches the whole UI between English and 简体中文 live;
  the zh_CN `.mo` is visually verified.
- Keyboard list loads from `localectl`; selecting one runs `loadkeys` and
  the effect is visible in the next text input.
- `nmtui` opens full-screen, returns to the wizard, and the connectivity
  check correctly reports online/offline.
- Region list loads from `/etc/pacman.d/mirrorlist`; selecting a region
  reorders the file with that region's mirrors on top; manual-entry path
  accepts a `Server =` line; `pacman -Sy` validation succeeds/fails as
  appropriate and the UI lets the user retry on failure.

### Unit tests

- `locale_list` — parse a fixture `/etc/locale.gen` and return the enabled
  locales (ignoring comments and blank lines).
- `util::process` — the suspend/resume bookkeeping (not the actual spawn);
  e.g. the helper restores raw mode even if the subprocess errors.
- Mirror `Server =` line format validation (regex / structure, not the
  network fetch).

### Dependencies

M0 (done).

---

## M2 — Disk partitioning

The disk step (step 5). The most complex single step: cfdisk integration,
partition-table re-read, role auto-suggestion with override, ESP
no-reformat rule, and optional extra mounts.

### Deliverables

- `src/steps/disk.rs` — disk selection, existing-partition-table warning +
  double-confirm, `cfdisk /dev/<disk>` invocation, post-cfdisk
  `partprobe` + re-read, role auto-suggest + override UI, ESP no-reformat
  guard, optional extra-partition mapping (device + mountpoint + format
  choice).
- `src/util/lsblk.rs` — parse `lsblk -J -O` JSON into a typed partition
  list (name, fstype, parttype, size, mountpoints).

### Acceptance

- Disk selection lists block devices from `lsblk`.
- Selecting a disk with an existing partition table shows the warning and
  requires explicit confirmation before entering cfdisk.
- `cfdisk` opens on the selected disk (GPT label if empty); on return the
  installer re-reads the partition table and shows the new layout.
- Auto-suggest correctly flags the single vfat+ESP partition as ESP and
  the single btrfs partition as root; ambiguous layouts surface a
  per-partition picker the user can override.
- An ESP that already has `TYPE=vfat` is **not** reformatted; a non-vfat
  partition assigned the ESP role is formatted with a warning.
- Extra partitions can be mapped to mountpoints (e.g. `/home` on a second
  disk).

### Unit tests

- `util::lsblk` — parse a fixture `lsblk -J -O` blob into the typed
  structure (including the ESP parttype UUID `c12a7328-...`).
- Partition role suggestion — given a list of partitions, the auto-suggest
  returns the correct ESP/root candidates and flags ambiguity.
- ESP no-reformat decision — given a partition's `blkid` TYPE and parttype,
  returns reformat / skip / warn.

### Dependencies

M0 (disk step does not require M1 to be complete, but running it end-to-end
needs network from M1 for a fully populated state).

---

## M3 — Selection & identity

Steps 6-11: kernel, nvidia, timezone, user, hostname, confirm. After M3
the wizard holds a complete, validated pre-install configuration and the
confirm screen can show a full summary.

### Deliverables

- `src/steps/kernel.rs` — single-select from `linux` / `linux-lts` /
  `linux-zen` / `linux-hardened`.
- `src/steps/nvidia.rs` — "no nvidia" or one variant from
  `nvidia` / `nvidia-dkms` / `nvidia-open-dkms` / `nvidia-lts`, with
  incompatible options disabled based on the chosen kernel (see the matrix
  in `design.md` §4 step 7).
- `src/steps/timezone.rs` — `curl -s http://ip-api.com/json` → `timezone`
  field default; fallback UTC; manual override by typing `Region/City` or
  picking from `/usr/share/zoneinfo/`.
- `src/steps/user.rs` — username (`^[a-z_][a-z0-9_-]*$`), optional GECOS,
  password + confirm with a strength bar; writes `state.user` (password
  itself never stored, only `password_set`).
- `src/steps/hostname.rs` — input validated
  `^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$`.
- `src/steps/confirm.rs` — full summary of all state; linear Back/Next;
  final blocking "this will format disks" dialog before handing off to the
  install step.
- `src/util/geoip.rs` — ip-api.com fetch + JSON parse.
- `src/util/password.rs` — strength heuristic (length, char classes,
  common-password check) returning a weak/fair/good/strong level.

### Acceptance

- Kernel single-select records the choice in `state.kernel`.
- Nvidia variant list is correctly filtered by the chosen kernel;
  incompatible options are visible but not selectable.
- Timezone defaults to the GeoIP result when online, UTC when not; manual
  override accepts only `Region/City` strings that exist under
  `/usr/share/zoneinfo/`.
- Username validation rejects invalid names live; password strength bar
  updates as the user types; confirm mismatch blocks Next.
- Hostname validation rejects invalid input live.
- Confirm screen shows every choice; the blocking dialog appears before
  the install step.

### Unit tests

- Nvidia-kernel compatibility matrix (all 4 kernels × all 4 variants).
- Username regex (valid/invalid boundary cases).
- Hostname regex (length, leading/trailing hyphen, uppercase rejection).
- Password strength heuristic (empty, short, all-lower, mixed, common
  password from a small fixture list).
- `util::geoip` JSON parse on a fixture response (and a malformed one).

### Dependencies

M2 (confirm reads disk state; nvidia reads kernel state).

---

## M4 — Install stage

Split into three sub-milestones. Each builds on the previous; none is
usable standalone for a full install until M4c lands.

### M4a — Partition, format, mount

#### Deliverables

- `src/installer/partition.rs` — for each selected partition: format if
  needed (root `mkfs.btrfs -f`; ESP `mkfs.vfat -F32` only if not already
  vfat); create subvolumes `@` and `@home`; remount root with
  `-o compress=zstd:1,subvol=@`; mount `@home` at `/mnt/home`; mount ESP
  at `/mnt/boot/efi`; mount extra partitions per the disk step's mapping.

#### Acceptance

- Root partition is formatted as btrfs with `@` and `@home` subvolumes.
- Root is mounted with `compress=zstd:1,subvol=@`; `@home` at `/mnt/home`.
- Existing-vfat ESP is mounted without reformat; non-vfat ESP is formatted
  then mounted, at `/mnt/boot/efi`.
- Extra partitions mount at their assigned mountpoints.

#### Unit tests

- Btrfs mount-option string construction
  (`compress=zstd:1,subvol=@` for root, `subvol=@home` for home).
- Subvolume path computation (`/mnt` vs `/mnt/home` given subvol names).
- ESP reformat decision (reused from M2's test, applied at format time).

#### Dependencies

M3 (full state).

### M4b — Pacstrap & chroot config

#### Deliverables

- `src/installer/pacstrap.rs` — append `[clipsneko]` section to the live
  `/etc/pacman.conf` from `repo.conf`; construct and run the `pacstrap`
  command from state (base, base-devel, chosen kernel, linux-firmware,
  `packages.list` contents, chosen nvidia package, grub, grub-btrfs,
  efibootmgr, zsh, grml-zsh-config, sudo, networkmanager, nano, vi);
  run `genfstab -U /mnt >> /mnt/etc/fstab` and ensure btrfs entries carry
  `compress=zstd:1`.
- `src/installer/chroot.rs` — under `arch-chroot /mnt`: timezone symlink +
  `hwclock --systohc`; `/etc/locale.gen` per state → `locale-gen`; write
  `/etc/locale.conf` and `/etc/vconsole.conf`; `/etc/hostname` + `/etc/hosts`;
  `passwd -l root`; `useradd -m -G wheel -s /bin/zsh` + `chpasswd`;
  uncomment `%wheel ALL=(ALL:ALL) ALL` in `/etc/sudoers`; copy live
  mirrorlist to target; append `[clipsneko]` to target `/etc/pacman.conf`;
  remove `kms` from `HOOKS` in `/etc/mkinitcpio.conf` if nvidia was
  installed; `mkinitcpio -P`.

#### Acceptance

- Live `pacman.conf` gains the `[clipsneko]` section with the configured
  `Server` and `SigLevel = Never`.
- `pacstrap` installs exactly the packages derived from state + the
  external `packages.list`.
- `/mnt/etc/fstab` is generated and btrfs entries carry `compress=zstd:1`.
- Inside the chroot: timezone, locale, vconsole, hostname, hosts, root
  lock, user creation, sudoers, mirrorlist copy, target pacman.conf, and
  mkinitcpio (with `kms` removed when nvidia is chosen) are all applied.

#### Unit tests

- `pacstrap` argument list construction from a fixture `InstallerState`
  and `packages.list`.
- `mkinitcpio.conf` HOOKS `kms`-removal (string edit, idempotent).
- `/etc/locale.gen` editing (enable a set of locales).
- `/etc/sudoers` `%wheel` uncomment logic (string edit).
- `[clipsneko]` pacman.conf section text generation from `repo.conf`.
- `genfstab` output post-processing to guarantee `compress=zstd:1` on
  btrfs lines.

#### Dependencies

M4a.

### M4c — Bootloader & finalize

#### Deliverables

- `src/installer/bootloader.rs` —
  `grub-install --target=x86_64-efi --efi-directory=/boot/efi --bootloader-id=clipsneko`;
  `grub-mkconfig -o /boot/grub/grub.cfg`;
  `systemctl enable NetworkManager` (inside the chroot);
  prompt "Reboot now?"; on yes `umount -R /mnt && reboot`, on no drop an
  info message to the live root shell.

#### Acceptance

- GRUB is installed to the ESP with the `clipsneko` bootloader ID.
- `grub.cfg` is generated and references the btrfs root subvolume.
- `NetworkManager.service` is enabled on the target.
- The reboot prompt offers yes/no; yes unmounts and reboots, no returns
  to a usable live shell.

#### Unit tests

- `grub-install` argument list construction.
- `grub-mkconfig` output path.
- Reboot-decision state machine (yes → umount+reboot, no → shell).

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
- `CLIPSNEKO_LOG_FILE` and/or `CLIPSNEKO_CONFIG_DIR` env-var overrides for
  non-root dev testing, **if** the user approves (open items below).
- Final end-to-end install test on a test VM: a full run from language
  pick through reboot produces a bootable ClipsNeko system.

### Acceptance

- The postinstall hook runs the specified script as the new user inside
  the chroot, with the agreed env, and its output is captured to the log.
- F1 shows a help screen listing all keybindings.
- (If approved) the binary runs without root for dev by pointing
  `CLIPSNEKO_LOG_FILE` and `CLIPSNEKO_CONFIG_DIR` at writable paths.
- A full end-to-end install on a VM boots into a working system with the
  created user, locked root, zsh shell, NetworkManager, and GRUB.

### Unit tests

- Depends on the postinstall hook's design — at minimum, the argument/env
  construction for the `runuser` (or equivalent) invocation.

### Dependencies

M4c; postinstall hook blocked on user direction.

---

## Open items (need user decision)

- **`CLIPSNEKO_LOG_FILE` override** — allow a non-root log path for dev
  testing, or keep `/var/log/clipsneko-installer.log` root-only?
- **`CLIPSNEKO_CONFIG_DIR` override** — allow a non-`/etc/clipsneko-installer/`
  config dir for dev testing, or require the sample `config/*` files to be
  copied to `/etc/` manually?
- **F1 help screen content** — what to show (keybindings only? per-step
  help? both?).
- **Postinstall hook** (M5 blocker) — script path on disk, package that
  installs it, invocation (`runuser -u <user> --` vs systemd user unit),
  HOME/XDG env injection.
