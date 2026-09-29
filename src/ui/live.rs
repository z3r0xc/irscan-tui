//! The live log: what the scan is doing, as it does it.
//!
//! Newest last, so the eye reads downward into the present and the bottom of the
//! screen is always the current moment. Each line's style comes from its `LogKind`
//! and nothing else - a log line has no severity of its own, it inherits the claim
//! of the thing it is reporting.

use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

use crate::app::{App, LogKind};

/// The live screen.
pub fn widget(app: &App) -> Paragraph<'static> {
    let mut lines = lines(app);
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "nothing logged yet — press s to scan",
            app.theme.body(),
        )));
    }
    Paragraph::new(lines).block(
        Block::bordered()
            .border_style(app.theme.border_focused())
            .title(Span::styled(" LIVE ", app.theme.label(false))),
    )
}

/// Draw the live screen into `area`.
pub fn render(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    frame.render_widget(widget(app), area);
}

/// The log as lines, oldest first.
fn lines(app: &App) -> Vec<Line<'static>> {
    app.log
        .iter()
        .map(|entry| Line::from(Span::styled(entry.text.clone(), style(app, entry.kind))))
        .collect()
}

/// The style a log line is drawn in.
///
/// `LogKind::Finding` takes the display step: a finding is the one thing a user
/// opened the live view to watch arrive, and the log is where they watch for it.
fn style(app: &App, kind: LogKind) -> Style {
    let theme = &app.theme;
    match kind {
        LogKind::Failure => theme.damage(),
        LogKind::Warning => theme.staged(),
        LogKind::Finding => theme.display(),
        LogKind::Progress => theme.body(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::tests_common::{app, render, text_rows};

    #[test]
    fn an_empty_log_says_the_scan_has_not_produced_anything_yet() {
        let app = app();
        let text = text_rows(&render(widget(&app), 80, 12)).join("\n");
        assert!(text.contains("nothing logged"), "got {text}");
    }

    #[test]
    fn the_log_arrives_oldest_first_so_the_newest_line_is_at_the_bottom() {
        let mut app = app();
        app.push_log(LogKind::Progress, "first".to_string());
        app.push_log(LogKind::Progress, "second".to_string());
        let text = text_rows(&render(widget(&app), 80, 12));
        let first = text.iter().position(|l| l.contains("first"));
        let second = text.iter().position(|l| l.contains("second"));
        assert!(first < second, "got {text:?}");
    }

    #[test]
    fn each_kind_of_log_line_is_drawn_in_its_own_style() {
        let mut app = app();
        for kind in [
            LogKind::Progress,
            LogKind::Finding,
            LogKind::Warning,
            LogKind::Failure,
        ] {
            app.push_log(kind, "x".to_string());
        }
        let theme = app.theme;
        let drawn: Vec<Style> = lines(&app).iter().map(|l| l.spans[0].style).collect();
        assert_eq!(
            drawn,
            vec![
                theme.body(),
                theme.display(),
                theme.staged(),
                theme.damage()
            ]
        );
    }

    #[test]
    fn a_failure_in_the_log_is_readable_even_with_no_colour() {
        use crate::theme::{Depth, Theme};
        let mut app = app();
        app.theme = Theme::new(Depth::None);
        app.push_log(LogKind::Failure, "the disk is full".to_string());
        let buffer = render(widget(&app), 80, 12);
        // The words carry the claim; the style is weight, not hue.
        assert!(text_rows(&buffer).join("\n").contains("the disk is full"));
    }
}
