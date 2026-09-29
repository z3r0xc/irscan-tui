//! The scan screen: the meter, the collectors, and the two things a scan can tell
//! you before it has found anything.
//!
//! A failed collector is drawn in `damage()` and keeps its place in the list rather
//! than disappearing, because "we found nothing" and "we could not look" are the two
//! claims this tool exists to keep apart (FR-7). For the same reason the blind spots
//! of a non-admin scan are named, not counted.

use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

use crate::app::{App, CollectorReport, Focus, ScanState};

/// The dashboard.
pub fn widget(app: &App) -> Paragraph<'static> {
    let theme = &app.theme;
    let mut lines: Vec<Line> = vec![
        Line::from(vec![
            Span::styled(
                format!("{}/{} collectors", progress(app), app.collectors_total),
                theme.display(),
            ),
            Span::raw("  "),
        ]),
        crate::ui::meter::bar(app, 40),
        Line::default(),
    ];

    if let Some(report) = app.report.as_ref() {
        for spot in report.host.blind_spots() {
            lines.push(Line::from(vec![
                Span::styled("blind spot: ", theme.staged()),
                Span::styled(spot.to_string(), theme.body()),
            ]));
        }
        lines.push(Line::default());
    }

    if app.collectors.is_empty() {
        lines.push(Line::from(Span::styled(idle_message(app), theme.body())));
    } else {
        for collector in &app.collectors {
            lines.push(collector_line(app, collector));
        }
    }

    Paragraph::new(lines).block(
        Block::bordered()
            .border_style(if app.focus == Focus::Primary {
                theme.border_focused()
            } else {
                theme.border()
            })
            .title(Span::styled(" SCAN ", theme.label(false))),
    )
}

/// Draw the dashboard into `area`.
pub fn render(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    frame.render_widget(widget(app), area);
}

/// How many collectors have reported.
fn progress(app: &App) -> usize {
    match &app.scan {
        ScanState::Running { done, .. } => *done,
        ScanState::Idle => 0,
        // A finished scan has all of them, whether or not they succeeded: the
        // failures are named individually further down.
        ScanState::Finished { .. } => app.collectors.len(),
    }
}

/// What to say when no scan is running and none has been loaded.
fn idle_message(app: &App) -> &'static str {
    if matches!(app.scan, ScanState::Finished { .. }) {
        "scan finished — open REPORT to read it"
    } else {
        "press s to scan this machine"
    }
}

/// One collector's line: name, how long it took, what it found, or why it could not
/// run at all.
fn collector_line<'a>(app: &'a App, collector: &'a CollectorReport) -> Line<'static> {
    let theme = &app.theme;
    match collector.error.as_ref() {
        Some(error) => Line::from(vec![
            Span::styled(format!("{:<16}", collector.name), theme.damage()),
            Span::styled("FAILED", theme.damage()),
            Span::styled(format!("  {error}"), theme.body()),
        ]),
        None => Line::from(vec![
            Span::styled(format!("{:<16}", collector.name), theme.body()),
            Span::styled(
                format!("{:>6} ms", collector.elapsed_ms),
                theme.label(false),
            ),
            Span::styled(
                format!("  {} findings", collector.findings_added),
                theme.machine(),
            ),
        ]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Event;
    use crate::report::{Host, Report};
    use crate::ui::tests_common::{app, render, text_rows};

    #[test]
    fn an_idle_dashboard_says_which_key_starts_a_scan() {
        let app = app();
        let text = text_rows(&render(widget(&app), 80, 12)).join("\n");
        assert!(text.contains("press s to scan"), "got {text}");
    }

    #[test]
    fn a_failed_collector_stays_on_screen_and_is_marked_as_a_failure() {
        // FR-7. A collector that vanished is indistinguishable from a collector that
        // found nothing.
        let mut app = app();
        app.apply(Event::Collector(CollectorReport {
            name: "events".to_string(),
            elapsed_ms: 3,
            findings_added: 0,
            error: Some("access denied".to_string()),
        }));
        let text = text_rows(&render(widget(&app), 80, 12)).join("\n");
        assert!(text.contains("events"), "got {text}");
        assert!(text.contains("FAILED"), "got {text}");
        assert!(text.contains("access denied"), "got {text}");
    }

    #[test]
    fn a_scan_that_ran_without_admin_rights_names_what_it_could_not_look_at() {
        let mut app = app();
        app.report = Some(Report {
            host: Host {
                name: "PC-01".to_string(),
                admin: false,
                ..Host::default()
            },
            findings: Vec::new(),
            ..Report::default()
        });
        let text = text_rows(&render(widget(&app), 80, 16)).join("\n");
        assert!(text.contains("Security event log"), "got {text}");
        assert!(text.contains("Prefetch"), "got {text}");
    }

    #[test]
    fn the_dashboard_grows_a_list_without_growing_the_frame() {
        let mut app = app();
        let short = text_rows(&render(widget(&app), 80, 12));
        for index in 0..40 {
            app.apply(Event::Collector(CollectorReport {
                name: format!("collector{index}"),
                elapsed_ms: index,
                findings_added: 0,
                error: None,
            }));
        }
        let long = text_rows(&render(widget(&app), 80, 12));
        assert_eq!(short.len(), long.len());
    }
}
