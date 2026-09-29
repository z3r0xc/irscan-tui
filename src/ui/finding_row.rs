//! One finding, as one line.
//!
//! The row is the densest thing in the interface and it is where `docs/design.md`
//! section 4.3 lives: `glyph · weight · tag · title · … · evidence count ·
//! category`, tag column fixed at four cells so the titles align down the screen.
//!
//! Two rules are enforced here rather than hoped for.
//!
//! * **A row is one row.** The title is truncated to whatever cells are left by the
//!   chip and the two right-hand columns. A row that overflows wraps, and the
//!   wrapped remainder lands on the *next* row and destroys the grid the whole
//!   layout exists to support - which is why the budget is computed in cells, not
//!   characters (`docs/spec.md` SR-2).
//! * **Glyph and tag come before colour.** Severity is carried by the glyph and the
//!   tag and the weight, and colour is the last of the four signals, so the row is
//!   still readable on a monochrome terminal.

use ratatui::text::{Line, Span};

use crate::app::{App, Row};
use crate::report::truncate_to_cells;
use unicode_width::UnicodeWidthStr;

/// The fixed width of the tag column, in cells.
pub const TAG_CELLS: usize = 4;

/// One finding rendered as one line, truncated to `width`.
pub fn row(app: &App, index: usize, width: u16) -> Line<'static> {
    let Some(finding) = index_of(app, index) else {
        return Line::default();
    };
    let theme = &app.theme;
    let severity = finding.severity;
    let text_style = if severity.emphatic() {
        theme.display()
    } else {
        theme.body()
    };

    // `Sev::tag` is already four cells wide, with a trailing space on the shorter
    // three so the column holds. The separator appended here is what keeps the
    // title from running straight into `HIGH`.
    let tag = format!("{} ", severity.tag());
    let evidence = finding.evidence.len();
    let category = truncate_to_cells(&finding.category, 20);
    // Every fixed cell is subtracted before the title is truncated, which is what
    // makes the row one row: glyph, its space, the four-cell tag, the separator
    // after it, then the two-cell evidence count, two of padding, the category and
    // one closing gap.
    let evidence_cells = 2;
    let right = evidence_cells + 2 + category.width() + 1;
    let chip = 1 + 1 + TAG_CELLS + 1;
    let title_cells = (usize::from(width)).saturating_sub(chip + right);

    Line::from(vec![
        Span::styled(format!("{} ", severity.glyph()), text_style),
        Span::styled(tag, text_style),
        Span::styled(
            format!("{} ", truncate_to_cells(&finding.title, title_cells)),
            text_style,
        ),
        Span::styled(format!("{evidence:>2}"), theme.label(false)),
        Span::raw("  "),
        Span::styled(category, theme.machine()),
    ])
}

/// The visible row at `index`, or `None` past the end of the list.
fn index_of(app: &App, index: usize) -> Option<&Row> {
    app.visible.get(index).and_then(|i| app.rows.get(*i))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{Sev, Theme};
    use crate::ui::tests_common::{app, render, row as make_row, text_rows};
    use ratatui::widgets::Paragraph;
    use unicode_width::UnicodeWidthStr;

    /// An app holding one finding, so a row can be rendered on its own.
    fn one(row: crate::app::Row) -> App {
        let mut app = app();
        app.rows = vec![row];
        app.visible = vec![0];
        app
    }

    #[test]
    fn a_finding_row_carries_its_glyph_and_tag_before_it_carries_colour() {
        // Section 1 of the design system. A monochrome terminal gets the glyph and
        // the tag and the weight; colour is only ever the fourth signal.
        let app = one(make_row(
            Sev::High,
            "task scheduler created a persistent task",
        ));
        let line = row(&app, 0, 80);
        let glyph = Sev::High.glyph();
        assert!(
            line.spans[0].content.contains(glyph),
            "the glyph should lead the row, got {:?}",
            line.spans[0].content
        );
        let tag = line.spans[1].content.to_string();
        // The tag span is the four-cell column plus the separator that keeps the
        // title from running into it. Without the separator the row reads
        // `! HIGHSome scheduled task`, which is the sort of thing a test written
        // against the span rather than the rendered buffer does not catch.
        assert_eq!(tag, "HIGH ", "the tag column is fixed width");
        assert_eq!(
            tag.trim_end().width(),
            TAG_CELLS,
            "the tag itself is four cells"
        );
        // And both are on the buffer as styled cells, not merely as text.
        let buffer = render(Paragraph::new(line.clone()), 80, 1);
        assert_eq!(buffer[(0, 0)].symbol(), glyph);
        assert_eq!(
            buffer[(0, 0)].style().add_modifier,
            app.theme.display().add_modifier
        );
        // The tag occupies the four cells right after the glyph, which is what makes
        // the titles below it align down the column.
        let tag_cells: String = (0..TAG_CELLS)
            .map(|offset| buffer[(2 + offset as u16, 0)].symbol())
            .collect();
        assert_eq!(tag_cells, "HIGH", "the tag should follow the chip");
    }

    #[test]
    fn a_finding_row_that_is_not_high_is_styled_as_body() {
        // Bolding every severity would make bold mean "a severity" instead of "the
        // worst severity".
        let app = one(make_row(Sev::Low, "a preference key"));
        assert_eq!(row(&app, 0, 80).spans[0].style, app.theme.body());
    }

    #[test]
    fn a_long_path_is_elided_at_the_head_and_never_wraps() {
        // SR-2 and section 4.3. A row that wraps does not take the next row with it
        // by accident - it takes it by overflowing, which is worse.
        let long = format!(
            "C:\\Windows\\System32\\drivers\\{}\\payload.dll",
            "x".repeat(180)
        );
        let mut app = one(make_row(Sev::Medium, &long));
        app.rows[0].title = long.clone();
        for width in [40u16, 80, 200] {
            let line = row(&app, 0, width);
            let text: String = line
                .spans
                .iter()
                .map(|s| s.content.to_string())
                .collect::<String>();
            assert!(
                text.width() <= usize::from(width),
                "at {width} the row is {} cells wide: {text:?}",
                text.width()
            );
            let buffer = render(Paragraph::new(line), width, 1);
            let drawn = text_rows(&buffer);
            assert!(
                drawn[0].ends_with("autoruns"),
                "at {width} the category column should still be readable: {:?}",
                drawn[0]
            );
        }
    }

    #[test]
    fn a_title_too_long_for_the_column_loses_its_tail_with_an_ellipsis() {
        let app = one(make_row(Sev::Info, &"a".repeat(400)));
        let line = row(&app, 0, 60);
        let text: String = line
            .spans
            .iter()
            .map(|s| s.content.to_string())
            .collect::<String>();
        assert!(text.contains('\u{2026}'), "got {text:?}");
    }

    #[test]
    fn a_theme_with_no_colour_still_prints_the_glyph_and_the_tag() {
        let mut app = one(make_row(Sev::High, "anything"));
        app.theme = Theme::new(crate::theme::Depth::None);
        let buffer = render(Paragraph::new(row(&app, 0, 80)), 80, 1);
        let text = text_rows(&buffer)[0].clone();
        assert!(text.trim_start().starts_with(Sev::High.glyph()));
        assert!(text.contains("HIGH"), "got {text:?}");
    }
}
