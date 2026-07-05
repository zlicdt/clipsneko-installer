//! Entry point: initialize i18n + tracing, then hand off to the TUI.

mod i18n;

use anyhow::{Context, Result};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::{text::Line, widgets::Paragraph, Terminal};
use std::io::Stdout;

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

fn run_app(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    let greeting = t!("Welcome to ClipsNeko Linux Installer");
    let hint = t!("Press q to quit");
    terminal.draw(|frame| {
        let area = frame.area();
        let widget = Paragraph::new(vec![
            Line::from(greeting.clone()),
            Line::from(""),
            Line::from(hint.clone()),
        ]);
        frame.render_widget(widget, area);
    })?;
    loop {
        if let ratatui::crossterm::event::Event::Key(key) = ratatui::crossterm::event::read()? {
            if key.kind == ratatui::crossterm::event::KeyEventKind::Press
                && (key.code == ratatui::crossterm::event::KeyCode::Char('q')
                    || key.code == ratatui::crossterm::event::KeyCode::Char('Q')
                    || key.code == ratatui::crossterm::event::KeyCode::Esc)
            {
                break;
            }
        }
    }
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

    let result = run_app(&mut terminal);

    disable_raw_mode().ok();
    execute!(std::io::stdout(), LeaveAlternateScreen).ok();

    result
}
