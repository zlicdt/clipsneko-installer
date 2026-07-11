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
- Arch release packaging is present under `package/`. The PKGBUILD builds and
  tests the locked Rust sources, installs the binary, backed-up runtime package
  list, GPL license, and all seven GNU-standard MO catalogs. Numeric `vX.Y.Z`
  tags trigger an Arch `base-devel` container, verify that the tag, Cargo, and
  PKGBUILD versions agree, set the requested makepkg packager identity, build
  the package as an unprivileged user, preserve it as an Actions artifact, and
  upload it to the corresponding generated GitHub Release.
- The English project README presents the logo and live CI status, summarizes
  the installer and its supported environment, documents Arch/ClipsNeko and
  Ubuntu build prerequisites, and provides verified release-build and
  development-check commands with a destructive-installation warning.
- i18n uses stable dot-separated IDs and a literal-only `t!()` macro. English,
  Simplified Chinese, Traditional Chinese, Japanese, German, Korean, and
  Russian are available in the language picker. The POT and all seven catalogs
  contain the same 168 message IDs with no untranslated, fuzzy, or obsolete
  entries. CI and build-time MO generation cover every supported catalog.
- UI language changes only the process's `LC_MESSAGES`; applying it also adds
  the matching target locale to the generation set without replacing the
  target's default `LANG`. Debug builds load generated catalogs from OUT_DIR;
  release builds use the GNU-standard `/usr/share/locale` path without a
  runtime override. Missing locales or catalogs are fatal Live ISO invariant
  failures.
- `config/packages.list` contains the authoritative static package set. Startup
  requires only `/etc/clipsneko-installer/packages.list`; there is no separate
  repository config. The Live ISO's existing `pacman.conf` supplies the
  ClipsNeko repository, and the install design requires `pacstrap -P` to copy
  `pacman.conf` and `pacman.d` to the target. The static `base-devel` entry
  supplies `sudo`; the installer does not add it dynamically.
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
  Every focused bordered body control now shares a bold white border and title,
  including lists, tables, text inputs, and the actionable network panel. The
  style clears when a footer button owns focus; footer focus, list highlights,
  non-interactive containers, and semantic colors such as password strength
  retain their existing independent styles. Every visible border, including
  modal dialogs and informational containers, is created through the shared
  rounded-corner block constructor.
- **Language/locale step:** coordinated UI-language and target-locale lists are
  implemented. UI switching is live and automatically selects the matching
  target locale. The target list contains every commented or enabled UTF-8
  entry in `/etc/locale.gen`, supports multiple bold-white selections, starts
  with `en_US.UTF-8`, and prevents removing the final selection. Space toggles
  selection; L enables the highlighted locale when needed and records it as the
  default `LANG`, while Enter advances without changing locale choices.
  Removing the default while another locale remains transfers the default to
  the next selected row. Automatically added locales may be removed under the
  same rule. Re-entry, Tab/Shift+Tab, L, Enter, and footer commit paths are
  covered.
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
- **Partition roles:** a responsive device/size/label/role/filesystem table and
  explicit ESP/Target/Unassigned dialog are implemented. Role follows Label,
  uses the width of the longest current translation, and leaves the trailing
  filesystem column to absorb narrow-screen truncation. Protected Live/read-
  only partitions cannot be assigned, roles are mutually exclusive, ESP
  requires the GPT ESP type, and refreshes reconcile stale assignments.
- **Multi-target btrfs:** RAID0/RAID1 selection and conservative usable-capacity
  checks are implemented. Every Target appears in the destructive warning;
  only an already-vfat ESP is reused without a format warning.
- **Kernel step:** the four supported kernels are available in a translated
  single-select list, with `linux-zen` selected by default. Space, Enter, and
  footer Next commit consistently; returning to the step restores the saved
  choice. Every kernel maps to its matching headers package, which installation
  always includes in the dynamic pacstrap package set.
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
- **Hostname step:** a centered, bordered input accepts one ASCII DNS label of
  1–63 characters. Live validation accepts and preserves uppercase letters but
  rejects FQDNs, unsupported characters, and leading/trailing hyphens; Enter
  and footer Next share the same commit path, and re-entry restores the saved
  value. The install design writes it to `/etc/hostname` and adds a
  `127.0.1.1` mapping in `/etc/hosts`.
- **Confirmation step:** a scrollable installation-summary paragraph lists the
  default `LANG`, every locale enabled in `locale.gen`, keymap, kernel, NVIDIA,
  hostname, timezone, username, every affected disk, the ESP, every Target
  partition, and the btrfs RAID profile when more than one Target exists. The
  account password is deliberately excluded from the summary; missing fields
  render as the shared "not available" value. Enter
  and footer Next open a centered, Cancel-first destructive-action dialog only
  when the summary is complete (all required state present and RAID mode set
  for multi-target); an incomplete summary cannot open it. Tab/Left/Right
  toggle dialog focus, Enter on Install advances with `StepAction::Next`, and
  Enter on Cancel or Esc dismisses the dialog. The modal locks global
  navigation while open, and Up/Down/j/k, PageUp/PageDown, and Home/End scroll
  long summaries.
- Password storage is implemented as a non-Debug `SecretString` backed by
  `zeroize`. Both editable password buffers and the confirmed state secret are
  zeroized on clear or Drop; installation feeds `<username>:<password>` only
  through `chpasswd` stdin and clears both the temporary credential buffer and
  confirmed secret immediately after success.
  Passwords never enter command arguments, summaries, tracing fields, or logs.
- The account/install design no longer includes a GECOS value or
  `passwd -l root`; the installer creates the wheel user and leaves root-account
  policy unchanged.
- **M4a format and mount:** the final disk snapshot now records whether the ESP
  requires formatting. Installation formats a single Target as btrfs or all
  multi-Target devices with the selected RAID0/RAID1 data profile and RAID1
  metadata, creates `@` and `@home`, and mounts both with implicit-level
  `compress=zstd`. Existing-vfat ESPs are reused; other ESPs are formatted
  FAT32 and mounted at `/mnt/boot/efi`.
- **M4b packages and target configuration:** the installer reads the runtime
  packages list, appends deduplicated kernel/headers/linux-firmware/NVIDIA
  choices, runs `pacstrap -P`, validates both generated btrfs fstab entries
  while preserving a kernel-normalized zstd level, and appends fstab without a
  shell. Chroot configuration makes the selected locales the exact enabled
  UTF-8 set in `/etc/locale.gen`, generates them, writes the separately selected
  default `LANG`, and applies timezone, hardware clock, vconsole,
  hostname/hosts, wheel user and stdin-only password, sudoers, and NVIDIA-
  specific `kms` HOOK removal before `mkinitcpio -P`.
- **M4c boot and finalization:** GRUB UEFI installation and configuration plus
  NetworkManager enablement are implemented. The install pipeline runs in a
  background worker with a responsive spinner; Back, Esc, Ctrl+C, and the
  normal footer are locked. Failure stops without rollback and offers Return
  or a scrollable log view. Success defaults to Reboot; reboot runs privileged
  recursive unmount then reboot, while Not now exits with `/mnt` preserved.
- Destructive system work is isolated behind a command-runner seam. Automated
  tests cover command and package construction, ESP decisions, btrfs RAID and
  mount options, fstab validation, target-file transforms, stdin-only password
  handoff/clearing, navigation locking, failure/log behavior, and reboot focus
  without executing any real format, mount, pacstrap, chroot, or reboot command.
- Current automated verification is green: `cargo fmt --check`,
  `cargo clippy --all-targets -- -D warnings`, `cargo test` (151 tests),
  `cargo build`, `cargo build --release`, `msgfmt --check`, and POT/PO `msgcmp`.

## Not done

- **M1 runtime acceptance:** all seven UI languages, automatic matching-locale
  selection, target-locale multi-selection/default handling, keyboard changes,
  nmtui return path, mirror rewrite, and release catalogs still need an
  interactive check on the actual ClipsNeko Live ISO or a matching VM. The ISO
  must generate all seven documented UI locales, and the packaged
  `config/packages.list` plus every MO catalog still need validation at their
  documented system paths after installation.
- **Full-screen restoration test seam:** the helper's actual terminal-state
  bookkeeping on subprocess spawn failure has not been automated; current
  coverage tests privilege command construction only.
- **M2 runtime acceptance:** cfdisk suspension, partprobe recovery, real lsblk
  output, protected Live media, responsive tables in all supported languages, role
  assignment, RAID profiles, and wipe dialogs still need an interactive
  multi-disk Live ISO/VM check.
- **M3 selection and identity:** the kernel, NVIDIA, timezone, user-account,
  hostname, and confirmation steps still need an interactive multilingual Live
  ISO/VM check, including real GeoIP, `timedatectl` data, centered-form
  layouts, input focus, masking, strength colors, hostname validation, and the
  final summary/destructive-dialog review before install.
- **M4 runtime acceptance:** the full destructive pipeline, spinner/navigation
  lock, failure/log dialog, generated fstab, target configuration, GRUB output,
  preserved-mount Not now path, and unmount/reboot path still need an
  interactive multi-disk test on the actual ClipsNeko Live ISO or a disposable
  matching VM.
- **F1 help:** content and rendering still need the user's decision; F1 is not
  advertised in the UI meanwhile.
- **Postinstall hook:** blocked on the user's decisions about its path/package,
  invocation method, and HOME/XDG environment injection.
- **End-to-end acceptance:** no complete VM/Live ISO installation has yet
  produced and booted a target ClipsNeko system.
