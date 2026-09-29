//! IRScan TUI - a terminal interface for the irscan endpoint triage tool.
//!
//! Dependency direction is strictly downward, and the rule is the same one the engine
//! follows: `main -> run -> app -> ui -> theme`, with `report` (the JSON codec) and
//! `motion` (pure animation maths) as leaves that depend on nothing of ours.
//!
//! Two boundaries are load-bearing and are each pinned by a test rather than a review
//! habit:
//!
//! * **The platform boundary.** The engine is raw Win32 FFI with no `cfg(windows)`
//!   anywhere in it, so it does not compile off Windows. `irscan` is therefore a
//!   `cfg(windows)`-gated *dependency*, not a gated module, and only `scan::engine`
//!   may mention a platform. Everything else - including the whole interface, the
//!   archive and the report viewer - builds and is tested on every host. See
//!   `docs/spec.md` section 2.
//! * **The trust boundary.** Every string this program renders came from a machine
//!   under test, which is hostile by assumption: registry values, service paths, task
//!   names, file names. They pass through `report::sanitise` before they reach a
//!   widget, and are truncated by display cell rather than by length. See
//!   `docs/spec.md` section 5.
//!
//! Requirements are `docs/spec.md`; the visual system is `docs/design.md`.

// The lint set above targets production code. In test code `panic!`, `unwrap` and
// `expect` ARE the assertion mechanism, so they are lifted for the test configuration
// only - never for a shipped binary.
#![cfg_attr(test, allow(clippy::panic, clippy::unwrap_used, clippy::expect_used))]

pub mod app;
pub mod motion;
pub mod report;
pub mod run;
pub mod scan;
pub mod theme;
pub mod ui;
