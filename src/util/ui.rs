//! Small reusable layout helpers for the TUI.

use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// Return a horizontally centered rectangle whose width is a percentage of
/// `area.width` and whose height is a fixed row count. Both dimensions are
/// clamped to the available area.
pub fn centered_rect(width_pct: u16, height_rows: u16, area: Rect) -> Rect {
    let h = height_rows.min(area.height);
    let y = area.y + (area.height - h) / 2;
    let width_pct = width_pct.min(100);
    let w_pad = (100 - width_pct) / 2;
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(w_pad),
            Constraint::Percentage(width_pct),
            Constraint::Percentage(w_pad),
        ])
        .split(area);
    let inner = horizontal[1];
    Rect::new(inner.x, y, inner.width, h)
}
