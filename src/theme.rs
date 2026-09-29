//! The visual system's tokens, and the degradation rules for a terminal that
//! cannot show all of them.
//!
//! Two decisions here are load-bearing.
//!
//! **Detection is a pure function of the environment.** `detect` takes an `Env` and
//! returns a `Depth`; it never reads `std::env` itself. That is what lets the
//! degradation rules below be tested at all, since a test cannot set the terminal's
//! capabilities and there is no library call that reports them.
//!
//! **There is no per-frame quantisation.** Ratatui 0.30 has no colour-depth API: no
//! `ColorDepth`, no `with_color`, and no RGB-to-256 conversion - the backend maps a
//! `Color` to crossterm one to one. So the whole `Theme` is built once, at startup,
//! for the depth that was detected, and held. A `Color` is four bytes and a `Style`
//! is five of them, so re-deriving a theme per frame is work for nothing. No render
//! code anywhere in this crate branches on depth, and a test says so.
//!
//! The colour choices are specified in `docs/design.md` section 2, and the one rule
//! they all follow is in section 1: severity is carried by glyph, weight and tag,
//! never by hue.

use ratatui::style::{Color, Modifier, Style};

/// How much colour the terminal can show.
///
/// Ordered from least to most capable so a comparison such as
/// `depth.supports_colour()` can be written without a match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Depth {
    /// No colour at all. Every distinction must survive on glyph, weight and text.
    None,
    /// The classic sixteen ANSI colours.
    Ansi16,
    /// The 256-entry xterm palette.
    Ansi256,
    /// Twenty-four bit colour.
    TrueColor,
}

impl Depth {
    /// Whether this depth emits any colour at all.
    pub const fn supports_colour(self) -> bool {
        !matches!(self, Depth::None)
    }

    /// Pick a token for this depth out of the palettes it is declared in.
    ///
    /// At `None` the answer is `Reset`, which crossterm means as "use the terminal's
    /// own default". Falling back to the 16-colour entry instead would emit
    /// `Color::Black`, and black-on-black is the one outcome a monochrome terminal
    /// cannot show: the text is there and cannot be read. `Reset` is the only value
    /// that lets the terminal pick something legible on the user's behalf.
    const fn pick(self, truecolor: Color, indexed: Color, ansi16: Color) -> Color {
        match self {
            Depth::TrueColor => truecolor,
            Depth::Ansi256 => indexed,
            Depth::Ansi16 => ansi16,
            Depth::None => Color::Reset,
        }
    }
}

/// The part of the environment the detection reads.
///
/// A struct rather than three `Option<String>` so that a test states its whole input
/// at once, and a new signal cannot be added without a test naming the case it
/// changes.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Env {
    /// `COLORTERM`. Set to `truecolor` or `24bit` by every terminal that means it.
    pub colorterm: Option<String>,
    /// `TERM`.
    pub term: Option<String>,
    /// Whether `NO_COLOR` is present, by the no-colors.org convention: presence
    /// alone, whatever its value, including the empty string.
    pub no_color: bool,
}

impl Env {
    /// Read the real environment.
    pub fn from_process() -> Self {
        Env {
            colorterm: std::env::var("COLORTERM").ok(),
            term: std::env::var("TERM").ok(),
            no_color: std::env::var_os("NO_COLOR").is_some(),
        }
    }
}

/// Decide what the terminal can show.
///
/// `NO_COLOR` is checked first and unconditionally. It is a convention that says
/// "this program should not emit colour", not a hint about the terminal's
/// capabilities, and a program that honours it only when the terminal is also low on
/// colour is not honouring it.
pub fn detect(env: &Env) -> Depth {
    if env.no_color {
        return Depth::None;
    }
    if matches!(env.colorterm.as_deref(), Some("truecolor") | Some("24bit")) {
        return Depth::TrueColor;
    }
    match env.term.as_deref() {
        // `dumb` is the terminfo capability name for a terminal that cannot do
        // cursor addressing or colour at all.
        Some("dumb") => Depth::None,
        Some(term) if term.contains("direct") || term.contains("truecolor") => Depth::TrueColor,
        Some(term) if term.contains("256color") => Depth::Ansi256,
        // An unset TERM is the Windows console in the common case, whose palette is
        // sixteen colours. Guessing truecolour there is how a program ends up
        // emitting escape sequences a terminal renders as garbage.
        _ => Depth::Ansi16,
    }
}

/// The tokens, resolved for one depth.
///
/// Values are the ones in `docs/design.md` section 2.1 to 2.4. Every colour is stored
/// in the form this `Depth` can actually display, so a render site never has to
/// think about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    depth: Depth,
    /// The content area.
    pub bg: Color,
    /// Chrome: the header and the key hint, and nothing else.
    pub bg_raised: Color,
    /// Marks an object - a finding, a report, an archive entry.
    pub panel: Color,
    /// The hairline that separates two regions.
    pub line: Color,
    /// A rule inside a panel.
    pub line_strong: Color,
    /// The value being read.
    pub ink: Color,
    /// Body text.
    pub ink_2: Color,
    /// Supporting text, and every machine string.
    pub ink_3: Color,
    /// Labels, column headers, key hints.
    pub ink_4: Color,
    /// A disabled row or a placeholder.
    pub ink_5: Color,
    /// A filled meter track.
    pub ink_void: Color,
    /// Focus, and the active panel's border.
    pub accent: Color,
    /// A failure, a refusal, an unsaved change.
    pub damage: Color,
    /// An edit that applies to the next scan.
    pub staged: Color,
    /// A completed check, a resolved finding.
    pub good: Color,
}

impl Theme {
    /// Build the theme for a depth.
    pub const fn new(depth: Depth) -> Self {
        Theme {
            depth,
            bg: depth.pick(rgb(0x0a, 0x0a, 0x0a), Color::Indexed(232), Color::Black),
            bg_raised: depth.pick(rgb(0x10, 0x10, 0x10), Color::Indexed(234), Color::Black),
            panel: depth.pick(rgb(0x14, 0x14, 0x14), Color::Indexed(236), Color::Black),
            line: depth.pick(rgb(0x23, 0x23, 0x23), Color::Indexed(238), Color::DarkGray),
            line_strong: depth.pick(rgb(0x2e, 0x2e, 0x2e), Color::Indexed(240), Color::DarkGray),
            ink: depth.pick(rgb(0xf6, 0xf6, 0xf6), Color::Indexed(255), Color::White),
            ink_2: depth.pick(rgb(0xbd, 0xbd, 0xbd), Color::Indexed(251), Color::Gray),
            ink_3: depth.pick(rgb(0x8a, 0x8a, 0x8a), Color::Indexed(245), Color::Gray),
            ink_4: depth.pick(rgb(0x56, 0x56, 0x56), Color::Indexed(240), Color::DarkGray),
            ink_5: depth.pick(rgb(0x33, 0x33, 0x33), Color::Indexed(237), Color::DarkGray),
            ink_void: depth.pick(rgb(0x1c, 0x1c, 0x1c), Color::Indexed(235), Color::Black),
            accent: depth.pick(rgb(0x5a, 0xc8, 0xfa), Color::Indexed(45), Color::Cyan),
            damage: depth.pick(rgb(0xd7, 0x5f, 0x5f), Color::Indexed(167), Color::Red),
            staged: depth.pick(rgb(0xd7, 0xaf, 0x5f), Color::Indexed(179), Color::Yellow),
            good: depth.pick(rgb(0x87, 0xaf, 0x5f), Color::Indexed(108), Color::Green),
        }
    }

    /// Build from the real environment.
    pub fn from_env() -> Self {
        Self::new(detect(&Env::from_process()))
    }

    /// The depth this theme was built for.
    pub const fn depth(&self) -> Depth {
        self.depth
    }
    /// Paint a foreground, or paint nothing at all at `Depth::None`.
    ///
    /// This is the single gate every foreground style goes through, and it exists
    /// because the two failure modes look identical in the source and are not the
    /// same in the terminal:
    ///
    /// * `Style::new().fg(Color::Reset)` emits an explicit "reset to default"
    ///   sequence. That is a colour *request*, and a terminal is entitled to answer
    ///   it.
    /// * `Style::new()` emits nothing and leaves the terminal alone.
    ///
    /// A theme whose tokens collapse to `Reset` at no colour therefore cannot simply
    /// hand those tokens to `fg()`; it has to decline to name a colour at all.
    const fn paint(&self, colour: Color) -> Style {
        match self.depth {
            Depth::None => Style::new(),
            _ => Style::new().fg(colour),
        }
    }

    /// The one number the user came to read: a verdict, a count, a host name.
    pub fn display(&self) -> Style {
        self.paint(self.ink).add_modifier(Modifier::BOLD)
    }

    /// Anything read a word at a time: evidence, remediation, a log line.
    pub fn body(&self) -> Style {
        self.paint(self.ink_2)
    }

    /// A column header, a key hint, a unit, a count of something.
    ///
    /// At sixteen colours `DIM` is replaced by `BOLD` on a darker step: many
    /// terminals render `DIM` as near-invisible, and a label step that cannot be seen
    /// is a step that does not exist. This is the one place the typography rule in
    /// `docs/design.md` section 3 is deliberately broken, and it is why the label
    /// takes its colour from a parameter rather than always using `ink_4`.
    pub fn label(&self, on_dark: bool) -> Style {
        match self.depth {
            Depth::None => Style::new(),
            Depth::Ansi16 => {
                // On a raised surface the label needs a brighter step as well as the
                // weight: `ink_4` on `bg_raised` is the dimmest pairing in the whole
                // system, and bolding the dimmest ink is how a label disappears.
                let step = if on_dark { self.ink_3 } else { self.ink_4 };
                Style::new().fg(step).add_modifier(Modifier::BOLD)
            }
            _ => Style::new().fg(self.ink_4).add_modifier(Modifier::DIM),
        }
    }

    /// A path, a registry key, a command line: the kind of string that must never
    /// wrap, styled so the eye finds it.
    pub fn machine(&self) -> Style {
        self.paint(self.ink_3)
    }

    /// The focused element.
    pub fn focus(&self) -> Style {
        match self.depth {
            Depth::None => Style::new().add_modifier(Modifier::REVERSED),
            _ => Style::new().fg(self.accent).add_modifier(Modifier::BOLD),
        }
    }

    /// A failure, a refusal, a collector that could not run.
    pub fn damage(&self) -> Style {
        match self.depth {
            Depth::None => Style::new().add_modifier(Modifier::BOLD),
            _ => Style::new().fg(self.damage),
        }
    }

    /// An edit that is saved but not yet in effect.
    pub fn staged(&self) -> Style {
        match self.depth {
            Depth::None => Style::new().add_modifier(Modifier::ITALIC),
            _ => Style::new().fg(self.staged),
        }
    }

    /// A completed check.
    pub fn good(&self) -> Style {
        self.paint(self.good)
    }

    /// The unfilled part of a meter.
    ///
    /// At `Depth::None` this is the terminal's default background. An "empty" cell
    /// cannot be made to look empty in a monochrome terminal, so the meter's filled
    /// and unfilled parts are told apart by weight and by the character drawn in
    /// them instead, and this is only there so the track is not an accident.
    pub const fn meter_track(&self) -> Style {
        match self.depth {
            Depth::None => Style::new(),
            _ => Style::new().bg(self.ink_void),
        }
    }

    /// The filled part of a meter.
    pub fn meter_fill(&self) -> Style {
        match self.depth {
            // No background to fill and no default foreground to name, so the filled
            // part is carried by weight alone - which is the only signal a
            // monochrome terminal still has.
            Depth::None => Style::new().add_modifier(Modifier::BOLD),
            _ => Style::new().fg(self.accent).bg(self.ink_void),
        }
    }

    /// The border of a region that is not focused.
    ///
    /// No border is drawn at `Depth::None`: a hairline in the default foreground is
    /// the same brightness as the text inside the region, so the rule would compete
    /// with the content instead of separating it. `docs/design.md` section 2.2 says
    /// regions are separated by hairlines, and a rule that cannot be told from the
    /// text is not separating anything.
    pub const fn border(&self) -> Style {
        match self.depth {
            Depth::None => Style::new(),
            _ => Style::new().fg(self.line),
        }
    }

    /// The border of the focused region.
    ///
    /// Reverse video rather than a colour, so focus survives a monochrome terminal
    /// (`docs/design.md` section 4.1).
    pub fn border_focused(&self) -> Style {
        match self.depth {
            Depth::None => Style::new().add_modifier(Modifier::REVERSED),
            _ => Style::new().fg(self.accent),
        }
    }
}

/// A severity, as the interface shows it.
///
/// This is a copy of the engine's four values rather than a use of the engine's
/// `Severity` type, because the interface must render a report on a machine with no
/// engine present. The mapping is pinned by
/// `every_severity_the_engine_emits_has_a_chip_here`, so the copy cannot drift.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Sev {
    Info,
    Low,
    Medium,
    High,
}

impl Sev {
    /// Every severity, least to most severe.
    pub const ALL: [Sev; 4] = [Sev::Info, Sev::Low, Sev::Medium, Sev::High];

    /// The glyph. The first of the three non-colour signals.
    pub const fn glyph(self) -> &'static str {
        match self {
            Sev::Info => "\u{b7}",
            Sev::Low => ".",
            Sev::Medium => "~",
            Sev::High => "!",
        }
    }

    /// The tag, fixed width so that titles align down the column.
    pub const fn tag(self) -> &'static str {
        match self {
            Sev::Info => "INFO",
            Sev::Low => "LOW ",
            Sev::Medium => "MED ",
            Sev::High => "HIGH",
        }
    }

    /// The weight. The second non-colour signal.
    ///
    /// `High` is the only one that gets emphasis; bolding all four would make bold
    /// mean "a severity" instead of "the worst severity".
    pub const fn emphatic(self) -> bool {
        matches!(self, Sev::High)
    }

    /// Parse a severity out of the strings the engine's JSON report uses.
    ///
    /// The engine writes `Severity::label()` lower-case into JSON and
    /// `Severity::tag()` upper-case into text, and both spellings occur in reports
    /// people actually have, so both are accepted. Anything unrecognised is
    /// `Info`: an unknown severity from a newer engine must be shown, not dropped.
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "high" | "critical" => Sev::High,
            "medium" | "med" | "moderate" => Sev::Medium,
            "low" => Sev::Low,
            _ => Sev::Info,
        }
    }
}

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::Rgb(r, g, b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn env(colorterm: Option<&str>, term: Option<&str>) -> Env {
        Env {
            colorterm: colorterm.map(str::to_string),
            term: term.map(str::to_string),
            no_color: false,
        }
    }

    /// Every token of the theme, by name, so the matrix test cannot silently skip
    /// one that was added later.
    fn tokens(t: &Theme) -> Vec<(&'static str, Color)> {
        vec![
            ("bg", t.bg),
            ("bg_raised", t.bg_raised),
            ("panel", t.panel),
            ("line", t.line),
            ("line_strong", t.line_strong),
            ("ink", t.ink),
            ("ink_2", t.ink_2),
            ("ink_3", t.ink_3),
            ("ink_4", t.ink_4),
            ("ink_5", t.ink_5),
            ("ink_void", t.ink_void),
            ("accent", t.accent),
            ("damage", t.damage),
            ("staged", t.staged),
            ("good", t.good),
        ]
    }

    /// A token is present at this depth if it is one of the two forms this depth can
    /// actually display. `Color::Reset` is not a colour and counts as absent.
    fn present_for(depth: Depth, c: Color) -> bool {
        match depth {
            Depth::TrueColor => matches!(c, Color::Rgb(..)),
            Depth::Ansi256 => matches!(c, Color::Indexed(_)),
            Depth::Ansi16 => matches!(
                c,
                Color::Black
                    | Color::Red
                    | Color::Green
                    | Color::Yellow
                    | Color::Blue
                    | Color::Magenta
                    | Color::Cyan
                    | Color::Gray
                    | Color::DarkGray
                    | Color::LightRed
                    | Color::LightGreen
                    | Color::LightYellow
                    | Color::LightBlue
                    | Color::LightMagenta
                    | Color::LightCyan
                    | Color::White
            ),
            Depth::None => false,
        }
    }

    #[test]
    fn no_colour_is_detected_from_the_convention_and_wins_over_every_other_signal() {
        // A terminal that says it is truecolour, and a program that says do not use
        // colour, together. The convention has to win, or `NO_COLOR` is advisory.
        let loud = Env {
            colorterm: Some("truecolor".to_string()),
            term: Some("xterm-256color".to_string()),
            no_color: true,
        };
        assert_eq!(detect(&loud), Depth::None);
    }

    #[test]
    fn truecolour_is_detected_from_either_of_the_two_signals_that_mean_it() {
        // COLORTERM is what Windows Terminal, kitty, WezTerm and VTE set.
        assert_eq!(
            detect(&env(Some("truecolor"), Some("xterm-256color"))),
            Depth::TrueColor
        );
        assert_eq!(detect(&env(Some("24bit"), None)), Depth::TrueColor);
        // `direct` is the ncurses/terminfo capability name for Tc/RGB.
        assert_eq!(detect(&env(None, Some("xterm-direct"))), Depth::TrueColor);
    }

    #[test]
    fn two_hundred_and_fifty_six_colours_is_distinguished_from_sixteen() {
        // These two need different tokens, so conflating them is a visible bug: the
        // truecolour hexes would be emitted to a terminal that cannot show them.
        assert_eq!(detect(&env(None, Some("xterm-256color"))), Depth::Ansi256);
        assert_eq!(detect(&env(None, Some("xterm"))), Depth::Ansi16);
    }

    #[test]
    fn a_dumb_terminal_gets_no_colour_and_an_unset_term_gets_sixteen() {
        assert_eq!(detect(&env(None, Some("dumb"))), Depth::None);
        // The Windows console usually has no TERM at all, and its palette is sixteen
        // colours. Guessing truecolour there emits sequences it renders as garbage.
        assert_eq!(detect(&env(None, None)), Depth::Ansi16);
    }

    #[test]
    fn every_token_resolves_to_something_this_depth_can_display() {
        for depth in [Depth::None, Depth::Ansi16, Depth::Ansi256, Depth::TrueColor] {
            let theme = Theme::new(depth);
            for (name, colour) in tokens(&theme) {
                if depth == Depth::None {
                    // At no colour the tokens collapse onto the terminal's own
                    // default, which is the only thing there is to collapse onto.
                    assert_eq!(colour, Color::Reset, "{name} emits colour at Depth::None");
                } else {
                    assert!(
                        present_for(depth, colour),
                        "{name} is {colour:?}, which {depth:?} cannot display"
                    );
                }
            }
        }
    }

    #[test]
    fn no_colour_means_no_colour_is_emitted_by_any_style_the_theme_produces() {
        // This is the property that makes a monochrome terminal legible instead of
        // black-on-black: not "fewer colours" but *none*, and every distinction
        // carried by glyph, weight and text instead.
        let theme = Theme::new(Depth::None);
        let styles = [
            theme.display(),
            theme.body(),
            theme.label(false),
            theme.label(true),
            theme.machine(),
            theme.focus(),
            theme.damage(),
            theme.staged(),
            theme.good(),
            theme.meter_track(),
            theme.meter_fill(),
            theme.border(),
            theme.border_focused(),
        ];
        for style in styles {
            assert!(
                style.fg.is_none(),
                "a style emitted a foreground at Depth::None"
            );
            assert!(
                style.bg.is_none(),
                "a style emitted a background at Depth::None"
            );
        }
    }

    #[test]
    fn at_sixteen_colours_the_label_step_is_bold_rather_than_dim() {
        // Many terminals render DIM as near-invisible, so a DIM label on a dark
        // surface is a label that cannot be read. This is the one place the
        // typography rule in docs/design.md section 3 is deliberately broken.
        let theme = Theme::new(Depth::Ansi16);
        let on_dark = theme.label(true);
        assert!(!on_dark.add_modifier.contains(Modifier::DIM));
        assert!(on_dark.add_modifier.contains(Modifier::BOLD));
        // The label must land on a step of the ramp that is visible against its own
        // background, not on `ink_4`, which is the dimmest ink there is.
        assert_ne!(on_dark.fg, Some(theme.ink_4));
    }

    #[test]
    fn at_full_colour_the_label_step_is_dim_on_ink_four() {
        let theme = Theme::new(Depth::TrueColor);
        let label = theme.label(false);
        assert!(label.add_modifier.contains(Modifier::DIM));
        assert_eq!(label.fg, Some(theme.ink_4));
    }

    #[test]
    fn every_severity_is_distinguishable_with_no_colour_at_all() {
        // The test of the whole premise in docs/design.md section 1. If the four
        // severities were only told apart by hue, this fails - and on a monochrome
        // terminal the interface would be telling a triage operator nothing.
        let glyphs: BTreeSet<&str> = Sev::ALL.iter().map(|s| s.glyph()).collect();
        let tags: BTreeSet<&str> = Sev::ALL.iter().map(|s| s.tag()).collect();
        assert_eq!(glyphs.len(), 4, "two severities share a glyph");
        assert_eq!(tags.len(), 4, "two severities share a tag");
        // Exactly one severity is emphatic, so bold means "the worst" rather than
        // "a severity".
        assert_eq!(Sev::ALL.iter().filter(|s| s.emphatic()).count(), 1);
        assert!(Sev::High.emphatic());
    }

    #[test]
    fn only_high_is_emphatic_so_bold_keeps_meaning_the_worst_case() {
        assert!(Sev::High.emphatic());
        assert!(!Sev::Medium.emphatic());
        assert!(!Sev::Low.emphatic());
        assert!(!Sev::Info.emphatic());
    }

    #[test]
    fn a_severity_is_parsed_from_either_spelling_the_engine_writes() {
        // `Severity::label()` is lower-case and goes into JSON; `Severity::tag()` is
        // upper-case and goes into text. Both spellings turn up in reports people
        // actually have.
        assert_eq!(Sev::parse("high"), Sev::High);
        assert_eq!(Sev::parse("HIGH"), Sev::High);
        assert_eq!(Sev::parse(" Medium "), Sev::Medium);
        assert_eq!(Sev::parse("med"), Sev::Medium);
        assert_eq!(Sev::parse("low"), Sev::Low);
        assert_eq!(Sev::parse("info"), Sev::Info);
    }

    #[test]
    fn an_unrecognised_severity_is_shown_rather_than_dropped() {
        // A report from a newer engine can carry a severity this build has never
        // heard of. Showing it as INFO keeps it on screen; dropping it would hide a
        // finding, and "we found nothing" and "we could not read it" are the two
        // claims this tool exists to keep apart.
        assert_eq!(Sev::parse("spicy"), Sev::Info);
        assert_eq!(Sev::parse(""), Sev::Info);
    }

    #[test]
    fn severity_tags_are_all_the_same_width_so_titles_align() {
        let widths: BTreeSet<usize> = Sev::ALL.iter().map(|s| s.tag().chars().count()).collect();
        assert_eq!(widths.len(), 1, "the tag column is ragged");
    }

    #[test]
    fn focus_survives_a_monochrome_terminal_by_inverting_rather_than_by_hue() {
        // The design says focus is legible without colour; this is that claim, made
        // executable.
        let theme = Theme::new(Depth::None);
        assert!(theme.focus().add_modifier.contains(Modifier::REVERSED));
        assert!(theme
            .border_focused()
            .add_modifier
            .contains(Modifier::REVERSED));
    }

    #[test]
    fn at_full_colour_every_token_is_the_value_the_design_system_specifies() {
        // The palette is the one in docs/design.md section 2, and a design system
        // whose tokens can change without a failing test is not a design system.
        let t = Theme::new(Depth::TrueColor);
        assert_eq!(t.bg, rgb(0x0a, 0x0a, 0x0a));
        assert_eq!(t.bg_raised, rgb(0x10, 0x10, 0x10));
        assert_eq!(t.panel, rgb(0x14, 0x14, 0x14));
        assert_eq!(t.line, rgb(0x23, 0x23, 0x23));
        assert_eq!(t.line_strong, rgb(0x2e, 0x2e, 0x2e));
        assert_eq!(t.ink, rgb(0xf6, 0xf6, 0xf6));
        assert_eq!(t.ink_2, rgb(0xbd, 0xbd, 0xbd));
        assert_eq!(t.ink_3, rgb(0x8a, 0x8a, 0x8a));
        assert_eq!(t.ink_4, rgb(0x56, 0x56, 0x56));
        assert_eq!(t.ink_5, rgb(0x33, 0x33, 0x33));
        assert_eq!(t.ink_void, rgb(0x1c, 0x1c, 0x1c));
        assert_eq!(t.accent, rgb(0x5a, 0xc8, 0xfa));
        assert_eq!(t.damage, rgb(0xd7, 0x5f, 0x5f));
        assert_eq!(t.staged, rgb(0xd7, 0xaf, 0x5f));
        assert_eq!(t.good, rgb(0x87, 0xaf, 0x5f));
    }

    #[test]
    fn the_ink_ramp_gets_lighter_as_a_thing_gets_more_important() {
        // One direction, no second hue: two adjacent steps are always distinguishable
        // as steps and never as categories (docs/design.md section 2.3).
        let t = Theme::new(Depth::TrueColor);
        let ramp = [t.ink, t.ink_2, t.ink_3, t.ink_4, t.ink_5];
        let luminance = |c: Color| match c {
            Color::Rgb(r, g, b) => {
                0.299 * f32::from(r) + 0.587 * f32::from(g) + 0.114 * f32::from(b)
            }
            other => panic!("ramp token is not truecolour: {other:?}"),
        };
        for pair in ramp.windows(2) {
            assert!(
                luminance(pair[0]) > luminance(pair[1]),
                "the ramp is not monotonic: {pair:?}"
            );
        }
    }

    #[test]
    fn a_panel_is_lighter_than_its_background_so_an_object_is_visible_as_an_object() {
        let t = Theme::new(Depth::TrueColor);
        let luminance = |c: Color| match c {
            Color::Rgb(r, g, b) => {
                0.299 * f32::from(r) + 0.587 * f32::from(g) + 0.114 * f32::from(b)
            }
            other => panic!("token is not truecolour: {other:?}"),
        };
        assert!(luminance(t.panel) > luminance(t.bg));
        assert!(luminance(t.bg_raised) > luminance(t.bg));
    }
}
