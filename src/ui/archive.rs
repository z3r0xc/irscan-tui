//! The archive: every past scan, and the comparison the operator asked for.
//!
//! Two rows are marked rather than one, because comparison is the whole point of
//! this screen: the row being looked at and the row it is being compared against are
//! both visible at once, and a comparison whose baseline is not on screen is a
//! difference the user cannot interpret.

use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

use crate::app::{App, Archived};
use crate::report::truncate_to_cells;

/// The archive screen.
pub fn widget(app: &App) -> Paragraph<'static> {
    let theme = &app.theme;
    let mut lines: Vec<Line> = Vec::new();

    if app.archive.is_empty() {
        lines.push(Line::from(Span::styled(
            "no scans archived yet — one is written when a scan finishes",
            theme.body(),
        )));
    } else {
        for (index, entry) in app.archive.iter().enumerate() {
            lines.push(entry_line(app, index, entry));
        }
    }

    lines.push(Line::default());
    match app.delta.as_ref() {
        Some(delta) => lines.push(Line::from(vec![
            Span::styled("comparison: ", theme.label(false)),
            Span::styled(delta.summary(), theme.display()),
        ])),
        None => lines.push(Line::from(Span::styled(
            "press x to compare the marked scan against the selected one",
            theme.body(),
        ))),
    }

    Paragraph::new(lines).block(
        Block::bordered()
            .border_style(theme.border_focused())
            .title(Span::styled(" ARCHIVE ", theme.label(false))),
    )
}

/// Draw the archive into `area`.
pub fn render(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    frame.render_widget(widget(app), area);
}

/// One archived scan: marker, host, when it was taken, and what it found.
fn entry_line<'a>(app: &'a App, index: usize, entry: &'a Archived) -> Line<'static> {
    let theme = &app.theme;
    let selected = index == app.archive_selected;
    let comparing = app.archive_compare == Some(index);

    // A `>` for the selected row and `=` for the one it is paired with, so the
    // pairing survives a monochrome terminal.
    let marker = match (selected, comparing) {
        (true, true) => ">",
        (true, false) => ">",
        (false, true) => "=",
        (false, false) => " ",
    };
    let style = if selected {
        theme.focus()
    } else {
        theme.body()
    };
    let counts = format!(
        "{}H {}M {}L {}I",
        entry.counts.high, entry.counts.medium, entry.counts.low, entry.counts.info
    );
    Line::from(vec![
        Span::styled(format!("{marker} "), style),
        Span::styled(
            format!("{:<24}", truncate_to_cells(&entry.host.name, 24)),
            if selected { theme.display() } else { style },
        ),
        Span::styled(
            format!("{:<20}", truncate_to_cells(&entry.taken_at, 20)),
            theme.label(false),
        ),
        Span::styled(counts, theme.machine()),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::{Counts, Finding, Host, Report};
    use crate::ui::tests_common::{app, render, text_rows};

    /// One archived scan of `host`, taken at `taken_at`.
    fn archived(host: &str, taken_at: &str, high: usize) -> Archived {
        let mut report = Report {
            host: Host {
                name: host.to_string(),
                ..Host::default()
            },
            ..Report::default()
        };
        for _ in 0..high {
            report.findings.push(Finding {
                severity: "high".to_string(),
                category: String::new(),
                title: "x".to_string(),
                evidence: Vec::new(),
                remediation: Vec::new(),
            });
        }
        let mut counts = Counts::default();
        for finding in &report.findings {
            counts.add(finding.severity());
        }
        Archived {
            host_key: host.to_string(),
            counts,
            host: report.host.sanitised(),
            verdict: report.verdict.sanitised(),
            findings: report.findings.iter().map(|f| f.sanitised()).collect(),
            taken_at: taken_at.to_string(),
            path: None,
        }
    }

    #[test]
    fn an_empty_archive_says_that_nothing_has_been_archived_yet() {
        let app = app();
        let text = text_rows(&render(widget(&app), 80, 12)).join("\n");
        assert!(text.contains("no scans archived yet"), "got {text}");
    }

    #[test]
    fn every_archived_scan_shows_its_host_its_time_and_its_counts() {
        let mut app = app();
        app.archive = vec![
            archived("PC-01", "2026-09-01 09:00:00", 2),
            archived("PC-02", "2026-09-02 09:00:00", 0),
        ];
        let text = text_rows(&render(widget(&app), 80, 12)).join("\n");
        assert!(
            text.contains("PC-01") && text.contains("2026-09-01"),
            "got {text}"
        );
        assert!(
            text.contains("PC-02") && text.contains("2026-09-02"),
            "got {text}"
        );
        assert!(text.contains("2H"), "the counts should be shown: {text}");
    }

    #[test]
    fn the_selected_and_the_compared_scan_are_both_marked() {
        // A comparison whose baseline is not visible is a difference the user cannot
        // interpret.
        let mut app = app();
        app.screen = crate::app::Screen::Archive;
        app.archive = vec![
            archived("PC-01", "2026-09-01 09:00:00", 1),
            archived("PC-01", "2026-09-08 09:00:00", 3),
        ];
        app.archive_selected = 1;
        app.archive_compare = Some(0);
        app.compare();
        let text = text_rows(&render(widget(&app), 80, 12)).join("\n");
        let compared = text
            .lines()
            .find(|line| line.contains("2026-09-01"))
            .expect("the baseline should be on screen");
        assert!(compared.contains("= PC-01"), "got {compared:?}");
    }

    #[test]
    fn a_comparison_shows_what_moved_between_the_two_scans() {
        let mut app = app();
        app.screen = crate::app::Screen::Archive;
        app.archive = vec![
            archived("PC-01", "2026-09-01 09:00:00", 1),
            archived("PC-01", "2026-09-08 09:00:00", 3),
        ];
        app.archive_selected = 1;
        app.archive_compare = Some(0);
        app.compare();
        let delta = app.delta.as_ref().expect("two scans of one host compare");
        let text = text_rows(&render(widget(&app), 80, 12)).join("\n");
        assert!(text.contains(&delta.summary()), "got {text}");
    }
}
