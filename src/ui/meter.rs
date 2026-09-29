//! The progress bar, drawn as filled and empty cells.
//!
//! `motion::Meter` owns how full it is; this module only decides what a cell looks
//! like. The degradation rule in `docs/design.md` section 6.1 lives here and only
//! here: at `Depth::None` the two halves are told apart by the *character* drawn in
//! them, because a filled background and an empty one are the same colour in a
//! monochrome terminal.

use ratatui::text::{Line, Span};

use crate::app::App;
use crate::theme::Depth;

/// A `width`-cell bar showing `app.meter`.
pub fn bar(app: &App, width: u16) -> Line<'static> {
    let filled = app.meter.filled_cells(width);
    // The track glyph is the only difference between the two halves at `Depth::None`:
    // a filled background and an empty one are the same colour in a monochrome
    // terminal, so the character is what carries the distinction (section 6.1).
    let track_glyph = if app.theme.depth() == Depth::None {
        "░"
    } else {
        "·"
    };
    Line::from(vec![
        Span::styled("█".repeat(usize::from(filled)), app.theme.meter_fill()),
        Span::styled(
            track_glyph.repeat(usize::from(width - filled)),
            app.theme.meter_track(),
        ),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;
    use crate::ui::tests_common::app;

    #[test]
    fn a_half_full_bar_is_half_filled_and_is_exactly_the_width_asked_for() {
        let mut app = app();
        app.meter.set(0.5);
        app.meter.advance(std::time::Duration::from_secs(1));
        let line = bar(&app, 20);
        let cells: String = line.spans.iter().map(|s| s.content.to_string()).collect();
        assert_eq!(cells.chars().count(), 20);
        assert_eq!(cells.chars().filter(|ch| *ch == '█').count(), 10);
    }

    #[test]
    fn a_monochrome_terminal_tells_the_two_halves_apart_by_glyph() {
        // Section 6.1. Without this the bar is a row of identical cells and the
        // progress it exists to show is invisible.
        let mut app = app();
        app.theme = Theme::new(Depth::None);
        app.meter.set(0.5);
        app.meter.advance(std::time::Duration::from_secs(1));
        let cells: String = bar(&app, 10)
            .spans
            .iter()
            .map(|s| s.content.to_string())
            .collect();
        assert!(cells.contains('█') && cells.contains('░'), "got {cells:?}");
    }
}
