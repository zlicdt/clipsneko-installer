//! Small reusable layout helpers for the TUI.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};

/// Return the shared border style for editable text inputs.
///
/// A focused input uses a bold white border; an unfocused input keeps the
/// terminal's default style.
pub fn input_border_style(focused: bool) -> Style {
    if focused {
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_border_is_bold_white_only_while_focused() {
        let focused = input_border_style(true);
        assert_eq!(focused.fg, Some(Color::White));
        assert!(focused.add_modifier.contains(Modifier::BOLD));

        assert_eq!(input_border_style(false), Style::default());
    }
}
