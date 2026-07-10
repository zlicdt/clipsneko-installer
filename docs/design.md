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
- Runtime config: `/etc/clipsneko-installer/packages.list`, `repo.conf`.

## 3. Project layout

```
src/
  main.rs app.rs state.rs repo_conf.rs i18n.rs
  steps/    language keyboard network mirror disk
            kernel nvidia timezone user hostname confirm install
  installer/ partition pacstrap chroot mkinitcpio bootloader postinstall
  util/     process lsblk geoip password locale_list
po/         clipsneko-installer.pot
            en/LC_MESSAGES/clipsneko-installer.po
            zh_CN/LC_MESSAGES/clipsneko-installer.po
docs/       dev-prog.md  design.md
AGENTS.md
```

## 4. Wizard flow

Linear: Back/Next only, no per-item jump from the confirm page.

1. **UI language** — en / zh_CN. Only changes installer display language.
   Up/Down (or j/k) moves the highlight; Space selects the highlighted
   language and calls `set_language()` immediately so the whole UI
   re-translates live; Enter selects and advances. Default highlight is
   English on entry. The choice is not persisted across runs (Live ISO
   starts fresh; target-system locale is configured in §5). The ISO build
   must generate both `en_US.UTF-8` and `zh_CN.UTF-8`; `set_language()`
   failure is defensive-only and falls back to English (logged via
   `tracing`).
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
     manual `Server =` URL input field below the list.
   - Tab toggles focus between the list and the input field.
   - On Next: rewrite the mirrorlist, validate with `pacman -Sy` (exit 0 =
     ok). On failure, show a modal error dialog; dismiss and retry.
5. **Disk** — two sub-pages within the same step. There is **no** auto-suggested
   role assignment; every role is chosen by the user by hand.

   Sub-page A (disk picker):
   - List every block device of type `disk` from `lsblk -J -O -b` (name on the
     left, human-readable size on the right).
   - Enter opens `cfdisk /dev/<disk>` full-screen (via `sudo` when not root);
     on return the installer runs `partprobe` and re-reads `lsblk -J -O -b`.
   - The user may run cfdisk against multiple disks before leaving the page.
   - The on-screen Next button advances to sub-page B.

   Sub-page B (partition role picker):
   - List every partition of type `part` on every disk from the latest `lsblk`
     (name / size / current FSTYPE).
   - Selecting a partition (Enter) pops a small dialog asking the user to
     assign it the **ESP** role or the **Target** role (or cancel).
     ESP is single-select (assigning a new ESP clears the old one); Target is
     multi-select (choosing two or more Target partitions enables btrfs RAID
     at format time — see §5).
   - The Next button is enabled only when an ESP is assigned and the total size
     of all Target partitions exceeds 20 GiB.
   - Pressing Next, if any Target partition currently has a non-empty FSTYPE
     (it will be reformatted as btrfs → data loss) or the ESP partition is not
     already vfat (it will be `mkfs.vfat -F32`'d), shows a single blocking
     confirmation dialog listing every partition that will be wiped; the user
     must confirm to leave the step. A pure-vfat ESP partition incurs no
     warning and is not reformatted.
   - There is no extra-partition / extra-mount mapping in v0.1.
6. **Kernel** — `linux` / `linux-lts` / `linux-zen` / `linux-hardened` (single
   select).
7. **nvidia** — "no nvidia" OR one variant from the compatible matrix below
   (incompatible options disabled in the UI). Default: `nvidia-dkms`.

   | kernel          | allowed nvidia packages                                  |
   |-----------------|----------------------------------------------------------|
   | linux           | nvidia, nvidia-dkms, nvidia-open-dkms, nvidia-lts        |
   | linux-lts       | nvidia-lts, nvidia-dkms, nvidia-open-dkms                |
   | linux-zen       | nvidia-dkms, nvidia-open-dkms                            |
   | linux-hardened  | nvidia-dkms, nvidia-open-dkms                            |

8. **Timezone** — `curl -s http://ip-api.com/json` → `timezone` field; fallback
   UTC; user may override by typing `Region/City` or picking from
   `/usr/share/zoneinfo/`.
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
  or more Target partitions were chosen, the installer prompts the user to
  pick the data RAID mode (`raid0` or `raid1`; metadata is always `raid1`),
  then runs `mkfs.btrfs -f -d <mode> -m raid1 <part1> <part2> ...`. Create
  subvolumes `@`, `@home`; remount root with `-o compress=zstd:1,subvol=@`;
  `@home` at `/mnt/home`.
- ESP: skip if already vfat, else `mkfs.vfat -F32`; mount at `/mnt/boot/efi`.
- No extra-partition mapping in v0.1 (see §4 step 5).

12.2 Live `pacman.conf` — append `[clipsneko]` section using `repo.conf`
(`SigLevel = Never` for the debug phase).

12.3 `pacstrap /mnt base base-devel <chosen kernel> linux-firmware
<packages.list contents> <chosen nvidia pkg> grub grub-btrfs efibootmgr zsh
grml-zsh-config sudo networkmanager nano vi`

12.4 `genfstab -U /mnt >> /mnt/etc/fstab` — verify btrfs entries carry
`compress=zstd:1,subvol=@` / `subvol=@home`.

12.5 `arch-chroot /mnt`:

- timezone symlink + `hwclock --systohc`
- `/etc/locale.gen` per state list → `locale-gen`; write `/etc/locale.conf`
  (`LANG=...`) and `/etc/vconsole.conf` (`KEYMAP=...`)
- `/etc/hostname` + `/etc/hosts`
- `passwd -l root`
- `useradd -m -G wheel -s /bin/zsh <user>`; `chpasswd`
- uncomment `%wheel ALL=(ALL:ALL) ALL` in `/etc/sudoers`
- copy live `/etc/pacman.d/mirrorlist` → target
- append `[clipsneko]` section to `/mnt/etc/pacman.conf`
- mkinitcpio: **if nvidia was installed, remove `kms` from HOOKS in
  `/etc/mkinitcpio.conf`**; then `mkinitcpio -P`. (No MODULES additions needed:
  the default `filesystems` HOOK + btrfs-progs already cover btrfs, and current
  nvidia packages need no MODULES entries.)
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
  button); on a focused button, activate it.
- **Esc**: request quit — opens the quit-confirmation dialog. Esc is no
  longer used for going back.
- **Ctrl+C**: same as Esc — opens the quit-confirmation dialog.
- **Back button**: go to the previous step (disabled on the first step).
- **Next button**: go to the next step (disabled on the last step).
- **F1**: help (not implemented yet).
- Install phase: **Spinner + progress text** on screen, log only to file;
  **L**: view log after completion.

The quit-confirmation dialog shows a `[ Quit ]` button and the hint
"Esc to cancel, Enter to quit." Enter confirms and exits; Esc cancels and
returns to the wizard. `Y` is no longer used.

## 7. Deferred items (pending user direction)

- The "postinstall script run as the new user" inside chroot: location on disk,
  package that installs it, invocation (`runuser -u <user> --`? systemd user
  unit?), HOME/XDG env injection.
- Desktop environment / display manager selection (out of scope for v0.1).
- Password-strength algorithm tune-up (initial: lightweight heuristic).
- Install-failure rollback.

## 8. i18n workflow

- `en` is the POT source.
- Add a UI string → wrap in `t!(...)`; update `.pot` and both `.po` files in
  the same change.
- `zh_CN` must not lag `en` by more than one session.

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

### Logging

- Log file: `$XDG_CACHE_HOME/clipsneko-installer/log`, falling back to
  `$HOME/.cache/clipsneko-installer/log`. The path is **fixed** (no
  env-var override) so the binary runs without root on any user account.
- A `panic` hook restores the terminal (disables raw mode, leaves the
  alternate screen) so a crash never leaves the user stuck in a dead
  terminal.
