//! Small reusable layout helpers for the TUI.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Block;

/// Return the shared style for a focused interactive border and its title.
///
/// A focused widget uses a bold white border; an unfocused widget keeps the
/// terminal's default style.
pub fn focused_border_style(focused: bool) -> Style {
    if focused {
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    }
}

/// Apply the shared focus style to both a block's border and title.
pub fn focusable_block(block: Block<'_>, focused: bool) -> Block<'_> {
    let style = focused_border_style(focused);
    block.border_style(style).title_style(style)
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
    fn interactive_border_is_bold_white_only_while_focused() {
        let focused = focused_border_style(true);
        assert_eq!(focused.fg, Some(Color::White));
        assert!(focused.add_modifier.contains(Modifier::BOLD));

        assert_eq!(focused_border_style(false), Style::default());
    }

    #[test]
    fn focusable_block_styles_its_border_and_title_together() {
        let block = Block::default().title("Title");
        let style = focused_border_style(true);

        assert_eq!(
            focusable_block(block.clone(), true),
            block.border_style(style).title_style(style)
        );
    }
}
