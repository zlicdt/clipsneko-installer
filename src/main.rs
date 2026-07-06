//! Entry point: initialize i18n + tracing, then hand off to the TUI wizard.

mod app;
mod i18n;
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

/// Resolve the log file path under the user's cache directory:
/// `$XDG_CACHE_HOME/clipsneko-installer/log`, falling back to
/// `$HOME/.cache/clipsneko-installer/log`. The path is fixed (no env-var
/// override) so the installer runs without root on any user account.
fn log_path() -> PathBuf {
    let cache = match std::env::var("XDG_CACHE_HOME") {
        Ok(v) if !v.is_empty() => PathBuf::from(v),
        _ => {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            PathBuf::from(home).join(".cache")
        }
    };
    cache.join("clipsneko-installer").join("log")
}

fn init_tracing() -> Result<()> {
    let path = log_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating log directory {}", parent.display()))?;
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("opening log file {}", path.display()))?;
    tracing_subscriber::fmt()
        .with_writer(file)
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

fn main() -> Result<()> {
    install_panic_hook();
    init_tracing()?;
    set_language(UiLang::En)?;

    enable_raw_mode().context("enable_raw_mode failed")?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen).context("EnterAlternateScreen failed")?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("Terminal::new failed")?;

    let result = app::run(&mut terminal);

    disable_raw_mode().ok();
    execute!(std::io::stdout(), LeaveAlternateScreen).ok();

    result
}
