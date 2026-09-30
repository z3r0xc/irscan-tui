//! The report screen: the findings list beside the detail of the selected one.
//!
//! FR-9. The detail pane is not a bonus - a title with no evidence under it is a
//! claim, and the claim is the thing being judged. So the pane exists from the first
//! row, and it shows the evidence and the remediation of whatever
//! `app.selected_row()` returns, which by construction is the row the list is
//! highlighting.

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Widget};

use crate::app::{App, Focus};
use crate::report::truncate_to_cells;
use crate::theme::Sev;

/// Rows the list's chrome costs: two borders, the filter line, the verdict line and
/// a gap. The verdict is chrome rather than a finding, so it scrolls with everything
/// else and the list's capacity loses one row rather than a finding being pushed off
/// the bottom.
const LIST_CHROME: u16 = 5;

/// The report screen.
pub fn widget(app: &App) -> Report<'_> {
    Report { app }
}

/// The whole screen as one widget, so the shell renders it in one call.
pub struct Report<'a> {
    app: &'a App,
}

impl Widget for Report<'_> {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let [list_area, detail_area] =
            Layout::horizontal([Constraint::Percentage(55), Constraint::Percentage(45)])
                .areas(area);
        let list = Paragraph::new(self.list(list_area)).block(self.list_border());
        list.render(list_area, buffer);
        Paragraph::new(self.detail(detail_area))
            .block(self.detail_border())
            .render(detail_area, buffer);
    }
}

impl Report<'_> {
    /// The findings list, scrolled to `app.offset`, with the filter state on top.
    fn list(&self, area: Rect) -> Vec<Line<'static>> {
        let theme = &self.app.theme;
        let width = area.width.saturating_sub(2);
        let capacity = usize::from(area.height.saturating_sub(LIST_CHROME));
        let mut lines: Vec<Line> = vec![
            self.filter_line(),
            self.verdict_line(width),
            Line::default(),
        ];

        if self.app.visible.is_empty() {
            lines.push(Line::from(Span::styled(self.empty_message(), theme.body())));
            return lines;
        }
        let last = self.app.visible.len() - 1;
        let first = self.app.offset.min(last);
        for slot in 0..capacity {
            let index = first + slot;
            if index > last {
                break;
            }
            lines.push(crate::ui::finding_row::row(self.app, index, width));
        }
        lines
    }

    /// The engine's own judgement of this machine, in its own words.
    ///
    /// FR-8 says the verdict is visible, and it is the one line on this screen the
    /// engine wrote rather than this front end: "требуется внимание" alongside seven
    /// findings says something a severity count cannot, because it is a conclusion
    /// and not a tally. It was decoded, sanitised and stored the whole time and
    /// simply never drawn.
    fn verdict_line(&self, width: u16) -> Line<'static> {
        let theme = &self.app.theme;
        let Some(verdict) = self.app.report.as_ref().map(|r| &r.verdict) else {
            return Line::default();
        };
        if verdict.headline.is_empty() {
            return Line::default();
        }
        Line::from(vec![
            Span::styled("verdict: ", theme.label(false)),
            Span::styled(
                truncate_to_cells(&verdict.headline, usize::from(width).saturating_sub(8)),
                theme.display(),
            ),
        ])
    }

    /// The evidence and remediation of the selected finding.
    fn detail(&self, area: Rect) -> Vec<Line<'static>> {
        let theme = &self.app.theme;
        let budget = usize::from(area.width);
        let mut lines: Vec<Line> = Vec::new();
        let Some(row) = self.app.selected_row() else {
            lines.push(Line::from(Span::styled(
                "no finding selected",
                theme.body(),
            )));
            return lines;
        };
        let title = if row.severity.emphatic() {
            theme.display()
        } else {
            theme.body()
        };
        lines.push(Line::from(Span::styled(
            truncate_to_cells(&row.title, budget),
            title,
        )));
        lines.push(Line::from(Span::styled(
            format!("{} · {}", row.severity.tag(), row.category),
            theme.label(false),
        )));
        lines.push(Line::default());
        lines.extend(self.section("EVIDENCE", &row.evidence, budget, true));
        lines.push(Line::default());
        lines.extend(self.section("REMEDIATION", &row.remediation, budget, false));
        lines
    }

    /// A titled block of lines, or a line saying the block is empty.
    ///
    /// A blank section and a missing finding look the same on screen, and they are
    /// not the same claim.
    fn section(
        &self,
        title: &'static str,
        items: &[String],
        budget: usize,
        machine: bool,
    ) -> Vec<Line<'static>> {
        let theme = &self.app.theme;
        let style = if machine {
            theme.machine()
        } else {
            theme.body()
        };
        let mut lines = vec![Line::from(Span::styled(title, theme.label(false)))];
        if items.is_empty() {
            lines.push(Line::from(Span::styled(
                format!("  the engine reported no {title} for this finding"),
                theme.body(),
            )));
        } else {
            for line in items {
                lines.push(Line::from(Span::styled(
                    truncate_to_cells(&format!("  {line}"), budget),
                    style,
                )));
            }
        }
        lines
    }

    /// The severities on and off, so the list says what it is hiding.
    fn filter_line(&self) -> Line<'static> {
        let theme = &self.app.theme;
        let mut spans = vec![Span::styled("filter: ", theme.label(false))];
        for severity in Sev::ALL.iter().rev() {
            let on = self.app.filter.allows(*severity);
            let style = if on {
                theme.focus()
            } else {
                theme.label(false)
            };
            spans.push(Span::styled(
                if on {
                    format!("{} ", severity.tag().trim())
                } else {
                    format!("({}) ", severity.tag().trim())
                },
                style,
            ));
        }
        if !self.app.query.is_empty() {
            spans.push(Span::styled(
                format!("· \"{}\"", truncate_to_cells(&self.app.query, 24)),
                theme.machine(),
            ));
        }
        Line::from(spans)
    }

    /// The message for a list with nothing in it.
    ///
    /// Both are under forty cells so neither is truncated in the list pane at eighty
    /// columns: an empty state clipped to "no report loaded — press s to sca" tells
    /// the reader nothing they can act on.
    fn empty_message(&self) -> &'static str {
        if self.app.rows.is_empty() {
            "no report — press s to scan"
        } else {
            "nothing matches — press c to clear"
        }
    }

    fn list_border(&self) -> Block<'static> {
        self.border(" FINDINGS ", self.app.focus == Focus::Primary)
    }

    fn detail_border(&self) -> Block<'static> {
        self.border(" DETAIL ", self.app.focus == Focus::Detail)
    }

    fn border(&self, title: &'static str, focused: bool) -> Block<'static> {
        Block::bordered()
            .border_style(if focused {
                self.app.theme.border_focused()
            } else {
                self.app.theme.border()
            })
            .title(Span::styled(title, self.app.theme.label(false)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{Action, Screen};
    use crate::ui::tests_common::{app, app_with_findings, row_with, text_rows};

    /// The whole screen, list and detail, as one string.
    fn painted(app: &App, width: u16, height: u16) -> String {
        let area = Rect::new(0, 0, width, height);
        let mut buffer = Buffer::empty(area);
        ratatui::widgets::Widget::render(widget(app), area, &mut buffer);
        text_rows(&buffer).join("\n")
    }

    #[test]
    fn the_detail_pane_shows_the_evidence_and_remediation_of_the_selected_finding() {
        // FR-9.
        let mut app = app_with_findings(4);
        app.action(Action::Next);
        let text = painted(&app, 200, 20);
        assert!(text.contains("EVIDENCE"), "got {text}");
        assert!(text.contains("REMEDIATION"), "got {text}");
        assert!(text.contains("evidence"), "got {text}");
        assert!(text.contains("remediate"), "got {text}");
    }

    #[test]
    fn a_filtered_report_says_on_this_screen_which_severities_it_is_hiding() {
        // FR-10. The status bar carries the counts; which severities are hidden is
        // specific to this screen, so it is said here too.
        let mut app = app_with_findings(9);
        app.filter.high = false;
        app.refilter();
        assert!(painted(&app, 200, 20).contains("(HIGH)"));
    }

    #[test]
    fn a_report_with_no_findings_prints_an_empty_state_rather_than_an_empty_panel() {
        let mut app = app();
        app.screen = Screen::Report;
        assert!(painted(&app, 80, 20).contains("no report — press s to scan"));
    }

    #[test]
    fn a_filter_that_hides_everything_says_how_to_undo_it() {
        let mut app = app_with_findings(3);
        app.query = "nothing matches this".to_string();
        app.refilter();
        assert!(painted(&app, 80, 20).contains("press c to clear"));
    }

    #[test]
    fn a_finding_with_no_evidence_says_so_rather_than_looking_broken() {
        // A blank section and a missing finding look the same; they are not.
        let mut app = app();
        app.rows = vec![row_with(Sev::High, "a title", "autoruns", &[], &[])];
        app.visible = vec![0];
        let text = painted(&app, 200, 20);
        assert!(text.contains("no EVIDENCE"), "got {text}");
        assert!(text.contains("no REMEDIATION"), "got {text}");
    }

    #[test]
    fn four_hundred_findings_fill_the_list_and_the_frame_does_not_grow() {
        let app = app_with_findings(400);
        let mut buffer = Buffer::empty(Rect::new(0, 0, 80, 20));
        let area = Rect::new(0, 0, 80, 20);
        ratatui::widgets::Widget::render(widget(&app), area, &mut buffer);
        assert_eq!(text_rows(&buffer).len(), 20);
    }

    #[test]
    fn the_engines_own_verdict_is_on_screen_because_it_says_what_the_counts_cannot() {
        // FR-8, and a requirement that had been implemented everywhere except here:
        // the verdict was decoded, sanitised, stored, cloned into the archive and
        // never drawn. "требуется внимание" next to seven findings is a conclusion,
        // where a severity count is a tally - and it is the one line on this screen
        // the engine wrote rather than this front end.
        let mut app = app_with_findings(7);
        app.report
            .as_mut()
            .expect("a report is loaded")
            .verdict
            .headline = "требуется внимание".to_string();
        let text = painted(&app, 200, 20);
        assert!(
            text.contains("verdict:"),
            "the verdict is not on screen: {text}"
        );
        assert!(
            text.contains("требуется внимание"),
            "the engine's own words are missing: {text}"
        );
    }

    #[test]
    fn a_report_with_no_verdict_gets_no_verdict_line_rather_than_an_empty_one() {
        // A report from before the verdict existed. An empty labelled row would be a
        // claim that the engine said nothing, which is different from it not having
        // said anything.
        let mut app = app_with_findings(3);
        app.report
            .as_mut()
            .expect("a report is loaded")
            .verdict
            .headline = String::new();
        let text = painted(&app, 200, 20);
        assert!(
            !text.contains("verdict:"),
            "an empty verdict row was drawn: {text}"
        );
    }
}
