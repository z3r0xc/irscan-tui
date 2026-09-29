//! The help overlay: the keymap, over whatever is underneath.
//!
//! Modal, and therefore drawn last and over the whole area rather than into a panel:
//! while it is open every other key is swallowed, and a modal that leaves the screen
//! behind it half-legible invites the reader to act on what they half-read.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};
use ratatui::Frame;

use crate::app::App;

/// The narrowest terminal the overlay is drawn on.
const MIN_WIDTH: u16 = 20;
const MIN_HEIGHT: u16 = 8;
/// The panel's height: a border, a heading and every binding.
const PANEL_HEIGHT: u16 = 20;
const PANEL_WIDTH: u16 = 64;

/// Draw the overlay over `area`.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        // Too small for a panel. The chrome underneath still answers the question of
        // which screen this is, and a clipped overlay is worse than none.
        return;
    }
    let [_, middle, _] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(PANEL_HEIGHT.min(area.height - 2)),
        Constraint::Min(1),
    ])
    .areas(area);
    let [left, _] =
        Layout::horizontal([Constraint::Length(PANEL_WIDTH), Constraint::Min(0)]).areas(middle);

    frame.render_widget(Clear, left);
    frame.render_widget(widget(app), left);
}

/// The overlay itself, for a test that renders it on its own.
pub fn widget(app: &App) -> Paragraph<'static> {
    let theme = &app.theme;
    let mut lines: Vec<Line> = vec![Line::from(Span::styled("KEYS", theme.display()))];
    for (key, what) in bindings() {
        lines.push(Line::from(vec![
            Span::styled(format!("[{key}]"), theme.label(true)),
            Span::raw("  "),
            Span::styled(what, theme.body()),
        ]));
    }
    Paragraph::new(lines).block(
        Block::bordered()
            .border_style(theme.border_focused())
            .title(Span::styled(" HELP ", theme.label(false))),
    )
}

/// Every binding the state machine can perform, as `(key, what)`.
///
/// A binding that is missing here is a feature nobody can find, and one listed here
/// that the state machine does not perform would be a lie in the only place the user
/// goes to check.
fn bindings() -> Vec<(&'static str, &'static str)> {
    vec![
        ("tab", "next screen"),
        ("shift-tab", "previous screen"),
        ("s", "start a scan"),
        ("c", "cancel a running scan"),
        ("j/k", "move the selection"),
        ("pgup/pgdn", "move a page"),
        ("g/G", "first and last row"),
        ("/", "search title, category, evidence and remediation"),
        ("1-4", "toggle HIGH, MED, LOW and INFO"),
        ("c", "clear the filter and the search"),
        ("x", "compare two archived scans of one host"),
        ("d", "delete the selected archived scan"),
        ("e", "export the report in view"),
        ("?", "open and close this keymap"),
        ("q", "quit"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::tests_common::{app, render, text_rows};
    use ratatui::Terminal;

    #[test]
    fn the_overlay_names_a_key_for_every_thing_the_interface_can_do() {
        let app = app();
        let text = text_rows(&render(widget(&app), PANEL_WIDTH, PANEL_HEIGHT)).join("\n");
        for expected in [
            "next screen",
            "previous screen",
            "start a scan",
            "move the selection",
            "search",
            "compare",
            "export",
            "quit",
        ] {
            assert!(
                text.contains(expected),
                "{expected:?} is missing from the keymap: {text}"
            );
        }
    }

    #[test]
    fn a_terminal_too_small_for_the_panel_leaves_the_screen_underneath_untouched() {
        use ratatui::backend::TestBackend;
        let app = app();
        let mut terminal =
            Terminal::new(TestBackend::new(MIN_WIDTH, MIN_HEIGHT)).expect("a test backend");
        terminal
            .draw(|frame| {
                frame.render_widget(Paragraph::new("underneath"), frame.area());
                super::render(frame, frame.area(), &app);
            })
            .expect("a draw should not fail");
        assert!(text_rows(terminal.backend().buffer())
            .join("\n")
            .contains("underneath"));
    }
}
