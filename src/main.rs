//! Entry point: initialize i18n + tracing, then hand off to the TUI wizard.

mod app;
mod i18n;
mod state;
mod steps;

use anyhow::{Context, Result};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::i18n::{set_language, UiLang};

fn init_tracing() -> Result<()> {
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/var/log/clipsneko-installer.log")
        .with_context(|| "opening /var/log/clipsneko-installer.log")?;
    tracing_subscriber::fmt()
        .with_writer(file)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init()
        .map_err(|e| anyhow::anyhow!("tracing init failed: {e}"))?;
    Ok(())
}

fn main() -> Result<()> {
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
