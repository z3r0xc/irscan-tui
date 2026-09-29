//! The status line: the one row that has to be readable while everything else moves.
//!
//! Its content is chosen by a fixed precedence rather than by "whatever is most
//! interesting", because a status line that reorders itself is a status line nobody
//! can learn to read. A notice wins, because it is transient and the thing the user
//! most needs to know about; then the visible/total counts, because FR-10 requires
//! them *always*; then the marker for a filter that hides a HIGH, and the comparison
//! delta.

use ratatui::text::{Line, Span};

use crate::app::App;

/// The one-line status bar.
pub fn widget(app: &App) -> Line<'static> {
    let theme = &app.theme;
    if let Some((message, damaging)) = app.notice.as_ref() {
        let style = if *damaging {
            theme.damage()
        } else {
            theme.staged()
        };
        return Line::from(vec![
            Span::styled(if *damaging { "! " } else { "· " }, style),
            Span::styled(message.clone(), style),
        ]);
    }

    let mut spans = vec![Span::styled(
        format!("{} / {}", app.visible.len(), app.rows.len()),
        theme.display(),
    )];

    if app.hides_high() {
        // FR-10. A `!` and not a footnote: the whole point is that it is impossible
        // to read the count without reading this.
        spans.push(Span::styled("  ! HIGH hidden by filter", theme.damage()));
    }

    if let Some(delta) = app.delta.as_ref() {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(delta.summary(), theme.machine()));
    }

    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::tests_common::app_with_findings;

    #[test]
    fn a_damage_notice_replaces_the_counts_and_is_styled_as_damage() {
        let mut app = app_with_findings(3);
        app.notice = Some(("scan could not start".to_string(), true));
        let line = widget(&app);
        assert!(line.to_string().contains("scan could not start"));
        assert!(!line.to_string().contains("3 / 3"));
        assert_eq!(line.spans[1].style, app.theme.damage());
    }

    #[test]
    fn a_staged_notice_is_not_styled_as_damage() {
        // An export confirmation and a scan failure share a field; styling them the
        // same would train the user to ignore the one that matters.
        let mut app = app_with_findings(3);
        app.notice = Some(("3 findings exported".to_string(), false));
        let line = widget(&app);
        assert_eq!(line.spans[1].style, app.theme.staged());
    }

    #[test]
    fn the_delta_summary_is_shown_after_the_counts() {
        let mut app = app_with_findings(3);
        app.delta = Some(crate::app::Delta {
            added: vec![],
            resolved: vec![],
            changed: vec![],
        });
        assert!(widget(&app).to_string().contains("+0 new, -0 resolved"));
    }
}
