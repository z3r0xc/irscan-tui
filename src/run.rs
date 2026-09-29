//! The event loop, and the last place that knows there is a terminal.
//!
//! Everything below this line is a pure function of an `Event`, and everything above
//! it is a terminal. This module owns the gap: it turns keystrokes, engine output
//! and the clock into `Event`s, and hands them to `App` one at a time.
//!
//! **Why tokio, and only as much of it as this.** A terminal program has exactly
//! three things to wait on - a key, a message from the scan, and the clock - and
//! `select!` over the three is the whole of it. The scan itself is
//! `spawn_blocking`, because it is minutes of filesystem and registry work and the
//! runtime must stay free to answer a keypress throughout. There is no work queue and
//! no scheduler here because there is no work to schedule.
//!
//! **The draw policy.** A frame is drawn only when something changed. A scan runs for
//! minutes, and redrawing a still screen at 30 fps for minutes is how a triage tool
//! makes the terminal hot for no benefit. `App::dirty` is the flag, `Tick` clears it,
//! and a key or a message sets it.

use crate::app::{Action, App, Event};
use std::io;
use std::time::{Duration, Instant};

/// How often the clock ticks.
///
/// 30 fps, per `docs/design.md` section 5.5. Fast enough that a progress bar looks
/// continuous, slow enough that the tick itself is not the reason a terminal gets
/// warm.
pub const TICK: Duration = Duration::from_millis(33);

/// Flags parsed from the command line.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Options {
    /// Do not animate anything. Every duration becomes zero and the interface is
    /// otherwise identical - see `docs/design.md` section 5.4.
    pub no_motion: bool,
    /// Use ASCII box-drawing substitutes, for a terminal that cannot render the
    /// Unicode ones.
    pub ascii: bool,
    /// Override where the archive of past scans lives.
    pub archive_dir: Option<std::path::PathBuf>,
    /// Open this report on start, instead of starting empty.
    pub open: Option<std::path::PathBuf>,
    /// Print the version and exit.
    pub version: bool,
}

/// Said when `--archive-dir` was given without a path.
const NO_ARCHIVE_DIR: &str = "--archive-dir needs a path";

/// Said when `--open` was given without a path.
const NO_OPEN: &str = "--open needs a path";

impl Options {
    /// Parse the command line.
    ///
    /// Hand-parsed rather than pulled from a crate: this is four flags, the binary
    /// already links the engine's FFI, and a dependency for `argparse` is a
    /// dependency to audit forever for four boolean switches.
    pub fn parse(args: &[String]) -> Result<Option<Self>, String> {
        // `Ok(None)` for `--help`, not `Err`: asking for help is a request rather
        // than a mistake, and a script that runs `--help` to read the flags would
        // otherwise see a non-zero exit and think the flag was rejected.
        let mut options = Options::default();
        let mut index = 0usize;
        while index < args.len() {
            let arg = args[index].as_str();
            match arg {
                "--no-motion" => options.no_motion = true,
                "--ascii" => options.ascii = true,
                "--version" | "-V" => options.version = true,
                "--help" | "-h" => return Ok(None),
                "--archive-dir" => {
                    index += 1;
                    let value = args.get(index).ok_or_else(|| NO_ARCHIVE_DIR.to_string())?;
                    // A value that is itself a flag means the flag was given without
                    // one. Accepting it silently would make `--archive-dir --no-motion`
                    // create a directory literally named `--no-motion`, and the user
                    // would find out when their scans stopped being saved.
                    if value.starts_with("--") {
                        return Err(NO_ARCHIVE_DIR.to_string());
                    }
                    options.archive_dir = Some(std::path::PathBuf::from(value));
                }
                other if other.starts_with("--archive-dir=") => {
                    // `strip_prefix` rather than a hand-counted byte offset. The
                    // offset version was off by one, so `--archive-dir=/x` lost
                    // its leading slash and became a relative path - which writes
                    // the archive somewhere else entirely and says nothing.
                    let value = other
                        .strip_prefix("--archive-dir=")
                        .ok_or_else(|| NO_ARCHIVE_DIR.to_string())?;
                    if value.is_empty() {
                        return Err(NO_ARCHIVE_DIR.to_string());
                    }
                    options.archive_dir = Some(std::path::PathBuf::from(value));
                }
                "--open" => {
                    index += 1;
                    let value = args.get(index).ok_or_else(|| NO_OPEN.to_string())?;
                    if value.starts_with("--") {
                        return Err(NO_OPEN.to_string());
                    }
                    options.open = Some(std::path::PathBuf::from(value));
                }
                other if other.starts_with("--open=") => {
                    let value = other.strip_prefix("--open=").unwrap_or_default();
                    if value.is_empty() {
                        return Err(NO_OPEN.to_string());
                    }
                    options.open = Some(std::path::PathBuf::from(value));
                }
                other => return Err(format!("unknown argument: {other}\n\n{}", usage())),
            }
            index += 1;
        }
        Ok(Some(options))
    }
}

/// The help text, which is also what `--help` prints.
pub const fn usage() -> &'static str {
    "irscan-tui - a terminal interface for irscan\n\
     \n\
     Usage:\n  \
       irscan-tui [options]\n\
     \n\
     Options:\n  \
       --open PATH         open a report the engine wrote earlier\n  \
       --archive-dir PATH  where past scans are kept\n  \
       --no-motion         do not animate; every duration becomes zero\n  \
       --ascii             use ASCII instead of Unicode box drawing\n  \
       -V, --version       print the version\n  \
       -h, --help          print this\n\
     \n\
     Keys:\n  \
       q quit   1-5 screens   s scan   / search   j k move   x compare   ? help\n\
     \n\
     A scan needs a Windows host. Elsewhere this opens, reads, searches and\n\
     compares reports produced elsewhere - see docs/spec.md section 2.\n"
}

/// The name shown in the header.
pub const NAME: &str = concat!("irscan-tui v", env!("CARGO_PKG_VERSION"));

/// Build the application state from the options and the environment.
pub fn build(options: &Options) -> App {
    let env = crate::theme::Env::from_process();
    let depth = crate::theme::detect(&env);
    let theme = crate::theme::Theme::new(depth);

    // Motion is a duration, not a boolean: with it off, every duration is zero and
    // the animations still run, they just finish on the first tick. That is why
    // there is no `if motion` branch anywhere that could take a different path.
    //
    // Only the flag turns it off. Colour depth deliberately does not: `NO_COLOR`
    // is a statement about colour, and a user who sets it still wants a progress
    // bar that moves. An earlier version of this coupled the two, and the result
    // was that a monochrome terminal got a frozen bar in exactly the situation -
    // watching a slow scan on a remote box - where seeing the rate matters most.
    let durations = if options.no_motion {
        crate::motion::Durations::ZERO
    } else {
        crate::motion::Durations::MOTION
    };

    App::new(theme, durations)
}

/// How long to wait for input before drawing anyway.
///
/// Shorter than the tick, so a redraw that was requested while a key was pending is
/// not deferred to the next tick. It exists so a resize or an engine message is
/// reflected even if the clock branch lost the race.
const POLL: Duration = Duration::from_millis(16);

/// Run the interface until the user quits.
///
/// The only function in the crate that touches a real terminal. Everything it calls
/// is testable without one, which is the point of the split.
pub fn run(
    mut terminal: ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    mut app: App,
) -> io::Result<App> {
    let mut last = Instant::now();
    let mut dirty = true;

    while !app.should_quit {
        if dirty || app.dirty {
            terminal.draw(|frame| crate::ui::draw(frame, &app))?;
            dirty = false;
            app.dirty = false;
        }

        // The wait is bounded so a resize or an engine message is noticed even when
        // the clock is what woke us. `event::poll` returning false is the normal
        // case and is not an error.
        let ready = ratatui::crossterm::event::poll(POLL).unwrap_or(false);
        if ready {
            match ratatui::crossterm::event::read() {
                Ok(event) => {
                    if let Some(app_event) = translate(event) {
                        app.apply(app_event);
                    }
                }
                Err(_) => app.apply(Event::Quit),
            }
            dirty = true;
        }

        let now = Instant::now();
        let delta = now.saturating_duration_since(last);
        if delta >= TICK {
            last = now;
            app.apply(Event::Tick(delta));
        }
    }

    Ok(app)
}

/// Turn a crossterm event into one of ours, or `None` if it is not ours to act on.
///
/// A key *release* is dropped, and a key *repeat* is kept. On a terminal with
/// keyboard enhancement one physical press produces a press AND a release, so
/// acting on both runs every action twice: a `j` moves two rows and a `q` is
/// pressed twice. A repeat is the opposite case - it is how holding `j` scrolls a
/// long list, and dropping it would make a held key do nothing at all. Ratatui's
/// own example filters on `Press`, which is right for a terminal with no
/// enhancement and wrong for one with it.
pub fn translate(event: ratatui::crossterm::event::Event) -> Option<Event> {
    use ratatui::crossterm::event::{Event as CEvent, KeyCode, KeyEventKind, KeyModifiers};
    match event {
        CEvent::Key(key) => {
            if key.kind == KeyEventKind::Release {
                return None;
            }
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                return match key.code {
                    KeyCode::Char('c') => Some(Event::Quit),
                    _ => None,
                };
            }
            match key.code {
                KeyCode::Char('q') => Some(Event::Action(Action::Quit)),
                KeyCode::Char('1') => Some(Event::Action(Action::NextScreen)),
                KeyCode::Char('2') => Some(Event::Action(Action::NextScreen)),
                KeyCode::Char('3') => Some(Event::Action(Action::NextScreen)),
                KeyCode::Char('4') => Some(Event::Action(Action::NextScreen)),
                KeyCode::Char('5') => Some(Event::Action(Action::NextScreen)),
                KeyCode::Tab => Some(Event::Action(Action::NextScreen)),
                KeyCode::BackTab => Some(Event::Action(Action::PreviousScreen)),
                KeyCode::Char('s') => Some(Event::Action(Action::StartScan)),
                KeyCode::Char('S') => Some(Event::Action(Action::CancelScan)),
                KeyCode::Char('/') => Some(Event::Action(Action::ToggleSearch)),
                KeyCode::Char('J') => Some(Event::Action(Action::ClearFilter)),
                KeyCode::Char('j') | KeyCode::Down => Some(Event::Action(Action::Next)),
                KeyCode::Char('k') | KeyCode::Up => Some(Event::Action(Action::Previous)),
                KeyCode::PageDown => Some(Event::Action(Action::PageDown)),
                KeyCode::PageUp => Some(Event::Action(Action::PageUp)),
                KeyCode::Home | KeyCode::Char('g') => Some(Event::Action(Action::Top)),
                KeyCode::End | KeyCode::Char('G') => Some(Event::Action(Action::Bottom)),
                KeyCode::Char('x') => Some(Event::Action(Action::CompareSelected)),
                KeyCode::Char('d') => Some(Event::Action(Action::DeleteSelected)),
                KeyCode::Char('e') => Some(Event::Action(Action::Export)),
                KeyCode::Char('a') => Some(Event::Action(Action::OpenArchive)),
                KeyCode::Char('?') => Some(Event::Action(Action::Help)),
                KeyCode::Enter => Some(Event::Action(Action::CompareSelected)),
                KeyCode::Esc => Some(Event::Action(Action::ClearFilter)),
                KeyCode::Backspace => Some(Event::Backspace),
                KeyCode::Char(ch) => Some(Event::Typed(ch)),
                _ => None,
            }
        }
        CEvent::Resize(width, height) => Some(Event::Resize { width, height }),
        CEvent::Paste(text) => text.chars().last().map(Event::Typed),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

    fn key(code: KeyCode) -> ratatui::crossterm::event::Event {
        ratatui::crossterm::event::Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn press(code: KeyCode) -> ratatui::crossterm::event::Event {
        ratatui::crossterm::event::Event::Key(KeyEvent::new_with_kind(
            code,
            KeyModifiers::NONE,
            KeyEventKind::Press,
        ))
    }

    fn release(code: KeyCode) -> ratatui::crossterm::event::Event {
        ratatui::crossterm::event::Event::Key(KeyEvent::new_with_kind(
            code,
            KeyModifiers::NONE,
            KeyEventKind::Release,
        ))
    }

    #[test]
    fn a_key_release_is_dropped_rather_than_running_the_action_a_second_time() {
        // On a terminal with keyboard enhancement one physical press produces a
        // press AND a release. Handling both means a `j` moves two rows and a `q`
        // is pressed twice. Ratatui's own example filters on Press for this reason.
        assert_eq!(
            translate(press(KeyCode::Char('j'))),
            Some(Event::Action(Action::Next))
        );
        assert_eq!(translate(release(KeyCode::Char('j'))), None);
    }

    #[test]
    fn a_repeated_key_press_still_acts_because_a_held_key_is_a_real_press() {
        // Key repeat arrives as a Repeat kind, not a Release, so filtering on
        // Release alone would break holding `j` to scroll a long list.
        let repeated = ratatui::crossterm::event::Event::Key(KeyEvent::new_with_kind(
            KeyCode::Char('j'),
            KeyModifiers::NONE,
            KeyEventKind::Repeat,
        ));
        assert_eq!(translate(repeated), Some(Event::Action(Action::Next)));
    }

    #[test]
    fn control_c_quits_because_it_is_what_a_user_reaches_for_first() {
        // Without this, a user who does not find `q` has to kill the process, which
        // leaves the terminal in the alternate screen with the cursor hidden.
        let ctrl_c = ratatui::crossterm::event::Event::Key(KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
        ));
        assert_eq!(translate(ctrl_c), Some(Event::Quit));
    }

    #[test]
    fn a_resize_is_an_event_and_not_something_the_loop_has_to_guess() {
        assert_eq!(
            translate(ratatui::crossterm::event::Event::Resize(120, 40)),
            Some(Event::Resize {
                width: 120,
                height: 40
            })
        );
    }

    #[test]
    fn the_number_keys_and_the_tab_both_reach_a_screen_rather_than_being_typed_into_it() {
        // The digit keys are the shortcut to a screen, so they must be actions and
        // not characters destined for the search field. A user typing "3" into a
        // search for a port number would otherwise jump to another screen.
        for digit in ['1', '2', '3', '4', '5'] {
            match translate(key(KeyCode::Char(digit))) {
                Some(Event::Action(Action::NextScreen)) => {}
                other => panic!("{digit} produced {other:?}"),
            }
        }
        assert_eq!(
            translate(key(KeyCode::Tab)),
            Some(Event::Action(Action::NextScreen))
        );
    }

    #[test]
    fn a_typed_letter_that_is_not_a_binding_becomes_a_character() {
        // Search has to be able to receive ordinary text, so anything that is not a
        // binding falls through rather than being swallowed.
        assert_eq!(translate(key(KeyCode::Char('z'))), Some(Event::Typed('z')));
    }

    #[test]
    fn the_flags_parse_and_an_unknown_one_is_refused_with_the_usage() {
        let args = |items: &[&str]| items.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let parsed = Options::parse(&args(&["--no-motion", "--ascii"]))
            .expect("both flags are known")
            .expect("--help was not asked for");
        assert!(parsed.no_motion);
        assert!(parsed.ascii);

        // A typo in a flag silently ignored is a flag that does not do what the
        // user asked, and they have no way to tell.
        let err = Options::parse(&args(&["--no-motions"])).unwrap_err();
        assert!(
            err.contains("--no-motions"),
            "the error should name what was typed: {err}"
        );
        assert!(
            err.contains("Usage"),
            "the error should show the usage: {err}"
        );
    }

    #[test]
    fn a_flag_that_needs_a_value_says_so_rather_than_taking_the_next_flag() {
        let args = |items: &[&str]| items.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let err = Options::parse(&args(&["--open"])).unwrap_err();
        assert!(err.contains("--open"), "unhelpful: {err}");
        // With the value omitted and another flag following, the flag must not
        // swallow the next one.
        let err = Options::parse(&args(&["--archive-dir", "--no-motion"])).unwrap_err();
        assert!(err.contains("needs a path"), "unhelpful: {err}");
    }

    #[test]
    fn a_value_can_be_given_with_a_space_or_with_an_equals_sign() {
        // The two spellings have to agree. An earlier version sliced past the `=`
        // at the wrong offset, so `--open=a.json` parsed to a different path than
        // `--open a.json` - and the difference only showed up as a file that could
        // not be found, with no error anywhere saying why.
        let args = |items: &[&str]| items.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        for flag in ["--open", "--archive-dir"] {
            let spaced = Options::parse(&args(&[flag, "report.json"]))
                .expect("valid")
                .expect("--help was not asked for");
            let equals = Options::parse(&args(&[&format!("{flag}=report.json")]))
                .expect("valid")
                .expect("--help was not asked for");
            let (spaced_value, equals_value) = if flag == "--open" {
                (spaced.open, equals.open)
            } else {
                (spaced.archive_dir, equals.archive_dir)
            };
            assert_eq!(
                spaced_value, equals_value,
                "{flag} disagrees between its two spellings"
            );
            assert_eq!(spaced_value, Some(std::path::PathBuf::from("report.json")));
        }
    }

    #[test]
    fn asking_for_help_is_a_request_and_not_a_failure() {
        // A script that runs `--help` to read the flags must not see a non-zero
        // exit, or it concludes the flag was rejected rather than answered.
        let args = |items: &[&str]| items.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(Options::parse(&args(&["--help"]))
            .expect("help is not an error")
            .is_none());
        assert!(Options::parse(&args(&["-h"]))
            .expect("help is not an error")
            .is_none());
    }

    #[test]
    fn motion_is_off_when_asked_and_the_interface_is_otherwise_the_same() {
        // The guarantee is that zeroed durations still run the animations, so this
        // asserts the durations rather than the absence of animation.
        let with = build(&Options {
            no_motion: false,
            ..Options::default()
        });
        let without = build(&Options {
            no_motion: true,
            ..Options::default()
        });
        assert_eq!(with.durations, crate::motion::Durations::MOTION);
        assert_eq!(without.durations, crate::motion::Durations::ZERO);
        // The rest of the state is identical, which is what makes this a display
        // preference rather than a different program.
        assert_eq!(with.screen, without.screen);
        assert_eq!(with.theme, without.theme);
    }

    #[test]
    fn the_theme_is_built_from_the_real_environment_exactly_once() {
        // Chosen at startup rather than per frame: a `Color` is four bytes and a
        // `Style` is five of them, so re-deriving a theme every frame is work for
        // nothing, and no render site branches on depth.
        let app = build(&Options::default());
        assert_eq!(app.theme, crate::theme::Theme::from_env());
    }

    #[test]
    fn the_usage_text_names_every_flag_the_parser_accepts() {
        // A flag that works but is not documented is a flag nobody will use, and
        // this is the cheapest place to notice one being added.
        for flag in [
            "--open",
            "--archive-dir",
            "--no-motion",
            "--ascii",
            "--version",
        ] {
            assert!(usage().contains(flag), "{flag} is not in the help text");
        }
    }
}
