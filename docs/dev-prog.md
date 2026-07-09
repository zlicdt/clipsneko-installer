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
  (parse `/etc/pacman.d/mirrorlist` regions, reorder on selection, or
  manual `Server =` line, validated by `pacman -Sy` exit code) → disk (cfdisk + btrfs `@` /
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
  + `zh_CN/LC_MESSAGES/` (Simplified Chinese); 24 strings, all translated.
- `.gitignore` expanded (target, `*.mo`/`*.gmo`, editor swap, IDE dirs, OS
  metadata, local `.env`); `Cargo.lock` kept tracked (binary project).
- Verified green: `cargo fmt --check`, `cargo clippy -- -D warnings`,
  `cargo build`. Confirmed the regenerated `.mo` contains exactly the 24
  current msgids.
- `docs/dev-plan.md` written: M0-M5 milestone roadmap (M4 split into
  M4a/M4b/M4c). Definition of Done per milestone = fmt + clippy + build +
  `cargo test` (unit tests for pure-logic modules) + manual run test + po
  sync. Packaging is out of scope (user handles elsewhere); sample
  `/etc/clipsneko-installer/` config files (`config/packages.list`,
  `config/repo.conf`) are a deliverable in M1.
- **Test layout**: per-module unit tests live in sibling files
  (`src/util/process/tests.rs`, `src/steps/language/tests.rs`) reached from
  the parent module via a one-line `#[cfg(test)] mod tests;` declaration
  (Rust 2018 `foo.rs` + `foo/` coexistence, no `mod.rs`). Tests keep
  `use super::*` so private items stay testable. The inline
  `#[cfg(test)] mod tests { ... }` blocks in `util/process.rs` and
  `steps/language.rs` were removed in favor of this layout. Convention
  applies to all future modules with tests.
- **i18n msgid convention**: `t!()` keys are namespaced dot-ids (lowercase
  English with underscores), not English UI text — e.g. `app.title`,
  `app.step_indicator`, `button.back`, `button.next`, `button.quit`,
  `footer.hint`, `quit_dialog.title`, `quit_dialog.hint`,
  `language_step.title`, `language_step.hint`, `step.title.<step>`,
  `stub.body`, `stub.hint`. `.pot` and `en`/`zh_CN` `.po` rewritten to match:
  the template carries the dot-id msgid with empty msgstr; `en` stores the
  English UI text as msgstr (identity translation, since gettext returns the
  msgid when msgstr is empty — and the msgid is no longer the display text);
  `zh_CN` stores the Simplified Chinese translation. Multi-symbol hint lines
  (`footer.hint`, `quit_dialog.hint`, `language_step.hint`) are kept as one
  msgid per line rather than being split per key. `ZhTw` variant added to
  `UiLang` (`zh_TW.UTF-8`, label "繁體中文"); the picker is 3 entries now.
- **Minimum terminal size guard** (`main.rs`): before entering raw mode /
  alternate screen, `crossterm::terminal::size()` is checked against
  `MIN_COLS=60` × `MIN_ROWS=16`. Below that the installer bails with a
  plain `Error: terminal too small: need at least 60x16, got <c>x<r>`
  (no alt screen, no raw mode, so the message reaches the user's normal
  terminal). Driven by the quit-dialog regression: the dialog was sized
  via `centered_rect(50, 7, ...)` with the second argument treated as a
  percentage of terminal height — at 30 rows that is ~2 rows of content
  (only the empty border visible on Windows Terminal), at 40 rows ~3 rows
  (only the title visible on kitty). Now the dialog requests a fixed 8
  rows (6 content + 2 borders) and starting below 16 rows is refused.
- **Quit dialog layout fix** (`app.rs`): `centered_rect(width_pct,
  height_rows, area)` now takes a fixed row count for height (clamped to
  `area.height`); the previous percent-based height produced the
  Windows-Window-empty / kitty-title-only symptoms above. The dialog
  Paragraph dropped `.wrap(Wrap { trim: true })` — the fixed 8-row box
  already fits all 6 content lines, and `Wrap` + `Alignment::Center`
  could fragment lines on narrow terminals. `Wrap` import removed.
- **Language step** (`steps/language.rs`): `LanguageStep` with a stateful
  `List` of `UiLang::En` / `UiLang::ZhCn` / `UiLang::ZhTw` (labels via
  `UiLang::label()`); Up/Down/j/k moves the highlight cursor (rendered by
  the `REVERSED` `highlight_style`), Space selects and calls
  `set_language()` live so the whole UI re-translates immediately, Enter
  selects and advances; writes `state.ui_lang`; `sync_from_state()`
  restores the pick when re-entered via Back. The `▶` selection marker is
  embedded in each `ListItem` text (prefix `▶` for the active language,
  space otherwise) so it follows `self.selected` instead of the cursor —
  ratatui's `highlight_symbol` was dropped because it is bound to
  `ListState::selected()` (the cursor) and cannot mark a separate
  "currently applied" row. Selection failure falls back to English with a
  `tracing::warn!` (defensive only — the ISO build generates both
  `en_US.UTF-8` and `zh_CN.UTF-8`). Step body shows a `Space=Select
  Enter=Next` hint line below the list. Esc is no longer handled by the
  step — `app.rs` intercepts it as a global quit. `i18n.rs` dead-code
  allows on `UiLang` / `label()` removed; `set_language()` doc-comment
  records the ISO locale-build assumption.
- **Bottom Back/Next buttons + focus model** (`app.rs`): footer renders
  `[ Back ]` (left) and `[ Next ]` (right) with a center hint
  `Tab=Focus  F1=Help  Esc=Quit`; `Focus` enum (StepBody / BackButton /
  NextButton) with Tab/Shift+Tab cycling (skipping disabled buttons);
  button-focused Enter activates the button; step-body-focused Enter still
  advances via `StepAction::Next`. Back disabled on the first step, Next
  on the last. `Step::render` changed to `&mut self` so stateful widgets
  (e.g. `ListState`) are managed in place — the previous `&self` +
  clone-state approach lost ratatui's offset bookkeeping and could wedge
  the list after a few Up/Down presses.
- **Quit flow** (`app.rs`): Esc and Ctrl+C both open the quit-confirmation
  dialog (Esc is no longer "back"); the dialog shows a `[ Quit ]` button
  and `Esc to cancel, Enter to quit.` hint — Enter exits, Esc cancels,
  `Y` removed. `StubStep` no longer handles Esc.
- **Privilege model + logging** (`main.rs`, `util/process.rs`): installer
  runs as a normal user; log goes to `$XDG_CACHE_HOME/clipsneko-installer/log`
  (fallback `$HOME/.cache/...`), fixed path, no env-var override. A panic
  hook restores the terminal on crash. `util::process::privileged_command()`
  prepends `sudo` when euid != 0 (both `root` and the `installer` user are
  passwordless for sudo on the ISO); `is_root()` via `libc::geteuid()`.
  `libc 0.2` added to `Cargo.toml` (already an indirect dep via crossterm).
  `design.md` §1 / §6 / §9 and `AGENTS.md` §2 updated. Resolves the
  "Open — log file override" item (decided: no env-var override).
- **Keyboard step** (`steps/keyboard.rs`): `KeyboardStep` loads the keymap
  list from `localectl list-keymaps` and detects the live keymap from
  `localectl status` (the `VC KEYMAP:` line) at construction time, so the
  picker opens with the live keymap highlighted and marked (▶). The list
  styling mirrors the language step exactly — `List` + `REVERSED`
  `highlight_style` + per-row `▶`/space marker bound to `self.selected`
  (the applied keymap, not the cursor) + `Block` title + a 1-row centered
  DIM hint below. Up/Down/j/k moves the highlight (wrapping); Space runs
  `loadkeys` on the highlighted keymap and updates `state.keymap` /
  `self.selected` (▶ moves, no advance); Enter does the same and advances.
  `sync_from_state()` restores the pick when re-entered via Back. `localectl`
  and `loadkeys` go through `util::process::privileged_command()` per
  `design.md` §9; the `#[allow(dead_code)]` on `privileged_command` was
  removed now that it has a live caller. Per the step design, command
  failures are not defended against: `load_keymap_list()` /
  `current_keymap()` `expect` at startup (panic hook restores the
  terminal), and `apply()` logs a `tracing::warn!` on non-zero exit /
  spawn failure but does not block navigation. `state.keymap` is later
  written to the target's `/etc/vconsole.conf` in the install stage (§5).
  Pure parsers `parse_keymap_list()` (trim + drop empty lines) and
  `parse_current_keymap()` (extract `VC KEYMAP:` value, treat `n/a`/empty
  as unset) are unit-tested in `steps/keyboard/tests.rs` (10 cases).
  i18n: `keyboard_step.title` / `keyboard_step.hint` added to `.pot`, `en`
  (identity), and `zh_CN`; the hint text matches `language_step.hint` for
  visual consistency. Verified green: `cargo fmt --check`, `cargo clippy
  -- -D warnings`, `cargo build`, `cargo test` (15 tests); `.mo` confirmed
  to contain 26 msgids including both keyboard strings.
- **Network step** (`steps/network.rs`): `NetworkStep` checks connectivity
  on entry via `Step::activate()` — runs `curl --max-time 5 -sI
  http://ip-api.com/json` (exit 0 = connected) and sets `state.network_ok`.
  The body shows `✓ Connected` + network details (interface / address /
  gateway from `hostname -I` and `ip route show default`) or `✗ Not
  connected`. `Step::is_complete()` returns `state.network_ok`, so
  `app.rs`'s `next_enabled()` disables the Next button (dimmed, skipped in
  Tab focus cycling) until connectivity is verified — exactly the "grey =
  not clickable, bright = clickable" UX requested. Enter is
  context-sensitive: connected → `StepAction::Next`; disconnected →
  `StepAction::SuspendRun("nmtui", [])`. `N` always launches nmtui;
  `R` re-checks connectivity. `nmtui` runs via `util::process::run_fullscreen`
  — a new helper that leaves the alt screen + disables raw mode, runs the
  subprocess, then resumes ratatui; `app.rs` calls `terminal.clear()`
  afterwards and routes the exit status to `Step::on_subprocess_done()`,
  which triggers a re-check. The `Step` trait gained three default-method
  extension points: `activate(&mut self, &mut InstallerState)` (entry hook),
  `is_complete(&self, &InstallerState) -> bool` (Next-gate, default `true`),
  `on_subprocess_done(&mut self, ExitStatus, &mut InstallerState)` (post-
  subprocess hook). `StepAction` gained `SuspendRun(String, Vec<String>)`;
  `app.rs`'s `Action` gained `RunSubprocess(String, Vec<String>)`.
  `activate_current()` is called on initial entry and on every Back/Next
  navigation. `nmtui`, `curl`, `hostname`, and `ip` do not need root
  (`nmtui` uses polkit; the rest are plain user commands per `design.md`
  §9) so they use `Command::new` directly, not `privileged_command`. Pure
  parsers `parse_hostname_i()` (whitespace-split IPs) and
  `parse_default_route()` (extract `via`/`dev` from first line) are
  unit-tested in `steps/network/tests.rs` (9 cases). i18n: 8 new strings
  (`network_step.title`, `.status_connected`, `.status_disconnected`,
  `.label_interface`, `.label_address`, `.label_gateway`,
  `.hint_connected`, `.hint_disconnected`) added to `.pot`, `en`, `zh_CN`.
  Verified green: `cargo fmt --check`, `cargo clippy -- -D warnings`,
  `cargo build`, `cargo test` (25 tests); `.mo` confirmed to contain 34
  msgids.
- **Mirror step** (`steps/mirror.rs`): `MirrorStep` parses
  `/etc/pacman.d/mirrorlist` at construction time into region blocks
  (`## <Region>` header + `Server =` lines). The body shows a single-select
  region list (Up/Down/j/k, `REVERSED` highlight, `▶` applied marker —
  same list styling as the language/keyboard steps) and, above it, a
  one-line input field for a manual `Server =` URL. Tab toggles focus
  between the list and the input field. On Next: if the input field is
  non-empty, `normalize_server_line()` validates the URL scheme
  (http/https/ftp/rsync) and it becomes the sole mirror (prepended to the
  file); otherwise the selected region's `Server =` lines are moved to the
  top of the mirrorlist via `reorder_mirrorlist()` (file header comments
  preserved, other regions keep relative order). The rewritten file is
  written back via `privileged_command("cp")` from a temp file, then
  `pacman -Sy` validates (`.output()` so output never reaches the
  terminal). Exit 0 → `state.mirror_lines` recorded, advance. Non-zero →
  modal error dialog (Esc/Enter dismisses, retry). Invalid manual URL →
  modal error. `Step::is_complete()` returns `self.validated`, so Next is
  disabled until a selection passes validation. `reflector` was dropped
  from the design entirely (per user direction): `design.md` §4 step 4,
  §5, §9, `dev-plan.md` M1 deliverables/acceptance all updated. Pure
  parsers `parse_mirrorlist_regions()`, `reorder_mirrorlist()`,
  `extract_region_servers()`, `split_header()`, `split_blocks()`, and
  `normalize_server_line()` are unit-tested in `steps/mirror/tests.rs`
  (16 cases). i18n: 11 new strings (`mirror_step.input_title`,
  `.input_label`, `.list_title`, `.hint_list`, `.hint_input`,
  `.error_title`, `.error_hint`, `.error_pacman`, `.error_write`,
  `.error_invalid_url`) added to `.pot`, `en`, `zh_CN`. Verified green:
  `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo build`,
  `cargo test` (41 tests); `.mo` confirmed to contain 44 msgids.

## Not done

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
- **Open — F1 help screen**: content not designed.
- **No runtime/visual test performed yet**: the binary now runs as a normal
  user (log under `~/.cache/`), but an interactive terminal is still needed;
  only compile-time verification + `cargo test` (util::process) is done so
  far. TestBackend-based render tests are planned.
