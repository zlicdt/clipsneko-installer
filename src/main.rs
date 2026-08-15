//! Entry point: initialize i18n + tracing, then hand off to the TUI wizard.

mod app;
mod i18n;
mod installer;
mod state;
mod steps;
mod util;

use anyhow::{Context, Result};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::path::PathBuf;

use crate::i18n::{set_language, UiLang};

const REQUIRED_CONFIG_FILES: [&str; 4] = [
    "/etc/clipsneko-installer/packages.base",
    "/etc/clipsneko-installer/packages.dev",
    "/etc/clipsneko-installer/packages.hypr",
    "/etc/clipsneko-installer/packages.kde",
];

/// Byte offset of this session's first log line. The log file is opened in
/// append mode so earlier runs stay on disk for postmortems, while the in-app
/// log viewer starts reading here and only shows the current run.
static LOG_SESSION_START: std::sync::OnceLock<u64> = std::sync::OnceLock::new();

/// Offset recorded by `init_tracing` before the first line of this session
/// was written. Zero when initialization never ran.
pub(crate) fn log_session_start() -> u64 {
    LOG_SESSION_START.get().copied().unwrap_or(0)
}

/// Verify that the runtime files promised by the Live ISO are present before
/// entering the alternate screen. A broken ISO therefore exits with a normal,
/// visible error instead of failing later in the installation stage.
fn validate_runtime_config() -> Result<()> {
    for path in REQUIRED_CONFIG_FILES {
        if !std::path::Path::new(path).is_file() {
            anyhow::bail!("required runtime config is missing: {path}");
        }
    }
    Ok(())
}

/// Refuse to start unless the Live ISO was booted in UEFI mode. The installer
/// targets UEFI only; without the EFI variable interface the GRUB stage would
/// fail late, after destructive partitioning, so this invariant is enforced
/// before the TUI starts.
fn validate_boot_mode() -> Result<()> {
    if !std::path::Path::new("/sys/firmware/efi/efivars").is_dir() {
        anyhow::bail!(
            "UEFI boot mode is required; \
             reboot the Live ISO in UEFI mode (disable BIOS/CSM)"
        );
    }
    Ok(())
}

/// Resolve the log file path under the user's cache directory:
/// `$XDG_CACHE_HOME/clipsneko-installer/log`, falling back to
/// `$HOME/.cache/clipsneko-installer/log`. The path is fixed (no env-var
/// override) so the installer runs without root on any user account.
pub(crate) fn log_path() -> Result<PathBuf> {
    let cache = match std::env::var("XDG_CACHE_HOME") {
        Ok(v) if !v.is_empty() => PathBuf::from(v),
        _ => {
            let home = std::env::var("HOME").context("HOME is not set")?;
            PathBuf::from(home).join(".cache")
        }
    };
    Ok(cache.join("clipsneko-installer").join("log"))
}

fn init_tracing() -> Result<()> {
    let path = log_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating log directory {}", parent.display()))?;
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("opening log file {}", path.display()))?;
    let session_start = file.metadata().map(|meta| meta.len()).unwrap_or(0);
    let _ = LOG_SESSION_START.set(session_start);
    // The only reader is the in-app log viewer, which renders the file
    // verbatim in a ratatui paragraph — ANSI escapes would show as garbage.
    tracing_subscriber::fmt()
        .with_writer(file)
        .with_ansi(false)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init()
        .map_err(|e| anyhow::anyhow!("tracing init failed: {e}"))?;
    tracing::info!("clipsneko-installer starting (log: {})", path.display());
    Ok(())
}

/// Install a panic hook that restores the terminal so a panic does not leave
/// the user stuck in raw mode / alternate screen with no visible message.
fn install_panic_hook() {
    let original = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(std::io::stdout(), LeaveAlternateScreen);
        original(info);
    }));
}

/// Leave the alternate screen and disable raw mode. Both operations are
/// attempted even when one fails so the terminal has the best chance of
/// returning to a usable state.
fn restore_terminal() -> Result<()> {
    let raw_result = disable_raw_mode().context("disable_raw_mode failed");
    let screen_result =
        execute!(std::io::stdout(), LeaveAlternateScreen).context("LeaveAlternateScreen failed");
    raw_result?;
    screen_result?;
    Ok(())
}

fn finish_terminal_session(app_result: Result<()>, restore_result: Result<()>) -> Result<()> {
    match (app_result, restore_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
        (Err(app_error), Err(restore_error)) => Err(app_error.context(format!(
            "terminal restoration also failed: {restore_error:#}"
        ))),
    }
}

/// Minimum terminal size the wizard can render in. The tightest layout is
/// the user-account form (16 rows of body plus header and footer); several
/// hints and dialogs also assume ~78 usable columns for their longest
/// translations. The startup check refuses to launch below this size, and
/// `App::render` shows a resize notice if the window later shrinks under it.
pub(crate) const MIN_COLS: u16 = 80;
pub(crate) const MIN_ROWS: u16 = 24;

fn main() -> Result<()> {
    install_panic_hook();
    init_tracing()?;
    validate_runtime_config()?;
    validate_boot_mode()?;
    set_language(UiLang::En)?;

    let (cols, rows) = crossterm::terminal::size().context("could not read terminal size")?;
    if cols < MIN_COLS || rows < MIN_ROWS {
        anyhow::bail!("terminal too small: need at least {MIN_COLS}x{MIN_ROWS}, got {cols}x{rows}");
    }

    enable_raw_mode().context("enable_raw_mode failed")?;
    let mut stdout = std::io::stdout();
    let terminal_result = (|| -> Result<Terminal<CrosstermBackend<std::io::Stdout>>> {
        execute!(stdout, EnterAlternateScreen).context("EnterAlternateScreen failed")?;
        let backend = CrosstermBackend::new(stdout);
        Terminal::new(backend).context("Terminal::new failed")
    })();
    let mut terminal = match terminal_result {
        Ok(terminal) => terminal,
        Err(error) => return finish_terminal_session(Err(error), restore_terminal()),
    };

    finish_terminal_session(app::run(&mut terminal), restore_terminal())
}
