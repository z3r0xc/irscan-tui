# Implementation plan

Each phase is a vertical slice that ends green: `cargo build`, `cargo clippy --all-targets
-- -D warnings`, `cargo fmt --all --check` and `cargo test` all pass before the next begins.
Tests are written before the code that satisfies them, and each is named after the behaviour
guaranteed, with the requirement id in the doc comment — so a test failure says which
requirement broke.

The commit list below is the plan. Do not batch phases into one commit.

---

## Phase 0 — Scaffolding

**Nothing here is testable, so nothing here is tested.** It is the floor every later phase
rests on.

- `Cargo.toml`: crate `irscan-tui`, edition 2021, `rust-version = "1.85"` to match the
  engine. Dependencies `ratatui 0.30`, `serde` + `serde_json`, `unicode-width`,
  `directories` (archive location), `thiserror`. Dev-deps `tokio` with
  `features = ["macros", "rt", "test-util", "time", "sync"]` — `test-util` is what makes
  `start_paused` possible and it is *not* in `full`.
- **The platform boundary, declared exactly once:**
  ```toml
  [target.'cfg(windows)'.dependencies]
  irscan = { path = "../../prov/ir-recon" }
  ```
- `[lints]` copied verbatim from the engine's `Cargo.toml`, so `cargo clippy` means the same
  thing in both repositories.
- `rustfmt.toml` (the engine has none; one small file is cheaper than a reformat later).
- `src/lib.rs` declaring the modules, with the module doc stating the dependency rule from
  `docs/spec.md` section 4 as a *rule*, in the engine's own idiom.

**Gate:** `cargo build` is warning-free on Windows, and the whole crate compiles with
`--no-default-features` on a non-Windows target with no `irscan` symbol anywhere.

---

## Phase 1 — `theme` (FR-19, QR-6)

The foundation. Nothing renders before this exists, and the colour-degradation requirement
is cheapest to satisfy here rather than retrofitted into ten render sites.

Tests first:
- `every_token_resolves_in_every_depth` — the 4-depth x ~16-token matrix has no hole
  (FR-21).
- `no_colour_means_no_colour_is_emitted` — at `Depth::None`, every style the theme produces
  has `fg`/`bg` unset. This is what makes a monochrome terminal legible instead of
  black-on-black (QR-6).
- `at_sixteen_colours_the_label_step_is_bold_not_dim` — the degradation rule from
  `docs/design.md` section 6.1, asserted, because it is the one place the spec deliberately
  breaks its own typography rule.
- `severity_is_distinguishable_with_no_colour_at_all` — renders a HIGH/MED/LOW/INFO row with
  depth `None` and asserts the glyphs and tags differ. This is the test of the whole section 1
  premise, and it is the reason the design is not colour-dependent.

Then: `Depth` detection from the environment as pure functions over an `Env` struct (never
`std::env` directly — that is what makes it testable), the two baked palettes, and the
`Theme`.

---

## Phase 2 — `motion` (FR-20, QR-4)

Pure functions. No terminal, no clock, no globals.

Tests first:
- `an_easing_curve_is_monotonic_and_bounded` — for 1000 samples, `ease_out` never decreases
  and stays in `[0, 1]`.
- `a_transition_reaches_its_target_exactly_at_its_duration` — at `t >= duration` the value is
  the target, not asymptotically near it. Floating point makes this a real bug class.
- `a_staggered_row_beyond_the_cap_appears_with_the_rest` — row 11 is not still invisible
  after 200 ms (`docs/design.md` section 5.3).
- `zero_durations_complete_in_one_tick` — the degraded path still *runs* the animation, so
  there is no `if motion { }` branch to get wrong (section 5.4).

Then: `ease_out`, `ease_in_out`, `lerp`, `Transition`, `Reveal`, `Meter`, `Spinner`, and a
`Durations` struct that the `--no-motion` flag zeroes.

---

## Phase 3 — `report` codec (SR-1, SR-2, FR-8, FR-11)

The engine deliberately ships no JSON parser, so the schema belongs here — and it is the
thing most likely to drift, so it is pinned the way the engine pins its own wire contract.

Tests first, using a fixture that is a real `irscan --json` output:
- `a_report_round_trips_through_json` (FR-12).
- `an_unknown_field_in_a_future_report_does_not_break_the_load` — forward compatibility, so
  an engine upgrade cannot make the archive unreadable.
- `a_report_missing_the_optional_verdict_still_loads` — older reports have no verdict.
- `every_key_the_shell_reads_is_pinned_by_name` — the engine's own trick, from
  `desktop/src/scan.rs`. A rename in the engine is otherwise a silent blank screen.
- `a_hostile_evidence_string_cannot_reach_a_widget_unstripped` — ANSI, OSC 8, bidi
  override, NUL and a C1 control, all through the codec, all stripped (SR-1).
- `a_cjk_evidence_string_is_truncated_by_cells_not_by_characters` — a 20-char CJK string
  occupies 40 cells and must be cut to the cell budget (SR-2).

Then: `serde` structs with `#[serde(default)]` on everything optional, the sanitiser, and
the width-aware `truncate_to_cells` / `elide_head`.

---

## Phase 4 — `app` state (FR-4..FR-7, FR-13)

The state machine, with no terminal and no engine. Every transition is a function from an
event to a new state.

Tests first:
- `a_scan_cannot_be_started_while_one_is_running` (FR-4) — the guard that keeps a second
  scan from interleaving with the first.
- `a_failing_collector_is_recorded_and_the_scan_continues` (FR-7) — the requirement that
  "found nothing" and "could not look" stay distinguishable.
- `a_filter_that_hides_a_high_finding_is_marked` (FR-10) — the safety property in section 8 of
  the spec; a filter must not be able to make a HIGH finding silently disappear.
- `comparing_two_scans_of_different_hosts_is_refused` (FR-13).
- `two_scans_of_one_host_report_what_changed` — added, resolved, and severity movement.
- `deleting_from_the_archive_refuses_a_path_that_escapes_the_directory` (SR-3) — the traversal
  test, and the reason deletion is not `std::fs::remove_file` on a stored string.

Then: `App`, `Screen`, `Focus`, the `Event` enum, and `apply`.

---

## Phase 5 — `scan` engine boundary (FR-1, FR-2, FR-3, SR-8)

The only module allowed to be platform-gated, and the one that must stay small.

Tests first:
- `the_platform_boundary_is_one_module` (SR-8) — a test that counts `cfg(` occurrences across
  the tree and fails if it is not exactly the ones in this module. This is the test that keeps
  the cross-compile claim true instead of aspirational.
- `no_unsafe_outside_the_engine_boundary` (SR-7) — the same trick, reading the source. The
  engine does this for its own build script; it is a cheap invariant to pin and an expensive
  one to discover later.
- `an_unprivileged_scan_names_its_blind_spots_before_it_starts` (SR-6).

Then: `Engine` trait, the Windows implementation calling `irscan::collect::run_all_with`, and
the import implementation that decodes a report file. The trait is what keeps `app` free of
`cfg` (FR-3).

---

## Phase 6 — `ui` (FR-8..FR-11, FR-19, QR-7)

Screens, rendered to a `TestBackend` buffer in every test. No terminal, no engine (FR-1).

Tests first — each asserts on `Buffer` cells, not on rendered strings, so style is asserted
too:
- `the_header_status_and_hint_rows_do_not_move_when_findings_arrive` (QR-7) — the layout
  property from `docs/design.md` section 2.6, and the most valuable test in the phase: it is
  the one that catches an entire class of layout regressions.
- `a_finding_row_carries_glyph_and_tag_before_it_carries_colour` (FR-19, QR-6).
- `a_long_path_is_elided_at_the_head_and_never_wrapped` (SR-2).
- `the_visible_and_total_counts_are_both_on_screen` (FR-10).
- `every_screen_renders_at_eighty_columns_and_at_two_hundred` — two widths, because a layout
  that only works at one is not a layout.

Then: header, tab bar, status, hint bar, and the five screens.

---

## Phase 7 — `run` (FR-4, FR-20)

The loop, and the last place that knows about a terminal.

Tests first:
- `a_quit_key_wins_even_with_a_tick_and_a_progress_event_pending` — `biased;` with the quit
  branch first, or this test flakes (tokio's `select!` picks a random ready branch).
- `a_scan_runs_off_the_render_thread_and_the_ui_keeps_responding` (FR-4) — the property that
  matters most about the async structure, and the one that is hardest to eyeball.
- `a_still_screen_does_not_redraw` (section 5.5).

Then: `tokio::select!` over input / scan events / tick, `spawn_blocking` for the scan,
`ratatui::init()` + `restore()` with a panic hook so a panic does not leave the terminal in
the alternate screen.

---

## Phase 8 — Rules browser (FR-16..FR-18)

Last, because it is the only screen that writes to disk.

Tests first:
- `a_staged_edit_is_labelled_staged_and_says_it_applies_to_the_next_scan` (FR-17) — the
  honesty requirement, and the one most likely to regress into a lie.
- `there_is_no_key_that_reloads_rules_in_a_running_scan` (FR-18) — pins the absence, so a
  future "helpful" addition has to argue with a failing test.

---

## Phase 9 — Wiring, verification, GitHub

- CLI flags `--no-motion`, `--ascii`, `--archive-dir`, and the report import path.
- README usage, `docs/` cross-links updated.
- **Smoke run of the real binary** — launched, all five screens driven, output captured. Tests
  are not a substitute; the TUI is a visual artefact and has to be looked at.
- Security review pass against `docs/spec.md` section 5, item by item.
- `cargo audit` if the toolchain has it; otherwise a manual dependency review.
- Push, open a PR against a `feature/` branch referencing the spec.

---

## Order rationale

`theme` and `motion` are pure and have no dependencies, so they are the cheapest place to
establish that the test setup itself works. `report` comes before `app` because `app` is
mostly about what to do with decoded data. `app` comes before `ui` because a screen is a
function of state, and testing a function of state that does not exist yet is how render
code ends up doing business logic. `scan` is late because it is the only part that cannot be
tested on a non-Windows machine, and the parts that can be tested should be finished and green
first. `rules` is last because it is the only screen that writes.
