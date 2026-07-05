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

## 2026-07-05 — Project scaffolding

### Done

- Initialized Cargo project with the locked stack:
  `Cargo.toml` pins `ratatui 0.29`, `crossterm 0.28`, `gettext-rs 0.7` (with the
  `gettext-system` feature so it links the system gettext instead of vendoring
  one — vendored gettext 0.26 fails to build against current glibc headers),
  `anyhow 1`, `thiserror 2`, `tracing 0.1`, `tracing-subscriber 0.3` (with the
  `env-filter` feature). Release profile uses LTO + strip + panic=abort.
- Wrote `build.rs` that runs `msgfmt` to compile each `po/<lang>/LC_MESSAGES/
  clipsneko-installer.po` into `$OUT_DIR/locale/<lang>/LC_MESSAGES/
  clipsneko-installer.mo`; passes `$OUT_DIR/locale` to the binary via the
  `CLIPSNEKO_DEV_LOCALEDIR` compile-time env var, overridable at runtime via
  `CLIPSNEKO_LOCALEDIR` for production installs.
- Wrote `src/i18n.rs` with `UiLang` enum (en/zh_CN), `set_language()` (setlocale
  + bindtextdomain + bind_textdomain_codeset + textdomain), and the `t!()`
  macro exported via `#[macro_export]`. The macro replaces the originally
  drafted `_()` name — `_` is a Rust reserved identifier and cannot name a
  macro. `AGENTS.md` §3/§6 and `docs/design.md` §8 were updated to reference
  `t!()` instead of `_(...)`.
- Wrote `src/main.rs`: inits tracing to `/var/log/clipsneko-installer.log`
  with `RUST_LOG` override, sets UI language to English, then runs a minimal
  ratatui loop that renders the translated "Welcome to ClipsNeko Linux
  Installer" + "Press q to quit" lines and exits on q/Q/Esc. This proves the
  full i18n + TUI stack is wired end-to-end.
- Set up `po/` scaffolding: `clipsneko-installer.pot` (the source template),
  `po/en/LC_MESSAGES/clipsneko-installer.po` (identity), and
  `po/zh_CN/LC_MESSAGES/clipsneko-installer.po` (Simplified Chinese). Both
  example strings from `src/main.rs` are present in all three files so the
  translation pipeline is exercised.
- Added `.gitignore` covering `/target` and compiled `*.mo` files.
- Verified green: `cargo fmt --check`, `cargo clippy -- -D warnings`,
  `cargo build`. Confirmed the `.mo` files are emitted under
  `target/debug/build/clipsneko-installer-*/out/locale/`.

### Not done

- The minimal `main.rs` only renders a placeholder screen; no real wizard
  shell (`app.rs` / `state.rs`) or step modules exist yet.
- `set_language(UiLang::En)` is hardcoded; the language picker step is not
  implemented, so `UiLang::ZhCn` and `UiLang::label()` are currently
  dead code (marked `#[allow(dead_code)]` until the picker lands).
- No runtime fallback for the log file path — the binary requires
  `/var/log/clipsneko-installer.log` to be writable, which means running as
  root even for early dev testing. May want a `CLIPSNEKO_LOG_FILE` env var
  override later; not added now to avoid inventing behavior.
- No README at the project root (not required by AGENTS.md, just noted).

### Next

- Implement the linear wizard shell: `state.rs` (the `InstallerState` struct
  holding every step's choices), `app.rs` (the step state machine + main
  render loop), and stub `steps/*.rs` modules for all 12 steps. Goal:
  Tab/Shift+Tab/Esc/Enter navigation works end-to-end with placeholders.
- Wire the `$LANG`/`LC_ALL` requirement into `set_language` so it surfaces a
  clean error when the requested locale is not generated on the host (the
  code already returns an error, but the wizard shell will need to present
  it rather than crashing).
- Decide with the user whether to add a `CLIPSNEKO_LOG_FILE` override for
  non-root dev testing, or keep root-only.

