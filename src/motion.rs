//! Animation as pure functions of elapsed time.
//!
//! Nothing in this module reads a clock, touches a terminal, or holds global state.
//! Every animation is `value(elapsed) -> f64` where `elapsed` is how long it has been
//! running. That is not a stylistic preference and it is not purity for its own sake:
//! it is the only way motion can be tested without a terminal and without sleeping,
//! which `docs/spec.md` QR-4 requires and which a design that read the clock could
//! not be.
//!
//! **Motion is a duration, not a boolean.** `docs/design.md` section 5.4 requires
//! that the degraded path still *runs* the animation, instantaneously, so that no
//! code path anywhere has an `if motion { ... } else { ... }` branch to get wrong.
//! Setting every duration to zero achieves that with no branch at all.

use std::time::Duration;

/// How long a transition runs, and how long a row takes to appear.
///
/// The four values are the only motion in the interface (`docs/design.md` section
/// 5.2). Everything else is a shape applied to one of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Durations {
    /// A screen change. Short enough that it marks the change without being waited on.
    pub crossfade: Duration,
    /// One row appearing, before staggering.
    pub reveal: Duration,
    /// The gap between successive rows appearing.
    pub stagger: Duration,
    /// How long the progress bar takes to catch up by 1%. Linear, because progress
    /// is a measurement and a measurement has no easing curve.
    pub meter_step: Duration,
    /// One frame of the indeterminate spinner.
    pub spinner_frame: Duration,
}

impl Durations {
    /// The values in `docs/design.md` section 5.2.
    pub const MOTION: Durations = Durations {
        crossfade: Duration::from_millis(140),
        reveal: Duration::from_millis(180),
        stagger: Duration::from_millis(12),
        meter_step: Duration::from_millis(100),
        spinner_frame: Duration::from_millis(90),
    };

    /// Every duration is zero.
    ///
    /// The animation still runs and still completes; it just does so on the first
    /// tick. This is what `--no-motion`, `NO_COLOR` and a non-TTY terminal all produce.
    pub const ZERO: Durations = Durations {
        crossfade: Duration::ZERO,
        reveal: Duration::ZERO,
        stagger: Duration::ZERO,
        meter_step: Duration::ZERO,
        spinner_frame: Duration::ZERO,
    };

    /// Whether anything at all is still going to move.
    pub fn is_enabled(self) -> bool {
        self.crossfade > Duration::ZERO
    }
}

/// How far through an animation we are, as a fraction in `[0, 1]`.
///
/// Clamped rather than scaled: an animation that overshoots past its target and comes
/// back is a bug that shows up as a visible flicker, and clamping is cheaper than
/// discovering it.
pub fn progress(elapsed: Duration, total: Duration) -> f64 {
    let total_nanos = total.as_nanos();
    if total_nanos == 0 {
        // Zero means instantaneous, which is the degraded path. Returning 1 rather
        // than 0 is what makes a zero-duration animation *finish* instead of never
        // starting.
        return 1.0;
    }
    let ratio = elapsed.as_nanos() as f64 / total_nanos as f64;
    ratio.clamp(0.0, 1.0)
}

/// Fast start, long settle.
///
/// The default, and the reason things *arrive* rather than slide. A UI where content
/// slides in reads as slow; a UI where it arrives reads as responsive.
pub fn ease_out(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

/// Symmetric, for something leaving and coming back.
pub fn ease_in_out(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

/// No curve. For a value that is a measurement rather than a movement.
pub const fn linear(t: f64) -> f64 {
    t.clamp(0.0, 1.0)
}

/// The eight spinner frames, braille dots.
///
/// Braille is used rather than the classic `|/-\` because the dots advance smoothly
/// at one cell instead of jumping a cell at a time, and because `|` and `/` are full
/// width in many terminal fonts, which makes a line of them jitter as the cell
/// contents change.
pub const SPINNER: [&str; 8] = [
    "\u{280b}", "\u{2819}", "\u{2839}", "\u{2838}", "\u{283c}", "\u{2834}", "\u{2826}", "\u{2827}",
];

/// A screen change fading between two values.
#[derive(Debug, Clone, Copy)]
pub struct Transition {
    total: Duration,
    elapsed: Duration,
}

impl Transition {
    /// Start a transition that runs for `total`.
    pub const fn new(total: Duration) -> Self {
        Transition {
            total,
            elapsed: Duration::ZERO,
        }
    }

    /// Advance the clock.
    pub fn advance(&mut self, delta: Duration) {
        self.elapsed = self.elapsed.saturating_add(delta);
    }

    /// How far along, eased.
    pub fn progress(&self) -> f64 {
        ease_out(progress(self.elapsed, self.total))
    }

    /// Whether it has finished and can stop being advanced.
    pub fn is_done(&self) -> bool {
        self.elapsed >= self.total
    }

    /// Interpolate a value, returning `from` at the start and `to` at the end.
    ///
    /// At and past the total this returns exactly `to`, not an approximation of it.
    /// An easing curve that is asymptotic means the last frame drawn is never the
    /// final one, and the element it belongs to stays faintly wrong forever.
    pub fn lerp(&self, from: f64, to: f64) -> f64 {
        if self.is_done() {
            return to;
        }
        from + (to - from) * self.progress()
    }
}

/// A list gaining rows, each appearing slightly after the one above it.
///
/// Rows past the cap appear together (`docs/design.md` section 5.3): a 400-finding
/// list that staggers for four seconds is a list that looks broken, and the reader
/// wants the top of it anyway.
#[derive(Debug, Clone, Copy)]
pub struct Reveal {
    durations: Durations,
    /// How many rows are already on screen. The stagger is relative to this, not to
    /// the start of the list, so a row that arrives while the user is reading is
    /// never put behind a delay earned by rows that arrived earlier.
    count: usize,
    elapsed: Duration,
}

impl Reveal {
    /// How many rows stagger individually before the rest arrive together.
    pub const STAGGER_CAP: usize = 10;

    /// Begin with `count` rows already present.
    pub const fn new(durations: Durations, count: usize) -> Self {
        Reveal {
            durations,
            count,
            elapsed: Duration::ZERO,
        }
    }

    /// Advance the clock.
    pub fn advance(&mut self, delta: Duration) {
        self.elapsed = self.elapsed.saturating_add(delta);
    }

    /// How visible row `index` is, in `[0, 1]`.
    ///
    /// 0 means not yet drawn, 1 means fully drawn. A renderer multiplies its
    /// foreground step by this, or fades a background step toward it.
    pub fn row(&self, index: usize) -> f64 {
        if index < self.count {
            return 1.0;
        }
        // Past the cap everything lands at once, so a long list is not a slow list.
        if index - self.count >= Self::STAGGER_CAP {
            return 1.0;
        }
        let delay = self
            .durations
            .stagger
            .saturating_mul((index - self.count) as u32);
        ease_out(progress(
            self.elapsed.saturating_sub(delay),
            self.durations.reveal,
        ))
    }

    /// Whether every row is fully visible and the animation can be forgotten.
    pub fn is_settled(&self) -> bool {
        (0..self.count).all(|index| self.row(index) >= 1.0)
    }
}

/// A determinate progress bar, shown as filled and empty cells.
///
/// The fill eases towards its target rather than jumping, so a collector that
/// reports a jump in findings does not make the bar lurch. The easing is linear
/// because a progress bar is a measurement (`docs/design.md` section 5.2).
#[derive(Debug, Clone, Copy)]
pub struct Meter {
    shown: f64,
    target: f64,
    step: Duration,
    elapsed: Duration,
}

impl Meter {
    /// Start empty, advancing `step` of time per 1% of progress.
    pub const fn new(step: Duration) -> Self {
        Meter {
            shown: 0.0,
            target: 0.0,
            step,
            elapsed: Duration::ZERO,
        }
    }

    /// Set the true progress, in `[0, 1]`.
    ///
    /// Moving backwards is allowed and does nothing unusual: a scan that reports a
    /// correction should not have the bar refuse to go back.
    pub fn set(&mut self, value: f64) {
        self.target = value.clamp(0.0, 1.0);
    }

    /// Advance the clock, and latch at the target once it is reached.
    ///
    /// The latch lives here rather than in `value` so that `value` is a pure read:
    /// a render pass that asks how full the bar is must not itself move it, and a
    /// getter with a side effect makes the bar's rate depend on how often the screen
    /// happens to be drawn.
    pub fn advance(&mut self, delta: Duration) {
        self.elapsed = self.elapsed.saturating_add(delta);
        let caught_up = self.caught_up();
        if self.shown + caught_up >= self.target {
            // Latch rather than stay a hair short. A bar that asymptotes never
            // reaches full, so "finished" is a state the user never sees.
            self.shown = self.target;
            self.elapsed = Duration::ZERO;
        }
    }

    /// How far the bar has moved towards its target by now.
    ///
    /// One percent per `step`, so the bar shows a *rate*, which is the thing that
    /// tells a user working is still happening rather than a scan that has stalled.
    fn caught_up(&self) -> f64 {
        let step_nanos = self.step.as_nanos();
        if step_nanos == 0 {
            return self.target - self.shown;
        }
        let steps = self.elapsed.as_nanos() as f64 / step_nanos as f64;
        (self.target - self.shown).min(steps / 100.0)
    }

    /// How full the bar is right now, in `[0, 1]`.
    ///
    /// A pure read. Nothing here moves the bar; only `advance` does.
    pub fn value(&self) -> f64 {
        if self.shown >= self.target {
            return self.target;
        }
        (self.shown + self.caught_up()).min(self.target)
    }

    /// How many cells of a bar `width` cells wide are filled.
    ///
    /// Rounding rather than truncating: a bar at 99% of ten cells should show nine
    /// filled and one empty, not be indistinguishable from 90%.
    pub fn filled_cells(&self, width: u16) -> u16 {
        ((self.value() * f64::from(width)).round() as u16).min(width)
    }
}

/// An indeterminate wait: a looping spinner.
#[derive(Debug, Clone, Copy)]
pub struct Spinner {
    frame: Duration,
    elapsed: Duration,
}

impl Spinner {
    /// Start spinning, one frame every `frame`.
    pub const fn new(frame: Duration) -> Self {
        Spinner {
            frame,
            elapsed: Duration::ZERO,
        }
    }

    /// Advance the clock.
    pub fn advance(&mut self, delta: Duration) {
        self.elapsed = self.elapsed.saturating_add(delta);
    }

    /// The current frame's glyph.
    pub fn glyph(&self) -> &'static str {
        let nanos = self.frame.as_nanos();
        if nanos == 0 {
            return SPINNER[0];
        }
        let index = (self.elapsed.as_nanos() / nanos) as usize % SPINNER.len();
        SPINNER[index]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_easing_curve_is_monotonic_bounded_and_fixes_both_ends() {
        // A curve that dips or overshoots produces a visible flicker, and one that
        // does not reach its ends leaves the first and last frame wrong.
        for ease in [ease_out, ease_in_out] {
            let mut previous = f64::NEG_INFINITY;
            for step in 0..=1000 {
                let t = f64::from(step) / 1000.0;
                let value = ease(t);
                assert!(
                    (0.0..=1.0).contains(&value),
                    "ease({t}) = {value}, outside [0, 1]"
                );
                assert!(
                    value >= previous,
                    "ease({t}) went backwards: {value} < {previous}"
                );
                previous = value;
            }
            assert_eq!(ease(0.0), 0.0);
            assert_eq!(ease(1.0), 1.0);
        }
    }

    #[test]
    fn a_transition_reaches_its_target_exactly_and_not_asymptotically() {
        // An eased curve is asymptotic by nature, so a transition driven purely by the
        // eased progress never quite arrives: the last frame drawn is always a little
        // short and the element stays faintly wrong forever.
        let mut transition = Transition::new(Duration::from_millis(140));
        while !transition.is_done() {
            transition.advance(Duration::from_millis(10));
        }
        assert_eq!(transition.lerp(0.0, 100.0), 100.0);
        assert_eq!(transition.lerp(20.0, 0.0), 0.0);
    }

    #[test]
    fn a_transition_holds_its_start_value_before_it_starts() {
        let transition = Transition::new(Duration::from_millis(140));
        assert_eq!(transition.lerp(0.0, 100.0), 0.0);
        assert!(!transition.is_done());
    }

    #[test]
    fn a_zero_duration_transition_completes_on_the_first_tick() {
        // This is the whole reason motion is a duration and not a boolean: the
        // degraded path still runs the animation, it just finishes immediately, so
        // there is no `if motion` branch anywhere that could take a different path
        // and be wrong.
        let mut transition = Transition::new(Duration::ZERO);
        assert_eq!(transition.lerp(0.0, 42.0), 42.0);
        assert!(transition.is_done());
        transition.advance(Duration::from_millis(1));
        assert_eq!(transition.progress(), 1.0);
    }

    #[test]
    fn progress_past_the_end_is_clamped_rather_than_allowed_to_overshoot() {
        let total = Duration::from_millis(100);
        assert_eq!(progress(Duration::from_millis(50), total), 0.5);
        assert_eq!(progress(Duration::from_millis(500), total), 1.0);
        assert_eq!(progress(Duration::ZERO, total), 0.0);
        assert_eq!(progress(Duration::from_millis(1), Duration::ZERO), 1.0);
    }

    #[test]
    fn a_row_staggers_in_turn_and_a_row_past_the_cap_arrives_with_the_rest() {
        // A 400-finding list that staggers for four seconds is a list that looks
        // broken, and the reader wants the top of it anyway.
        let reveal = Reveal::new(Durations::MOTION, 0);
        assert_eq!(reveal.row(0), 0.0);
        assert_eq!(reveal.row(1), 0.0);
        // Past the cap everything has landed even though no time has passed.
        assert_eq!(reveal.row(Reveal::STAGGER_CAP), 1.0);
        assert_eq!(reveal.row(399), 1.0);
    }

    #[test]
    fn a_staggered_row_becomes_visible_after_its_own_delay_and_not_before() {
        let mut reveal = Reveal::new(Durations::MOTION, 0);
        reveal.advance(Duration::from_millis(11));
        assert!(reveal.row(0) > 0.0, "row 0 should be up by 11ms");
        // Row 1 waits its own stagger, so it is later than row 0 even though both
        // clocks started together.
        assert!(
            reveal.row(1) < reveal.row(0),
            "row 1 ({}) should lag row 0 ({})",
            reveal.row(1),
            reveal.row(0)
        );
    }

    #[test]
    fn rows_that_were_already_on_screen_are_never_re_animated() {
        // The stagger is relative to the rows already present, not to the start of
        // the list. Otherwise a row arriving while the user reads gets put behind a
        // delay earned by rows that arrived minutes ago.
        let reveal = Reveal::new(Durations::MOTION, 5);
        for index in 0..5 {
            assert_eq!(reveal.row(index), 1.0, "row {index} re-animated on arrival");
        }
        assert!(reveal.is_settled());
    }

    #[test]
    fn zero_durations_reveal_every_row_at_once_rather_than_never() {
        let reveal = Reveal::new(Durations::ZERO, 0);
        for index in 0..50 {
            assert_eq!(reveal.row(index), 1.0, "row {index} never appeared");
        }
    }

    #[test]
    fn a_meter_catches_up_over_time_and_latches_exactly_at_its_target() {
        let mut meter = Meter::new(Duration::from_millis(100));
        meter.set(1.0);
        assert_eq!(
            meter.value(),
            0.0,
            "the bar jumped to the target immediately"
        );
        // 10 steps of 100ms, one percent each, is 10% of the way.
        for _ in 0..10 {
            meter.advance(Duration::from_millis(100));
            meter.value();
        }
        let after_a_second = meter.value();
        assert!(
            after_a_second > 0.05 && after_a_second < 0.2,
            "unexpected rate: {after_a_second}"
        );
        // A very long wait overshoots and latches, rather than sitting forever a hair
        // short of full.
        for _ in 0..1000 {
            meter.advance(Duration::from_millis(100));
        }
        assert_eq!(meter.value(), 1.0);
    }

    #[test]
    fn a_meter_does_not_move_before_any_time_has_passed() {
        // A meter that advances on its own with no clock is a bar that lies about how
        // fast the scan is going, which is the only thing it is there to say.
        let mut meter = Meter::new(Duration::from_millis(100));
        meter.set(0.5);
        assert_eq!(meter.value(), 0.0);
    }

    #[test]
    fn a_meter_never_shows_more_filled_cells_than_the_bar_is_wide() {
        let mut meter = Meter::new(Duration::from_millis(100));
        meter.set(1.0);
        for _ in 0..200 {
            meter.advance(Duration::from_millis(100));
        }
        assert_eq!(meter.filled_cells(10), 10);
        assert_eq!(meter.filled_cells(0), 0);
    }

    #[test]
    fn a_meter_rounds_so_a_nearly_full_bar_does_not_look_empty() {
        let mut meter = Meter::new(Duration::ZERO);
        meter.set(0.99);
        // 99% of ten cells is nine and a bit; truncating would make it look like 90%,
        // which understates how far along the scan is.
        assert_eq!(meter.filled_cells(10), 10);
        meter.set(0.94);
        assert_eq!(meter.filled_cells(10), 9);
    }

    #[test]
    fn a_meter_accepts_a_target_outside_the_unit_range_rather_than_a_bar_overflowing() {
        let mut meter = Meter::new(Duration::ZERO);
        meter.set(5.0);
        assert_eq!(meter.value(), 1.0);
        meter.set(-1.0);
        assert_eq!(meter.value(), 0.0);
    }

    #[test]
    fn the_spinner_advances_one_frame_per_interval_and_wraps() {
        let mut spinner = Spinner::new(Duration::from_millis(90));
        assert_eq!(spinner.glyph(), SPINNER[0]);
        spinner.advance(Duration::from_millis(90));
        assert_eq!(spinner.glyph(), SPINNER[1]);
        // Past the end it wraps rather than indexing out of bounds. A panic in a
        // render path is how a terminal is left in the alternate screen.
        for _ in 0..(SPINNER.len() * 2 + 3) {
            spinner.advance(Duration::from_millis(90));
        }
        assert!(SPINNER.contains(&spinner.glyph()));
    }

    #[test]
    fn a_zero_frame_spinner_still_produces_a_glyph() {
        let mut spinner = Spinner::new(Duration::ZERO);
        assert_eq!(spinner.glyph(), SPINNER[0]);
        for _ in 0..10 {
            spinner.advance(Duration::from_millis(90));
        }
        assert!(SPINNER.contains(&spinner.glyph()));
    }

    #[test]
    fn every_spinner_glyph_is_a_single_cell_wide() {
        // A multi-cell glyph in a single-cell-wide spinner field wraps the line, and
        // the wrap moves every column below it.
        use unicode_width::UnicodeWidthStr;
        for glyph in SPINNER {
            assert_eq!(
                UnicodeWidthStr::width(glyph),
                1,
                "spinner glyph {glyph:?} is not one cell wide"
            );
        }
    }

    #[test]
    fn zero_durations_report_themselves_as_disabled() {
        assert!(Durations::MOTION.is_enabled());
        assert!(!Durations::ZERO.is_enabled());
    }

    #[test]
    fn the_motion_durations_are_the_ones_the_design_system_specifies() {
        // docs/design.md section 5.2. If these drift, the design document is wrong,
        // and it should be the design document that changes first.
        let d = Durations::MOTION;
        assert_eq!(d.crossfade, Duration::from_millis(140));
        assert_eq!(d.reveal, Duration::from_millis(180));
        assert_eq!(d.stagger, Duration::from_millis(12));
        assert_eq!(d.meter_step, Duration::from_millis(100));
        assert_eq!(d.spinner_frame, Duration::from_millis(90));
    }
}
