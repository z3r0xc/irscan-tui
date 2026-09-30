//! The shell: one layout, one draw, and nothing above the status line that moves
//! when data arrives.
//!
//! `docs/design.md` section 2.6 reserves every region's height except the content,
//! and `docs/spec.md` QR-7 makes that testable rather than aspirational: the header,
//! status and hint rows are byte-identical at 0 findings and at 400. That property
//! is not a consequence of the widgets being careful, it is a consequence of the
//! *layout* being fixed, so the five constraints below are the load-bearing code of
//! this module and nothing else here is allowed to grow a row.
//!
//! Rendering takes `&App` and mutates nothing. A draw that could change state would
//! make the bar's rate depend on how often the screen happened to be painted, which
//! is the exact failure `motion::Meter::value` is written to avoid.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::Frame;

use crate::app::{App, Screen};

pub mod archive;
pub mod dashboard;
pub mod finding_row;
pub mod header;
pub mod help;
pub mod hints;
pub mod live;
pub mod meter;
pub mod report;
pub mod rules;
pub mod status;
pub mod tabs;

/// Draw the whole interface.
pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let [header, tab_bar, content, status, hints] = Layout::vertical([
        Constraint::Length(1), // header
        Constraint::Length(1), // tab bar
        Constraint::Min(0),    // content: the only region that grows
        Constraint::Length(1), // status
        Constraint::Length(1), // key hints
    ])
    .areas(area);

    frame.render_widget(header::widget(app), header);
    frame.render_widget(tabs::widget(app), tab_bar);
    match app.screen {
        Screen::Dashboard => frame.render_widget(dashboard::widget(app, content.width), content),
        Screen::Report => frame.render_widget(report::widget(app), content),
        Screen::Archive => frame.render_widget(archive::widget(app), content),
        Screen::Live => frame.render_widget(live::widget(app), content),
        Screen::Rules => frame.render_widget(rules::widget(app), content),
    }
    frame.render_widget(status::widget(app), status);
    frame.render_widget(hints::widget(app), hints);

    if app.help_open {
        help::render(frame, area, app);
    }
}

/// The content region for a given full-screen area, for tests and for anything that
/// needs to reason about the reserved rows.
pub fn content_area(area: Rect) -> Rect {
    let [_header, _tabs, content, _status, _hints] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(area);
    content
}

#[cfg(test)]
pub(crate) mod tests_common {
    //! Fixtures every screen's tests share, kept in one place so a screen's test says
    //! what it is testing rather than how a row is built.

    use crate::app::{App, Row};
    use crate::motion::Durations;
    use crate::report::Finding;
    use crate::theme::{Depth, Sev, Theme};
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;

    /// A deterministic app: no animation, so two draws differ only in their data.
    pub fn app() -> App {
        App::new(Theme::new(Depth::TrueColor), Durations::ZERO)
    }

    /// The engine's own spelling of a severity, which is what `Finding` carries.
    pub fn severity_name(severity: Sev) -> &'static str {
        match severity {
            Sev::High => "high",
            Sev::Medium => "med",
            Sev::Low => "low",
            Sev::Info => "info",
        }
    }

    pub fn row(severity: Sev, title: &str) -> Row {
        row_with(severity, title, "autoruns", &["one"], &["fix it"])
    }

    /// A finding with every part spelled out, for tests that care about the parts.
    pub fn row_with(
        severity: Sev,
        title: &str,
        category: &str,
        evidence: &[&str],
        remediation: &[&str],
    ) -> Row {
        Row::from_sanitised(
            &Finding {
                severity: severity_name(severity).to_string(),
                category: category.to_string(),
                title: title.to_string(),
                evidence: evidence.iter().map(|s| (*s).to_string()).collect(),
                remediation: remediation.iter().map(|s| (*s).to_string()).collect(),
            }
            .sanitised(),
            false,
        )
    }

    /// An app holding one finding per entry, loaded the way a scan loads them.
    ///
    /// The report claims administrator rights, so the header carries the same text
    /// an empty app's does and the layout test can compare the two directly. A
    /// report that really ran unprivileged does say so, and the header test for that
    /// builds its own.
    pub fn app_with_severities(severities: &[Sev]) -> App {
        let mut app = app();
        let mut report = crate::report::Report {
            host: crate::report::Host {
                admin: true,
                ..crate::report::Host::default()
            },
            ..crate::report::Report::default()
        };
        for (index, severity) in severities.iter().enumerate() {
            report.findings.push(Finding {
                severity: severity_name(*severity).to_string(),
                category: format!("category{index}"),
                title: format!("finding {index}"),
                evidence: vec!["evidence".to_string()],
                remediation: vec!["remediate".to_string()],
            });
        }
        app.load_report(report);
        app
    }

    /// An app holding `count` findings, cycling through the four severities.
    pub fn app_with_findings(count: usize) -> App {
        app_with_severities(
            &(0..count)
                .map(|index| Sev::ALL[index % 4])
                .collect::<Vec<_>>(),
        )
    }

    /// Every row of a buffer, as one string per row.
    pub fn text_rows(buffer: &Buffer) -> Vec<String> {
        let area = *buffer.area();
        (0..area.height)
            .map(|y| {
                (0..area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect()
    }

    /// The buffer of a single rendered widget.
    pub fn render<W: ratatui::widgets::Widget>(widget: W, width: u16, height: u16) -> Buffer {
        let area = Rect::new(0, 0, width, height);
        let mut buffer = Buffer::empty(area);
        widget.render(area, &mut buffer);
        buffer
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Screen;
    use crate::theme::Sev;
    use crate::ui::tests_common::{app, app_with_findings, app_with_severities, row, text_rows};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    /// Render the whole shell and hand back its buffer as rows of text.
    fn render_rows(app: &App, width: u16, height: u16) -> Vec<String> {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("a test backend");
        terminal
            .draw(|frame| draw(frame, app))
            .expect("a draw should not fail");
        let buffer = terminal.backend().buffer().clone();
        text_rows(&buffer)
    }

    #[test]
    fn the_header_status_and_hint_rows_do_not_move_when_findings_arrive() {
        // QR-7. The single most important layout property in the system: a user
        // reading the status line must be able to keep their eye on it while a scan
        // fills the screen underneath. If this fails, some widget is sized by its
        //
        // The header and the key hints are compared byte for byte. The status row is
        // compared with its count masked, because FR-10 requires the count *on* the
        // status line: the digits have to change when findings arrive. What must not
        // change is where the row is, what else is in it, or where the count sits
        // inside it - and that is what the masked comparison pins.
        let empty = render_rows(&app_with_severities(&[]), 80, 24);
        let full = render_rows(&app_with_findings(400), 80, 24);

        assert_eq!(empty.len(), full.len(), "the screen changed height");
        assert_eq!(empty[0], full[0], "the header row moved");
        assert_eq!(empty[1], full[1], "the tab bar row moved");
        assert_eq!(
            without_count(&empty[empty.len() - 2]),
            without_count(&full[full.len() - 2]),
            "the status row moved"
        );
        assert_eq!(
            empty[empty.len() - 1],
            full[full.len() - 1],
            "the key hint row moved"
        );
    }

    /// A status row reduced to the chrome that follows the count, trailing padding
    /// trimmed.
    fn without_count(row: &str) -> String {
        let trimmed = row.trim();
        let Some(separator) = trimmed.find(" / ") else {
            return trimmed.to_string();
        };
        let rest = &trimmed[separator + 3..];
        let digits = rest
            .find(|ch: char| !ch.is_ascii_digit())
            .unwrap_or(rest.len());
        rest[digits..].to_string()
    }

    #[test]
    fn every_screen_renders_at_eighty_columns_and_at_two_hundred() {
        // The two widths that matter: the narrowest terminal a triage box is ever
        // given, and one wide enough for a detail pane beside a list.
        for screen in Screen::ALL {
            for width in [80u16, 200] {
                let mut app = app_with_findings(12);
                app.screen = screen;
                let rows = render_rows(&app, width, 24);
                assert_eq!(rows.len(), 24, "{screen:?} at {width} lost a row");
                // The chrome is painted at every size: a blank reserved row is a
                // hole in the frame, not a gap.
                assert!(
                    !rows[0].trim().is_empty(),
                    "{screen:?} at {width} drew no header"
                );
                assert!(
                    !rows[rows.len() - 1].trim().is_empty(),
                    "{screen:?} at {width} drew no key hints"
                );
            }
        }
    }

    #[test]
    fn a_screen_with_no_data_renders_its_empty_state_rather_than_a_blank_panel() {
        // An empty panel is indistinguishable from a hung program.
        for screen in Screen::ALL {
            let mut app = app();
            app.screen = screen;
            let buffer = {
                let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("a test backend");
                terminal
                    .draw(|frame| draw(frame, &app))
                    .expect("a draw should not fail");
                terminal.backend().buffer().clone()
            };
            let content = content_area(*buffer.area());
            let drawn: String = (content.y..content.y + content.height)
                .map(|y| {
                    (0..content.width)
                        .map(|x| buffer[(x, y)].symbol())
                        .collect::<String>()
                })
                .collect::<Vec<_>>()
                .join("\n");
            assert!(
                !drawn.trim().is_empty(),
                "{screen:?} with no data drew a blank panel"
            );
        }
    }

    #[test]
    fn a_hostile_string_in_a_finding_cannot_smuggle_an_escape_into_the_buffer() {
        // SR-1. The Row carries already-cleaned strings in production, but the
        // render path must not depend on that: a title stuffed with escapes and a
        // bidi override is the attack, and the cell buffer is where it would land.
        let mut app = app();
        let mut hostile = row(Sev::High, "innocent");
        hostile.title = "\u{1b}[31mred\u{1b}[0m \u{202e}exe.txt \u{7} \u{9b}31m".to_string();
        app.rows = vec![hostile];
        app.visible = vec![0];
        app.screen = Screen::Report;

        let buffer = {
            let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("a test backend");
            terminal
                .draw(|frame| draw(frame, &app))
                .expect("a draw should not fail");
            terminal.backend().buffer().clone()
        };
        for y in 0..buffer.area().height {
            for x in 0..buffer.area().width {
                let symbol = buffer[(x, y)].symbol();
                assert!(
                    !symbol.chars().any(|ch| ch.is_control() || ch == '\u{202e}'),
                    "a control or override character reached the buffer at {x},{y}: {symbol:?}"
                );
            }
        }
    }

    #[test]
    fn the_visible_and_total_counts_are_both_on_screen() {
        // FR-10. Both numbers, always: a filter that shows "3" without saying "of 7"
        // is indistinguishable from a report that found three things.
        // Two of each severity plus one extra MED, so turning HIGH and LOW off leaves
        // three of seven.
        let mut app = app_with_severities(&[
            Sev::High,
            Sev::Low,
            Sev::Medium,
            Sev::Info,
            Sev::High,
            Sev::Low,
            Sev::Medium,
        ]);
        app.filter.high = false;
        app.filter.low = false;
        app.refilter();
        assert_eq!(app.visible.len(), 3, "the fixture should leave three rows");

        let rows = render_rows(&app, 80, 24);
        assert!(
            rows[rows.len() - 2].contains("3 / 7"),
            "the status line should carry both counts, got {:?}",
            rows[rows.len() - 2]
        );
    }

    #[test]
    fn a_filter_that_hides_a_high_finding_puts_a_marker_in_the_status_line() {
        // FR-10's stated consequence: hiding a HIGH must raise a visible marker, not
        // only change a count, or a filter can make a compromised machine look clean.
        let mut app = app_with_findings(7);
        app.filter.high = false;
        app.refilter();
        assert!(app.hides_high());

        let rows = render_rows(&app, 80, 24);
        let status = &rows[rows.len() - 2];
        assert!(
            status.contains('!'),
            "a filter hiding a HIGH should mark the status line, got {status:?}"
        );
    }

    #[test]
    fn the_help_overlay_covers_the_screen_it_is_shown_over() {
        let mut app = app();
        app.screen = Screen::Report;
        let closed = render_rows(&app, 80, 24);
        app.help_open = true;
        let open = render_rows(&app, 80, 24);
        assert_ne!(closed, open, "the help overlay changed nothing");
        assert!(
            open.join("\n").contains("KEYS") || open.iter().any(|r| r.contains("[?")),
            "the overlay should show a keymap"
        );
    }
}
