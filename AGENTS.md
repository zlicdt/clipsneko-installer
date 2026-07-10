# AGENTS.md — ClipsNeko Linux Installer

Conventions for AI agents (and human contributors) working on this project.

## 1. Project purpose

A lightweight TUI installer for ClipsNeko Linux (an Arch Linux derivative).
Runs on the ClipsNeko Live ISO. Targets **UEFI only, 64-bit** systems.

## 2. Tech stack (locked)

- Rust (Edition 2021)
- TUI: `ratatui` + `crossterm`
- i18n: `gettext-rs` (binds to system libgettext), `.po/.mo` workflow; `en` is the
  source/POT language, `zh_CN` the first translation.
- Subprocess: `std::process::Command` — always shell out to existing tools, never
  reimplement. Commands needing root go through `util::process::privileged_command()`
  (prepends `sudo` when euid != 0; see `design.md` §9).
- Errors: `anyhow` + `thiserror`.
- Logging: `tracing` + `tracing-subscriber` → `~/.cache/clipsneko-installer/log`
  (fixed path under `$XDG_CACHE_HOME`, falling back to `$HOME/.cache`; no env-var
  override). A panic hook restores the terminal on crash.
- Runtime config (must exist or the installer exits with a clear error):
  `/etc/clipsneko-installer/packages.list` — one package name per line.
- The Live ISO's `/etc/pacman.conf` already contains the ClipsNeko repository.
  The installer must use `pacstrap -P` so pacman configuration is copied to
  the target; it must not maintain a separate repository configuration.

## 3. Language of code and docs

- All source comments, identifiers, doc-comments, commit messages, and docs are
  **English**.
- TUI strings are **never hardcoded** — every user-facing string goes through
  the `t!()` macro (backed by gettext).
- The installer UI language (en/zh_CN) is independent of the target system's
  locale.

## 4. Don't reinvent wheels

- Prefer existing crates from the locked stack. Add a new crate only after
  justification.
- For system operations, shell out to existing tools already shipped on the ISO:
  `nmtui`, `cfdisk`, `reflector`, `pacman`, `pacstrap`, `arch-chroot`,
  `grub-install`, `grub-mkconfig`, `mkinitcpio`, `genfstab`, `lsblk`, `blkid`,
  `partprobe`, `mkfs.btrfs`, `mkfs.vfat`, `mount`, `umount`, `loadkeys`,
  `localectl`, `systemctl`, and `ip-api.com` (HTTP) for GeoIP.
- Do not write partition editors, mirror ranking, or pacman frontends yourself.

## 5. Code style

- `cargo fmt` clean and `cargo clippy -- -D warnings` green before any task is
  considered done.
- Module layout follows `docs/design.md`.
- Public functions carry `///` doc-comments in English.

## 6. i18n workflow

- Add a UI string → wrap in `t!(...)` at the call site.
- After adding/changing strings: regenerate the POT (`xgettext` or the project's
  script), then update `po/en/LC_MESSAGES/clipsneko-installer.po` (identity) and
  `po/zh_CN/LC_MESSAGES/clipsneko-installer.po` (translation).
- Never leave a UI string untranslated in `zh_CN` unless intentionally marked
  fuzzy.

## 7. Progress logging (mandatory)

`docs/dev-prog.md` tracks the **current state** of the project as two
running lists, not a chronological log:

- **Done**: completed items. Once an item lands here it stays.
- **Not done**: pending, blocked, or deferred items.

After each task, move newly finished items from "Not done" to "Done". Do
not keep dated entries or a history of past sessions — the file is a
snapshot of where the project stands right now.

## 8. Git discipline (mandatory)

- The agent MUST NOT run `git commit`, `git push`, `git tag`, `git commit --amend`,
  or any write-side git command. The user commits manually.
- The agent MAY run read-only git commands (`status`, `diff`, `log`) to inspect.
- Never stage secrets or keys.

## 9. Verification before declaring a task done

- If Rust code was touched: run `cargo fmt --check`, `cargo clippy -- -D warnings`,
  and `cargo build` (or `cargo test` when tests exist).
- If i18n strings were touched: confirm `.pot` and both `.po` files are consistent.
- Update `docs/dev-prog.md` last, in the same session.

## 10. When uncertain

- Stop and ask the user. The user is the decision-maker on any design choice not
  pre-approved in `docs/design.md`.
- Do not silently invent behavior for partitioning, nvidia, chroot, or the deferred
  "postinstall script" hook — those are all waiting on explicit user direction.
