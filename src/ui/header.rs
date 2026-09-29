//! The header: what is being looked at, and how far the scan has got.
//!
//! Both facts on one line, and the second is not decoration: a scan that ran without
//! administrator rights produces a *shorter* report rather than a wrong one, and
//! nothing on the rest of the screen distinguishes those two cases except this line.
//! So the reduced-coverage marker sits next to the host name rather than in the log,
//! where it will scroll away.

use ratatui::text::{Line, Span};

use crate::app::{App, ScanState};
use crate::report::truncate_to_cells;

/// The engine's name, as the operator knows it.
pub const ENGINE: &str = "irscan-tui";

/// The one-line header.
pub fn widget(app: &App) -> Line<'static> {
    let theme = &app.theme;
    let mut spans = vec![Span::styled(ENGINE, theme.display())];

    if let Some(report) = app.report.as_ref() {
        let host = report.host.sanitised();
        if !host.name.is_empty() {
            spans.push(Span::raw("  "));
            spans.push(Span::styled(
                truncate_to_cells(&host.name, 40),
                theme.display(),
            ));
        }
        if !report.host.admin {
            spans.push(Span::styled(
                "  (not admin · reduced coverage)",
                theme.staged(),
            ));
        }
    }

    spans.push(Span::raw("  "));
    spans.push(Span::styled(state(app), theme.machine()));
    Line::from(spans)
}

/// What the scan is doing, in words.
fn state(app: &App) -> String {
    match &app.scan {
        ScanState::Idle => "idle".to_string(),
        ScanState::Running { done, current } => {
            format!("scanning {done}/{} · {current}", app.collectors_total)
        }
        ScanState::Finished { failed } => {
            if *failed == 0 {
                "scan finished".to_string()
            } else {
                format!("scan finished · {failed} collectors FAILED")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::{Finding, Host, Report};
    use crate::ui::tests_common::app;

    #[test]
    fn the_header_says_coverage_was_reduced_when_the_scan_ran_without_admin_rights() {
        // FR-8 / SR-6. A short report and a wrong report look the same everywhere
        // else, so the header is where the difference has to be visible.
        let mut app = app();
        app.report = Some(Report {
            host: Host {
                name: "PC-01".to_string(),
                admin: false,
                ..Host::default()
            },
            findings: vec![Finding {
                severity: "high".to_string(),
                category: String::new(),
                title: "t".to_string(),
                evidence: Vec::new(),
                remediation: Vec::new(),
            }],
            ..Report::default()
        });
        let text = widget(&app).to_string();
        assert!(text.contains(ENGINE), "the engine name should be shown");
        assert!(text.contains("PC-01"), "the host should be shown: {text}");
        assert!(
            text.contains("reduced coverage"),
            "a non-admin scan must say so: {text}"
        );
    }

    #[test]
    fn the_header_says_nothing_about_coverage_when_the_scan_had_admin_rights() {
        let mut app = app();
        app.report = Some(Report {
            host: Host {
                name: "PC-01".to_string(),
                admin: true,
                ..Host::default()
            },
            ..Report::default()
        });
        assert!(!widget(&app).to_string().contains("reduced coverage"));
    }

    #[test]
    fn the_header_names_the_collector_a_running_scan_is_waiting_on() {
        let mut app = app();
        app.start_scan();
        app.apply(crate::app::Event::Collector(crate::app::CollectorReport {
            name: "events".to_string(),
            elapsed_ms: 12,
            findings_added: 0,
            error: None,
        }));
        assert!(widget(&app).to_string().contains("events"));
    }
}
