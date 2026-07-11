# Development Progress

This file tracks the **current state** of the project, not a chronological
log. Items live in exactly one of two lists; when a piece of work is finished
it moves from "Not done" to "Done" and stays there.

## Done

- Project scope is locked: a lightweight ClipsNeko Linux Live ISO installer
  for UEFI-only, 64-bit systems. It runs as root or as the passwordless
  `installer` user and delegates system work to existing ISO tools.
- The core stack is in place: Rust 2021, ratatui/crossterm, gettext-rs,
  anyhow/thiserror, tracing/tracing-subscriber, serde/serde_json, and libc.
- The 12-step linear flow and install-stage outline are documented in
  `docs/design.md`: UI language and target locale, keyboard, network, mirrors,
  disk, kernel, NVIDIA, timezone, user, hostname, confirmation, and installation.
- Cargo scaffolding, the release profile, `.gitignore`, CI, and build-time
  gettext compilation are present. CI checks formatting, Clippy with warnings
  denied, tests, translation consistency, and a release build.
- i18n uses stable dot-separated IDs and a literal-only `t!()` macro. The POT
  and en/zh_CN catalogs contain the same 122 message IDs with no untranslated,
  fuzzy, or obsolete entries. zh_TW support was removed because it had no
  catalog and is outside the locked language set.
- UI language changes only `LC_MESSAGES` and remains independent of the target
  system locale. Debug builds load generated catalogs from OUT_DIR; release
  builds use the GNU-standard `/usr/share/locale` path without a runtime
  override. Missing locales or catalogs are fatal Live ISO invariant failures.
- `config/packages.list` contains the authoritative static package set. Startup
  requires only `/etc/clipsneko-installer/packages.list`; there is no separate
  repository config. The Live ISO's existing `pacman.conf` supplies the
  ClipsNeko repository, and the install design requires `pacstrap -P` to copy
  `pacman.conf` and `pacman.d` to the target.
- Logging writes to `$XDG_CACHE_HOME/clipsneko-installer/log`, falling back to
  `$HOME/.cache/clipsneko-installer/log`, with no path override. Missing HOME,
  log setup failures, and required runtime-file failures exit clearly before
  entering the TUI.
- The fatal/recoverable error boundary is consistent with the controlled Live
  ISO: missing commands/config/catalogs, malformed fixed command output,
  spawn/sudo failures, and privileged write failures propagate as fatal;
  offline state, invalid/unreachable mirror input, cancellation, confirmation,
  and non-zero partprobe device-state failures stay recoverable in the TUI.
- Terminal setup, shutdown, panic, and full-screen subprocess paths all attempt
  restoration. Restoration failures are fatal, and setup failures after raw
  mode begins restore the terminal before returning an error.
- `util::process::privileged_command()` runs commands directly as root and
  otherwise prefixes `sudo --`. Full-screen `nmtui` and `cfdisk` suspension is
  shared, and subprocess spawn failures do not leave the app running with an
  unknown terminal state.
- The app shell renders the header, body, and footer; owns the 12-step state
  machine; skips disabled buttons; and provides shared step activation,
  completion, modal, subprocess, and commit hooks.
- Body Enter and footer Back/Next use the same `StepAction` paths, so button
  navigation cannot bypass step validation or internal disk pages.
- Navigation is locked and implemented: modal Esc cancels; normal Esc follows
  Back; Ctrl+C opens quit confirmation from every page or step modal. The quit
  dialog has Cancel/Quit buttons and defaults to Cancel.
- Step-owned dialogs receive keys before global navigation. App tests cover
  modal Esc/Tab precedence, Back behavior, Next commit ordering, default
  cancellation, and explicit quitting.
- Body widgets know whether the footer owns focus, preventing a list cursor and
  footer button from both appearing focused. Shared dialog centering, wrapping,
  minimum-width layout, and horizontal mirror-input scrolling are implemented.
- **Language/locale step:** independent UI-language and target-locale lists are
  implemented. UI switching is live; target locales come from commented or
  enabled UTF-8 entries in `/etc/locale.gen`; `en_US.UTF-8` is the required
  default; both choices persist independently. Tab/Shift+Tab and Enter/footer
  commit paths are covered.
- **Keyboard step:** keymaps and the active VC keymap come from `localectl`;
  Space/Enter/footer Next apply with `loadkeys`; missing or malformed invariant
  data is fatal; shared state and re-entry synchronization are implemented.
- **Network step:** entry/retry connectivity checks, nmtui suspension/resume,
  connected/offline completion gating, and translated interface/address/gateway
  display are implemented. Command spawn/output invariants are fatal while a
  failed connectivity probe remains a normal offline state.
- **Mirror step:** region parsing/reordering, manual URL normalization, list and
  input focus, privileged replacement, captured `pacman -Sy` validation, error
  dialog, and state persistence are implemented. Manual mode activates only the
  manual server, preventing fallback mirrors from hiding invalid input.
- **Disk device model:** fixed-column typed lsblk JSON parsing, tree flattening,
  size formatting, GPT ESP validation, zram exclusion, Live-media detection,
  and the strict `> 20 GiB` capacity threshold are implemented.
- **Disk picker:** a responsive device/model/transport/size/status table is in
  place. The Live ISO disk and read-only disks remain visible but disabled;
  other removable disks are usable. Returning from cfdisk clears all roles,
  runs partprobe, and refreshes lsblk. A non-zero partprobe result is retryable
  and discards the stale pre-cfdisk partition snapshot.
- **Partition roles:** a responsive device/size/filesystem/label/role table and
  explicit ESP/Target/Unassigned dialog are implemented. Protected Live/read-
  only partitions cannot be assigned, roles are mutually exclusive, ESP
  requires the GPT ESP type, and refreshes reconcile stale assignments.
- **Multi-target btrfs:** RAID0/RAID1 selection and conservative usable-capacity
  checks are implemented. Every Target appears in the destructive warning;
  only an already-vfat ESP is reused without a format warning.
- **Kernel step:** the four supported kernels are available in a translated
  single-select list, with `linux-zen` selected by default. Space, Enter, and
  footer Next commit consistently; returning to the step restores the saved
  choice. Every kernel maps to its matching headers package, which M4b will
  always include in the dynamic pacstrap package set.
- **NVIDIA step:** no-driver, `nvidia-open`, `nvidia-open-lts`, and
  `nvidia-open-dkms` choices are implemented with the documented per-kernel
  compatibility matrix. The default is `nvidia-open-dkms`; incompatible
  choices remain visible with a dimmed translated suffix and are skipped by
  navigation. Returning after a kernel change automatically resets an
  incompatible saved choice to the DKMS default. NVIDIA contributes only its
  selected package because kernel headers are derived unconditionally by the
  kernel choice.
- **Timezone step:** GeoIP supplies the initial choice with a UTC fallback;
  saved choices are restored without another request. A two-panel picker uses
  `timedatectl list-timezones` for the ten supported geographic regions and
  their concrete zones, filters legacy top-level and `Etc` aliases, and offers
  UTC directly with the second panel visibly disabled. Arrow, Enter,
  Tab/Shift+Tab, and footer commit paths are implemented and tested.
- **User-account step:** a centered, bordered form collects only the username,
  password, and confirmation; there is no GECOS field. Username validation is
  live, empty passwords and mismatches block Next, and the four-level strength
  bar remains advisory so every matching non-empty weak password is accepted.
  Enter and Tab/Shift+Tab follow the field/footer focus chain, saved values are
  restored on re-entry, and password rendering is always masked.
- Password storage is implemented as a non-Debug `SecretString` backed by
  `zeroize`. Both editable password buffers and the confirmed state secret are
  zeroized on clear or Drop; M4b will feed `<username>:<password>` only through
  `chpasswd` stdin and clear the confirmed secret immediately after success.
  Passwords never enter command arguments, summaries, tracing fields, or logs.
- The account/install design no longer includes a GECOS value or
  `passwd -l root`; the installer creates the wheel user and leaves root-account
  policy unchanged.
- Current automated verification is green: `cargo fmt --check`,
  `cargo clippy --all-targets -- -D warnings`, `cargo test` (113 tests),
  `cargo build`, `cargo build --release`, `msgfmt --check`, and POT/PO `msgcmp`.

## Not done

- **M1 runtime acceptance:** the two languages and target-locale list, keyboard
  changes, nmtui return path, mirror rewrite, and release catalogs still need an
  interactive check on the actual ClipsNeko Live ISO or a matching VM. The
  packaging must install `config/packages.list` and both MO catalogs at their
  documented system paths.
- **Full-screen restoration test seam:** the helper's actual terminal-state
  bookkeeping on subprocess spawn failure has not been automated; current
  coverage tests privilege command construction only.
- **M2 runtime acceptance:** cfdisk suspension, partprobe recovery, real lsblk
  output, protected Live media, responsive tables in both languages, role
  assignment, RAID profiles, and wipe dialogs still need an interactive
  multi-disk Live ISO/VM check.
- **M3 selection and identity:** the kernel, NVIDIA, timezone, and user-account
  steps still need an interactive bilingual Live ISO/VM check, including real
  GeoIP, `timedatectl` data, centered-form layout, input focus, masking, and
  strength colors. Hostname and confirmation remain stubs; their validation
  and final confirmation UI are not yet code.
- **M4a install stage:** btrfs format/RAID/subvolume and ESP format/mount logic
  is not implemented.
- **M4b install stage:** packages.list loading, dynamic package derivation,
  `pacstrap -P`, fstab, target locale/vconsole/hostname/user configuration,
  stdin-only chpasswd, sudoers, and mkinitcpio are not implemented.
- **M4c install stage:** GRUB installation/configuration, NetworkManager
  enablement, unmount, reboot, and final shell behavior are not implemented.
- **F1 help:** content and rendering still need the user's decision; F1 is not
  advertised in the UI meanwhile.
- **Postinstall hook:** blocked on the user's decisions about its path/package,
  invocation method, and HOME/XDG environment injection.
- **End-to-end acceptance:** no complete VM/Live ISO installation has yet
  produced and booted a target ClipsNeko system.
