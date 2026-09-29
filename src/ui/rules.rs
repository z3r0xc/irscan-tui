//! The rules screen: the rule set, and the fact that an edit is not yet in effect.
//!
//! FR-17. A staged edit is saved and is *not* in the running scan, because the
//! engine compiles its rules once per scan and exposes no reload. So the screen says
//! plainly that the change applies to the next scan; pretending otherwise would be a
//! lie the operator only discovers after the next scan reproduces the finding.

use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::report::truncate_to_cells;

/// The rule screen.
pub fn widget(app: &App) -> Paragraph<'static> {
    let theme = &app.theme;
    let mut lines: Vec<Line> = vec![
        Line::from(Span::styled(
            "rules are compiled once per scan; nothing here changes a scan in flight",
            theme.label(false),
        )),
        Line::default(),
    ];

    match app.staged_rules.as_ref() {
        Some(edit) => lines.push(Line::from(vec![
            Span::styled("staged: ", theme.staged()),
            Span::styled(truncate_to_cells(edit, 48), theme.machine()),
            Span::styled("  — applies to the NEXT scan, not this one", theme.staged()),
        ])),
        None => lines.push(Line::from(Span::styled(
            "no staged edit — press e to edit a rule file",
            theme.body(),
        ))),
    }

    Paragraph::new(lines).block(
        Block::bordered()
            .border_style(theme.border_focused())
            .title(Span::styled(" RULES ", theme.label(false))),
    )
}

/// Draw the rules screen into `area`.
pub fn render(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    frame.render_widget(widget(app), area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::tests_common::{app, render, text_rows};

    #[test]
    fn with_nothing_staged_the_rules_screen_does_not_claim_a_change_is_pending() {
        let app = app();
        let text = text_rows(&render(widget(&app), 80, 12)).join("\n");
        assert!(!text.contains("applies to the NEXT scan"), "got {text}");
        assert!(text.contains("no staged edit"), "got {text}");
    }

    #[test]
    fn a_staged_rule_edit_says_plainly_that_it_applies_to_the_next_scan() {
        // FR-17. The engine has no reload, so this sentence is the whole contract.
        let mut app = app();
        app.staged_rules = Some("rules/extra.toml".to_string());
        let text = text_rows(&render(widget(&app), 80, 12)).join("\n");
        assert!(text.contains("rules/extra.toml"), "got {text}");
        assert!(text.contains("applies to the NEXT scan"), "got {text}");
    }
}
