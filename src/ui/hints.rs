//! The key hint row: what the keys are, on the screen you are looking at.
//!
//! Only the current screen's bindings. A footer that lists every key on every screen
//! is a footer nobody reads, and the set that changes per screen is exactly the set
//! a user has to be reminded of when they arrive on it.

use ratatui::text::{Line, Span};

use crate::app::{App, Screen};

/// The bindings worth hinting on one screen.
fn hints(screen: Screen) -> &'static [(&'static str, &'static str)] {
    match screen {
        Screen::Dashboard => &[
            ("s", "scan"),
            ("c", "cancel"),
            ("tab", "next"),
            ("?", "help"),
            ("q", "quit"),
        ],
        Screen::Report => &[
            ("j/k", "move"),
            ("/", "search"),
            ("1-4", "severity"),
            ("e", "export"),
            ("?", "help"),
            ("q", "quit"),
        ],
        Screen::Archive => &[
            ("j/k", "move"),
            ("x", "compare"),
            ("d", "delete"),
            ("?", "help"),
            ("q", "quit"),
        ],
        Screen::Live => &[("?", "help"), ("q", "quit")],
        // No `e edit` here: the in-app editor is not built, and a key hint is a
        // promise about what a key does. The hint and the action are removed
        // together so the two cannot drift.
        Screen::Rules => &[("?", "help"), ("q", "quit")],
    }
}

/// The one-line key hint row.
pub fn widget(app: &App) -> Line<'static> {
    let style = app.theme.label(true);
    let mut spans = Vec::new();
    for (index, (key, what)) in hints(app.screen).iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled("  ", style));
        }
        spans.push(Span::styled(*key, style));
        spans.push(Span::styled(format!(" {what}"), style));
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::tests_common::app;

    #[test]
    fn the_hint_row_shows_the_bindings_of_the_screen_in_view_and_no_others() {
        let mut app = app();
        app.screen = Screen::Archive;
        let archive = widget(&app).to_string();
        assert!(archive.contains("compare"), "archive bindings: {archive}");
        app.screen = Screen::Live;
        let live = widget(&app).to_string();
        assert!(
            !live.contains("compare"),
            "the live screen should not advertise the archive's keys: {live}"
        );
    }
}
