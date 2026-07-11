# ClipsNeko Linux Installer — Design

Status: draft v0.1 — locked decisions recorded here are authoritative unless the
user amends them in writing. Pending items are explicitly marked **(deferred)**.

## 1. Scope and runtime

- TUI installer for ClipsNeko Linux (Arch derivative).
- Runs on the ClipsNeko Live ISO. **UEFI-only, 64-bit.**
- Lightweight: single Rust binary; all system work via existing tools.
- Runs as a **normal user** (either `root` on a root shell or the
  passwordless `installer` user on the ISO); commands needing root go
  through `sudo` automatically (see §9).

## 2. Stack

See `AGENTS.md` §2. Highlights:

- Rust + ratatui + crossterm; gettext-rs; anyhow/thiserror; tracing.
- Runtime config: `/etc/clipsneko-installer/packages.list`.
- The Live ISO's `/etc/pacman.conf` already configures the ClipsNeko package
  repository. The installer reuses and copies it through `pacstrap -P`.

## 3. Project layout

```
src/
  main.rs app.rs state.rs i18n.rs
  steps/    language keyboard network mirror disk
            kernel nvidia timezone user hostname confirm install
  installer/ partition pacstrap chroot mkinitcpio bootloader postinstall
  util/     process lsblk ui geoip password locale_list
config/     packages.list
po/         clipsneko-installer.pot
            en/LC_MESSAGES/clipsneko-installer.po
            zh_CN/LC_MESSAGES/clipsneko-installer.po
docs/       dev-plan.md dev-prog.md design.md
AGENTS.md
```

## 4. Wizard flow

Linear: Back/Next only, no per-item jump from the confirm page.

1. **Language and locale** — two independent lists on one step:
   - Installer UI language: en / zh_CN. Space applies the highlighted language
     live through gettext; it changes `LC_MESSAGES` only.
   - Target-system locale: every UTF-8 locale parsed from `/etc/locale.gen`,
     defaulting to `en_US.UTF-8`; stored separately in state for M4b.
   - Tab/Shift+Tab moves between the two lists and footer buttons. Enter on the
     UI list applies it and moves to the target list; Enter on the target list
     records it and advances. Locale/catalog failures are fatal Live ISO
     invariant failures, with no language fallback.
2. **Keyboard** — list from `localectl list-keymaps`; `loadkeys` immediately;
   persisted to target `/etc/vconsole.conf`.
3. **Network** — suspend ratatui, run `nmtui`; on return verify with
   `curl -sI http://ip-api.com/json`. Required to proceed.
4. **Mirror**
   - Parse `/etc/pacman.d/mirrorlist` (assumed present and well-formed on
     the ISO) into region blocks (`## <Region>` header + `Server =` lines).
   - Show a single-select list of region names; selecting a region moves
     that region's `Server =` lines to the top of the file, ahead of all
     other regions (file header comments preserved). Alternatively, a
     manual `Server =` URL input field below the list. A manual entry becomes
     the sole active server so `pacman -Sy` validates that entry rather than
     silently falling back to a region server.
   - Tab toggles focus between the list and the input field.
   - On Next: rewrite the mirrorlist, validate with `pacman -Sy` (exit 0 =
     ok). On failure, show a modal error dialog; dismiss and retry.
5. **Disk** — two sub-pages within the same step. There is **no** auto-suggested
   role assignment; every role is chosen by the user by hand.

   Sub-page A (disk picker):
   - Read a fixed lsblk JSON schema containing device, model, transport, size,
     removable/read-only state, mountpoints, filesystem, GPT type, and label.
     Exclude zram pseudo-disks. Show physical candidates in a responsive table.
   - The disk containing `/run/archiso/` and read-only disks remain visible but
     disabled. Other removable disks remain selectable.
   - Enter opens `cfdisk /dev/<disk>` full-screen (via `sudo` when not root);
     on return all prior role assignments are cleared, then the installer runs
     `partprobe` and re-reads lsblk. A non-zero partprobe result is a blocking,
     retryable disk error; spawn failure is fatal.
   - The user may run cfdisk against multiple disks before leaving the page.
   - The on-screen Next button advances to sub-page B.

   Sub-page B (partition role picker):
   - List every partition in a responsive device/size/filesystem/label/role
     table. Partitions belonging to disabled disks are protected and cannot be
     assigned.
   - Selecting a partition (Enter) pops a small dialog asking the user to
     assign it the **ESP**, **Target**, or explicit **Unassigned** role.
     ESP is single-select (assigning a new ESP clears the old one); Target is
     multi-select (choosing two or more Target partitions enables btrfs RAID
     at format time — see §5). The roles are mutually exclusive for a given
     partition. The ESP must carry the GPT ESP type UUID.
   - With multiple Targets, Next asks for the btrfs data profile (`raid0` or
     `raid1`; metadata remains `raid1`). Usable capacity is checked against the
     strict `> 20 GiB` requirement: RAID0 is conservatively limited by the
     smallest-device stripe size; RAID1 is limited by two-copy overhead and
     space outside the largest device.
   - Before leaving, a blocking dialog lists **every** Target because all are
     formatted as btrfs, plus the ESP only when it is not already vfat. The
     user must explicitly confirm data loss.
   - There is no extra-partition / extra-mount mapping in v0.1.
6. **Kernel** — `linux` / `linux-lts` / `linux-zen` / `linux-hardened` (single
   select). Default: `linux-zen`. The matching headers package is always
   installed with the selected kernel: `linux-headers`, `linux-lts-headers`,
   `linux-zen-headers`, or `linux-hardened-headers`.
7. **NVIDIA** — "no NVIDIA" OR one variant from the compatible matrix below
   (incompatible options disabled in the UI). Default: `nvidia-open-dkms`.
   Disabled variants are dimmed, carry an "incompatible with selected kernel"
   suffix, and are skipped by keyboard navigation. If a user returns to the
   kernel step and makes the saved NVIDIA choice incompatible, entering the
   NVIDIA step automatically resets it to the compatible default
   `nvidia-open-dkms`.

   | kernel          | allowed NVIDIA packages                 |
   |-----------------|-----------------------------------------|
   | linux           | nvidia-open / nvidia-open-dkms          |
   | linux-lts       | nvidia-open-lts / nvidia-open-dkms      |
   | linux-zen       | nvidia-open-dkms                        |
   | linux-hardened  | nvidia-open-dkms                        |

   Kernel headers are already included unconditionally by the kernel choice;
   NVIDIA selection only contributes the selected driver package.

8. **Timezone** — `curl --max-time 5 --fail --silent --show-error
   http://ip-api.com/json` provides the initial `timezone`; failed or
   unsupported detection falls back to `UTC`. The available values come from
   `timedatectl list-timezones` and are presented as two side-by-side lists.
   The first list contains `Africa`, `America`, `Antarctica`, `Arctic`, `Asia`,
   `Atlantic`, `Australia`, `Europe`, `Indian`, `Pacific`, and the direct
   `UTC` choice. This excludes legacy top-level aliases and the `Etc`
   compatibility namespace. Selecting a geographic region enables the second
   list of full timezone names such as `Asia/Shanghai`; selecting `UTC` dims
   and disables the second list. Up/Down moves within a list, Right or Enter
   enters the timezone list, Left returns to the region list, and Enter on a
   concrete timezone (or on `UTC`) applies it and continues. Tab/Shift+Tab
   traverses both lists and then the footer. Returning to the step restores
   the saved timezone without repeating GeoIP detection. There is no manual
   timezone text input.
9. **User** — single user:
   - username validated `^[a-z_][a-z0-9_-]*$`
   - GECOS optional
   - password + confirm (strength bar)
   - Created as `useradd -m -G wheel -s /bin/zsh <user>`; `%wheel` line
     uncommented in `/etc/sudoers`; root locked (`passwd -l root`).
10. **Hostname** — input validated `^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$`.
11. **Confirm** — full summary; linear Back/Next; final blocking dialog "This
    will format disks. Continue?".
12. **Install** — see §5.

## 5. Install stage

12.1 Format & mount:

- root: if a single Target partition was chosen, `mkfs.btrfs -f <part>`. If two
  or more Target partitions were chosen, use the data RAID mode already chosen
  in the disk step (`raid0` or `raid1`; metadata is always `raid1`) and run
  `mkfs.btrfs -f -d <mode> -m raid1 <part1> <part2> ...`. Create
  subvolumes `@`, `@home`; remount root with `-o compress=zstd:1,subvol=@`;
  `@home` at `/mnt/home`.
- ESP: skip if already vfat, else `mkfs.vfat -F32`; mount at `/mnt/boot/efi`.
- No extra-partition mapping in v0.1 (see §4 step 5).

12.2 Package source — use the Live ISO's existing `/etc/pacman.conf`, which
already contains the ClipsNeko repository. Packages with names beginning in
`clipsneko-` may therefore be listed in `packages.list` like ordinary packages.
The installer does not parse or generate repository configuration.

12.3 `pacstrap -P /mnt <packages.list contents> <chosen kernel>
<matching kernel headers> linux-firmware <chosen NVIDIA package>`.
`packages.list` is the authoritative static package set; the installer only
adds packages derived from wizard state. `-P` copies the Live ISO's
`pacman.conf` and `pacman.d` configuration to the target.

12.4 `genfstab -U /mnt >> /mnt/etc/fstab` — verify btrfs entries carry
`compress=zstd:1,subvol=@` / `subvol=@home`.

12.5 `arch-chroot /mnt`:

- timezone symlink + `hwclock --systohc`
- enable the selected target locale in `/etc/locale.gen`, run `locale-gen`,
  write `/etc/locale.conf` (`LANG=...`) and `/etc/vconsole.conf` (`KEYMAP=...`)
- `/etc/hostname` + `/etc/hosts`
- `passwd -l root`
- `useradd -m -G wheel -s /bin/zsh <user>`; `chpasswd`
- uncomment `%wheel ALL=(ALL:ALL) ALL` in `/etc/sudoers`
- use the pacman configuration copied by `pacstrap -P`
- mkinitcpio: **if NVIDIA was installed, remove `kms` from HOOKS in
  `/etc/mkinitcpio.conf`**; then `mkinitcpio -P`. (No MODULES additions needed:
  the default `filesystems` HOOK + btrfs-progs already cover btrfs, and current
  NVIDIA packages need no MODULES entries.)
- `grub-install --target=x86_64-efi --efi-directory=/boot/efi
  --bootloader-id=clipsneko`
- `grub-mkconfig -o /boot/grub/grub.cfg`
- `systemctl enable NetworkManager`
- **postinstall hook (deferred)** — see §7

12.6 Prompt "Reboot now?" — `y` → `umount -R /mnt && reboot`; `n` → drop info to
root shell on live env.

## 6. Keybindings

- **Tab / Shift+Tab**: cycle focus between the step body and the on-screen
  Back/Next buttons. Disabled buttons are skipped during focus cycling.
- **Up / Down** (or **j / k**): list navigation (step body only).
- **Space**: toggle / select the highlighted item (step body only).
- **Enter**: in the step body, confirm / select / advance (a step may emit
  `Next` to advance, so Enter still works without Tab-ing to the Next
  button); on a focused button, activate it. Activating the Next button goes
  through the same per-step commit/validation path as body Enter.
- **Esc**: cancel the active modal; otherwise follow the Back-button path
  (internal disk page first, then previous wizard step).
- **Ctrl+C**: open quit confirmation from any page or step-owned modal.
- **Back button**: go to the previous step (disabled on the first step).
- **Next button**: go to the next step (disabled on the last step).
- **F1**: help (not implemented or advertised in the footer yet).
- Install phase: **Spinner + progress text** on screen, log only to file;
  **L**: view log after completion.

The quit-confirmation dialog shows `[ Cancel ]` and `[ Quit ]`, initially
focused on Cancel. Left/Right or Tab changes focus, Enter activates the focused
button, and Esc always cancels.

Step-owned modal dialogs receive all keyboard input before global shortcuts or
footer focus. Esc therefore cancels the active step dialog, and Tab cannot
activate controls behind it.

## 7. Deferred items (pending user direction)

- The "postinstall script run as the new user" inside chroot: location on disk,
  package that installs it, invocation (`runuser -u <user> --`? systemd user
  unit?), HOME/XDG env injection.
- Desktop environment / display manager selection (out of scope for v0.1).
- Password-strength algorithm tune-up (initial: lightweight heuristic).
- Install-failure rollback.

Password handoff is locked: keep the confirmed password only in a dedicated
in-memory `SecretString` that does not implement `Debug`; pipe
`<username>:<password>` to `chpasswd` through stdin; never place it in command
arguments, summaries, tracing fields, or logs. On success, zeroize it
immediately; its `Drop` implementation zeroizes again on failure or early exit.
The `zeroize` crate is justified when the user step is implemented.

## 8. i18n workflow

- `en` is the POT source.
- Add a UI string → wrap in `t!(...)`; update `.pot` and both `.po` files in
  the same change.
- `zh_CN` must not lag `en` by more than one session.
- Changing the installer language sets `LC_MESSAGES` only; it does not alter
  other process locale categories or the target-system locale.
- Debug builds load build-generated catalogs from OUT_DIR. Release builds use
  the GNU-standard `/usr/share/locale` path with no runtime path override.

## 9. Privilege model and logging

### Privilege model

The installer runs as a **normal user**, not as root (unless the user
explicitly launched it from a root shell). On the ClipsNeko ISO the
`installer` user is in `sudoers` and is passwordless; `root` is also
passwordless. This means `sudo` never prompts.

Commands that require root privileges (disk partitioning, formatting,
mounting, `pacstrap`, `arch-chroot`, `grub-install`, `mkinitcpio`,
`genfstab`, `partprobe`, `pacman`, `localectl`,
`systemctl`, `loadkeys`, `cfdisk`, …) are wrapped via
`util::process::privileged_command(program)`: when the effective UID is 0
the command runs directly, otherwise `sudo -- <program>` is used.

Commands that do not require root (`nmtui` via polkit, HTTP fetches to
`ip-api.com`, reading `/etc/clipsneko-installer/*` config files, reading
`/usr/share/zoneinfo/`) are invoked with a plain `Command::new(...)`.

This is the required pattern for all future modules that shell out — see
`AGENTS.md` §2 and the `util::process` module.

### Error boundary

- Live ISO invariants are fatal and propagate with context after restoring the
  terminal: missing commands/config/catalogs, sudo/spawn failure, malformed
  fixed command output, missing locales/keymaps, and privileged file-write
  failure. These do not receive fallback UI.
- User/external states remain recoverable in the TUI: offline connectivity,
  invalid or unreachable mirror input, user cancellation, destructive-action
  confirmation, and a non-zero partprobe result caused by device state.
- Terminal restoration is always attempted and any restoration failure is
  fatal; the app never continues with an unknown terminal state.

### Logging

- Log file: `$XDG_CACHE_HOME/clipsneko-installer/log`, falling back to
  `$HOME/.cache/clipsneko-installer/log`. The path is **fixed** (no
  env-var override) so the binary runs without root on any user account.
- A `panic` hook restores the terminal (disables raw mode, leaves the
  alternate screen) so a crash never leaves the user stuck in a dead
  terminal.
- Before entering the alternate screen, startup verifies that the required
  `/etc/clipsneko-installer/packages.list` runtime file exists.
