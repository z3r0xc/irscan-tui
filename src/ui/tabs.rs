//! The tab bar: five screens, one of them lit.
//!
//! Every label is on the bar at all times, so the interface never hides what exists
//! behind a key. The active one is in `focus()` and the rest in `label()`; the
//! separators are hairlines in `line`, which is the only decoration this bar has.

use ratatui::style::Modifier;
use ratatui::text::{Line, Span};

use crate::app::{App, Screen};

/// The one-line tab bar.
pub fn widget(app: &App) -> Line<'static> {
    let theme = &app.theme;
    let mut spans: Vec<Span> = Vec::new();
    for (index, screen) in Screen::ALL.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled(" │ ", theme.border()));
        }
        if *screen == app.screen {
            // Underline is the second non-colour signal for the active tab, so the
            // bar still says which screen is current on a monochrome terminal.
            spans.push(Span::styled(
                screen.label(),
                theme.focus().add_modifier(Modifier::UNDERLINED),
            ));
        } else {
            spans.push(Span::styled(screen.label(), theme.label(false)));
        }
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::tests_common::app;

    #[test]
    fn every_screen_is_named_on_the_bar_and_exactly_one_is_focused() {
        let mut app = app();
        for screen in Screen::ALL {
            app.screen = screen;
            let spans = widget(&app).spans;
            let focused: Vec<String> = spans
                .iter()
                .filter(|s| s.style == app.theme.focus().add_modifier(Modifier::UNDERLINED))
                .map(|s| s.content.to_string())
                .collect();
            assert_eq!(
                focused,
                vec![screen.label().to_string()],
                "{screen:?} should be the only focused tab"
            );
            let text: String = spans.iter().map(|s| s.content.to_string()).collect();
            for other in Screen::ALL {
                assert!(text.contains(other.label()), "{other:?} is missing: {text}");
            }
        }
    }
}
