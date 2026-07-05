# Development Progress

## 2026-07-05 — Kickoff

### Done

- Defined project scope and target runtime (ClipsNeko Live ISO, UEFI-only,
  64-bit).
- Locked tech stack: Rust (Edition 2021) + ratatui + crossterm; gettext-rs for
  i18n (en as POT source, zh_CN as first translation); anyhow/thiserror for
  errors; tracing + tracing-subscriber for logging to
  `/var/log/clipsneko-installer.log`.
- Locked external runtime config (must exist or the installer exits with a
  clear error):
  - `/etc/clipsneko-installer/packages.list` — one package name per line.
  - `/etc/clipsneko-installer/repo.conf` — `name`, `server_url`, `siglevel`
    (default `Never` for the debug phase; the ClipsNeko repository itself is
    not built yet — only the interface shape is locked).
- Locked the 12-step linear wizard (Back/Next only) covering: UI language,
  keyboard, network (nmtui), mirror (reflector `--latest 20 --sort rate
  --protocol https` or manual input, validated by `pacman -Sy` exit code),
  disk (cfdisk + btrfs `@`/`@home` with `compress=zstd:1`, ESP not reformatted
  if already vfat, optional extra partitions), kernel
  (linux/linux-lts/linux-zen/linux-hardened), nvidia (variant chosen from a
  kernel-dependent compatibility matrix), timezone (ip-api.com GeoIP + manual
  override), single zsh user in `wheel` with locked root, hostname, confirm
  page, install.
- Locked bootloader: GRUB (UEFI, `--bootloader-id=clipsneko`) with grub-btrfs;
  ESP at `/mnt/boot/efi`, kernel stays under `/boot`.
- Locked mkinitcpio handling: no MODULES additions needed. The default
  `filesystems` HOOK + btrfs-progs already cover btrfs, and current nvidia
  packages ship everything required. After installing the chosen nvidia package,
  the only change is to remove `kms` from `HOOKS` in `/etc/mkinitcpio.conf`,
  then run `mkinitcpio -P`. No `/etc/modprobe.d/nvidia-drm.modprobe`.
- Locked keybindings: Tab/Shift+Tab focus, Up/Down/j/k list nav, Space toggle,
  Enter confirm/advance, Esc back, on-screen Next/Back buttons, Ctrl+C exit,
  F1 help; install phase shows spinner + progress text with logs only to file
  and `L` to view the log afterwards.
- Wrote `AGENTS.md` (project conventions: English-only code/docs, no UI string
  hardcoding, no wheel reinvention, mandatory `docs/dev-prog.md` updates, no
  agent-side git writes, fmt+clippy+build verification).
- Wrote `docs/design.md` capturing all of the above.

### Not done

- No Rust source tree, no `Cargo.toml`, no `po/` scaffolding yet — the project
  is documentation-only at this point.
- The ClipsNeko package repository is not yet built; only the `repo.conf`
  interface is specified.
- The deferred "postinstall script run as the new user" hook is unspecified.

### Next

- User reviews and commits `AGENTS.md` and `docs/design.md`.
- Initialize the Cargo project: `Cargo.toml` with ratatui, crossterm, gettext-rs,
  anyhow, thiserror, tracing, tracing-subscriber; `src/main.rs` stub that
  inits i18n + logging and renders an empty ratatui screen.
- Set up `po/` scaffolding: `clipsneko-installer.pot`, `en/LC_MESSAGES/`,
  `zh_CN/LC_MESSAGES/`.
- Implement the linear wizard shell (`app.rs` + `state.rs`) with stubbed step
  modules so navigation works end-to-end before any real system logic lands.
