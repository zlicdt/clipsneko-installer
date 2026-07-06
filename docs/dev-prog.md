# Development Progress

This file tracks the **current state** of the project, not a chronological
log. Items live in exactly one of two lists; when a piece of work is
finished it moves from "Not done" to "Done" and stays there.

## Done

- Project scope and runtime locked: ClipsNeko Linux Live ISO, **UEFI-only,
  64-bit**.
- Tech stack locked: Rust (Edition 2021) + `ratatui` + `crossterm`;
  `gettext-rs` with the `gettext-system` feature (links the system gettext;
  vendored gettext 0.26 fails against current glibc headers) — `en` is the
  POT source, `zh_CN` the first translation; `anyhow` + `thiserror`;
  `tracing` + `tracing-subscriber` → `/var/log/clipsneko-installer.log`.
- External runtime config locked (must exist or the installer exits with a
  clear error): `/etc/clipsneko-installer/packages.list` (one package name
  per line) and `/etc/clipsneko-installer/repo.conf` (`name`, `server_url`,
  `siglevel` = `Never` for the debug phase).
- 12-step linear wizard locked (Back/Next only, no per-item jump from the
  confirm page): UI language → keyboard → network (nmtui) → mirror
  (reflector `--latest 20 --sort rate --protocol https` or manual `Server =`
  line, validated by `pacman -Sy` exit code) → disk (cfdisk + btrfs `@` /
  `@home` with `compress=zstd:1`, ESP not reformatted if already vfat,
  optional extra partitions for `/home` etc.) → kernel
  (linux/linux-lts/linux-zen/linux-hardened) → nvidia (variant filtered by
  a kernel compatibility matrix) → timezone (ip-api.com GeoIP + manual
  override) → single zsh user in `wheel` with locked root → hostname →
  confirm → install.
- Bootloader locked: GRUB UEFI (`--bootloader-id=clipsneko`) with
  `grub-btrfs`; ESP at `/mnt/boot/efi`; kernel stays under `/boot`.
- mkinitcpio handling locked: **no MODULES additions** — the default
  `filesystems` HOOK + btrfs-progs cover btrfs, and current nvidia packages
  need no MODULES entries. If nvidia is installed, remove `kms` from HOOKS
  in `/etc/mkinitcpio.conf`, then `mkinitcpio -P`. No
  `/etc/modprobe.d/nvidia-drm.modprobe`.
- nvidia compatibility matrix locked:
  - linux → nvidia / nvidia-dkms / nvidia-open-dkms / nvidia-lts
  - linux-lts → nvidia-lts / nvidia-dkms / nvidia-open-dkms
  - linux-zen → nvidia-dkms / nvidia-open-dkms
  - linux-hardened → nvidia-dkms / nvidia-open-dkms
  - default recommendation: nvidia-dkms.
- Keybindings locked: Tab/Shift+Tab focus, Up/Down/j/k list nav, Space
  toggle, Enter confirm/advance, Esc back, on-screen Next/Back buttons,
  Ctrl+C quit (with confirmation), F1 help; install phase shows spinner +
  progress text with logs only to file and `L` to view the log afterwards.
- `AGENTS.md` written (English-only code/docs, `t!()` macro for all UI
  strings, no wheel reinvention, mandatory `docs/dev-prog.md` updates, no
  agent-side git writes, fmt+clippy+build verification).
- `docs/design.md` written capturing all locked decisions above.
- Cargo project initialized: `Cargo.toml` pins `ratatui 0.29`, `crossterm
  0.28`, `gettext-rs 0.7` (gettext-system feature), `anyhow 1`, `thiserror
  2`, `tracing 0.1`, `tracing-subscriber 0.3` (env-filter); release profile
  uses LTO + strip + panic=abort.
- `build.rs` compiles each `po/<lang>/LC_MESSAGES/clipsneko-installer.po`
  → `.mo` into `$OUT_DIR/locale` via `msgfmt`; exposes
  `CLIPSNEKO_DEV_LOCALEDIR` (overridable at runtime via
  `CLIPSNEKO_LOCALEDIR` for production installs).
- `src/i18n.rs`: `UiLang` enum (En/ZhCn), `set_language()` (setlocale +
  bindtextdomain + bind_textdomain_codeset + textdomain), `t!()` macro via
  `#[macro_export]`. Named `t!` not `_()` because `_` is a Rust reserved
  identifier; `AGENTS.md` §3/§6 and `docs/design.md` §8 updated to match.
- `src/main.rs`: inits tracing + i18n, then calls `app::run`.
- `src/state.rs`: full data model (`InstallerState`, `DiskState`,
  `KernelChoice`, `NvidiaChoice`, `UserInfo`), `#![allow(dead_code)]` until
  steps populate the fields.
- `src/steps/mod.rs`: `StepId` (12 variants in design order), `Step` trait,
  `StepAction` (None/Next/Back/Quit), `StubStep` placeholder, `build_steps()`
  returning all 12 slots as `Box<dyn Step>`.
- `src/app.rs`: `App` state machine + main loop; 3-row header (bold title +
  "Step X/12: <step title>"), step body, 1-row footer hint; Ctrl+C arms a
  centered quit-confirmation dialog (Y→quit, other→cancel); F1 reserved for
  help; Next/Back clamped at the ends.
- `po/` scaffolding: `clipsneko-installer.pot` + `en/LC_MESSAGES/` (identity)
  + `zh_CN/LC_MESSAGES/` (Simplified Chinese); 19 strings, all translated.
- `.gitignore` expanded (target, `*.mo`/`*.gmo`, editor swap, IDE dirs, OS
  metadata, local `.env`); `Cargo.lock` kept tracked (binary project).
- Verified green: `cargo fmt --check`, `cargo clippy -- -D warnings`,
  `cargo build`. Confirmed the regenerated `.mo` contains exactly the 19
  current msgids.

## Not done

- **Language step** (`steps/language.rs`): list En/ZhCn via `UiLang::label()`,
  call `set_language()` on change so the rest of the UI re-translates live,
  write into `state.ui_lang`. Currently `main.rs` hardcodes `UiLang::En`, so
  `UiLang::ZhCn` and `UiLang::label()` are dead code (annotated).
- **Keyboard step** (`steps/keyboard.rs`): list `localectl list-keymaps`,
  `loadkeys` immediately, persist `state.keymap`.
- **Network step** (`steps/network.rs`): suspend ratatui, run `nmtui`, verify
  connectivity (`curl -sI http://ip-api.com/json`).
- **Mirror step** (`steps/mirror.rs`): reflector run + manual `Server =`
  entry + `pacman -Sy` validation.
- **Disk step** (`steps/disk.rs`): disk select, cfdisk, partition
  auto-suggest (vfat+ESP→ESP, btrfs→root) with override, ESP no-reformat
  rule, optional extra mounts.
- **Kernel step** (`steps/kernel.rs`): list linux/linux-lts/linux-zen/
  linux-hardened, single select.
- **Nvidia step** (`steps/nvidia.rs`): "no nvidia" or one variant from the
  kernel-filtered matrix; incompatible options disabled.
- **Timezone step** (`steps/timezone.rs`): ip-api.com GeoIP default + manual
  override from `/usr/share/zoneinfo/`.
- **User step** (`steps/user.rs`): username validation (`^[a-z_][a-z0-9_-]*$`),
  GECOS, password + confirm + strength bar.
- **Hostname step** (`steps/hostname.rs`): RFC 1123 validation
  (`^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$`).
- **Confirm step** (`steps/confirm.rs`): full summary, linear Back/Next,
  final blocking "this will format disks" dialog.
- **Install stage** (`steps/install.rs` + `installer/` modules): format &
  mount, live `pacman.conf` [clipsneko] section, `pacstrap`, `genfstab`,
  `arch-chroot` config (tz, locale, hostname, root lock, useradd, sudoers,
  mirrorlist copy, target pacman.conf, mkinitcpio `kms` removal, grub
  install + mkconfig, enable NetworkManager), finalize/reboot.
- **Deferred — postinstall hook**: the "run a script as the new user inside
  chroot" step. Needs user direction on: script path, package that installs
  it, invocation (`runuser -u <user> --`? systemd user unit?), HOME/XDG env
  injection.
- **Deferred — ClipsNeko package repository**: not built yet; only the
  `repo.conf` interface shape is specified.
- **Open — log file override**: decide whether to add a `CLIPSNEKO_LOG_FILE`
  env var so the binary can run without root for dev testing, or keep
  `/var/log/clipsneko-installer.log` root-only.
- **Open — F1 help screen**: content not designed.
- **No runtime/visual test performed yet**: the binary needs a writable
  `/var/log/clipsneko-installer.log` (root) and an interactive terminal;
  only compile-time verification is done so far.
