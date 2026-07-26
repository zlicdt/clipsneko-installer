//! Small reusable layout helpers for the TUI.

use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

/// Spinner frames for indeterminate-progress dialogs, cycled by `Step::tick`.
pub const SPINNER: [char; 4] = ['|', '/', '-', '\\'];

/// Render a centered modal "working" dialog: a rounded box holding one line
/// of bold text prefixed by the spinner frame at `spinner_idx` (e.g.
/// `"/ Checking network connectivity…"`). It is drawn last so it overlays the
/// step body; while it is visible the owning step must report
/// `has_modal() == true` so keys cannot reach the body behind the overlay.
pub fn render_loading_dialog(frame: &mut Frame, text: &str, spinner_idx: usize) {
    let spinner = SPINNER[spinner_idx % SPINNER.len()];
    let body = Paragraph::new(vec![
        Line::from(""),
        Line::from(ratatui::text::Span::styled(
            format!("{spinner} {text}"),
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ])
    .alignment(Alignment::Center)
    .wrap(Wrap { trim: true })
    .block(rounded_block());
    render_autosized_dialog(frame, frame.area(), 60, body);
}

/// Render `dialog` as a centered modal overlay whose height follows the
/// paragraph's own wrapped line count, so translated text of any length keeps
/// its last line (usually the dismiss hint or the button row) visible. The
/// paragraph must already carry its block and, when its text can exceed the
/// dialog width, a `Wrap`; `line_count` takes the inner text width and adds
/// the block's top/bottom borders itself. Height is clamped to `area`.
pub fn render_autosized_dialog(frame: &mut Frame, area: Rect, width_pct: u16, dialog: Paragraph) {
    let width = ((u32::from(area.width) * u32::from(width_pct.min(100))) / 100) as u16;
    let inner_width = width.saturating_sub(2).max(1);
    let height = dialog.line_count(inner_width).min(usize::from(area.height)) as u16;
    let dialog_area = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, dialog_area);
    frame.render_widget(dialog, dialog_area);
}

/// Greedy display-width word wrap for plain text, for widgets that cannot
/// wrap by themselves (e.g. `List` items). Breaks at spaces when possible
/// and inside overlong words otherwise (CJK text has no spaces to break at).
/// Always returns at least one line.
pub fn wrap_plain(text: &str, width: u16) -> Vec<String> {
    use unicode_width::UnicodeWidthChar;
    let max = usize::from(width.max(1));
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    let mut line_width = 0usize;
    for word in text.split(' ') {
        let word_width: usize = word.chars().map(|c| c.width().unwrap_or(0)).sum();
        let sep = usize::from(!line.is_empty());
        if line_width + sep + word_width <= max {
            if sep == 1 {
                line.push(' ');
            }
            line.push_str(word);
            line_width += sep + word_width;
            continue;
        }
        if !line.is_empty() {
            lines.push(std::mem::take(&mut line));
            line_width = 0;
        }
        if word_width <= max {
            line.push_str(word);
            line_width = word_width;
        } else {
            for c in word.chars() {
                let c_width = c.width().unwrap_or(0);
                if line_width + c_width > max && !line.is_empty() {
                    lines.push(std::mem::take(&mut line));
                    line_width = 0;
                }
                line.push(c);
                line_width += c_width;
            }
        }
    }
    if !line.is_empty() || lines.is_empty() {
        lines.push(line);
    }
    lines
}

/// Return the shared block with all four borders rendered using rounded corners.
pub fn rounded_block() -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
}

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

/// Return a centered rectangle whose width is a percentage of `area.width`
/// and whose height is a fixed row count. Both dimensions are clamped to the
/// available area and the position is computed in cells, so odd percentages
/// stay centered instead of drifting by a leftover percentage column.
pub fn centered_rect(width_pct: u16, height_rows: u16, area: Rect) -> Rect {
    let w = ((u32::from(area.width) * u32::from(width_pct.min(100))) / 100) as u16;
    let h = height_rows.min(area.height);
    Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    )
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

    #[test]
    fn wrap_plain_breaks_at_spaces_and_inside_cjk_runs() {
        assert_eq!(
            wrap_plain("nvidia-open-dkms (needs linux or linux-zen)", 20),
            vec!["nvidia-open-dkms", "(needs linux or", "linux-zen)"]
        );
        // CJK: 6 chars of display width 2 wrap at a 2-char/4-cell boundary.
        assert_eq!(wrap_plain("インストール", 4), vec!["イン", "スト", "ール"]);
        assert_eq!(wrap_plain("", 10), vec![""]);
    }

    #[test]
    fn centered_rect_is_exactly_centered_in_cells() {
        let area = Rect::new(0, 0, 80, 24);
        let rect = centered_rect(75, 8, area);
        assert_eq!(rect, Rect::new(10, 8, 60, 8));
    }

    #[test]
    fn rounded_block_uses_the_shared_border_shape() {
        assert_eq!(
            rounded_block(),
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
        );
    }
}
