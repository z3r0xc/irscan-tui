# IRScan TUI — Specification

**Status:** approved for implementation
**Date:** 2026-09-29
**Supersedes:** nothing. Adds a terminal front end to `github.com/z3r0xc/irscan`; the existing
CLI and the Tauri desktop application are untouched.

---

## 1. What this is

A terminal user interface for [IRScan](https://github.com/z3r0xc/irscan), the read-only
Windows endpoint triage tool that detects unauthorised remote-control and employee-monitoring
agents.

The desktop application already exists and works. This is not a replacement for it. It is the
surface you use when you are on a remote box over SSH, in a terminal, and want the same four
capabilities the GUI has: run a scan, watch it run, read the report, and keep the history.

The difference that justifies its existence is the report **history**. The GUI keeps a single
"last scan" snapshot (`desktop/src/monitor.rs`). A triage operator works a shift, scans a
machine, lets it sit while they work the finding, scans again, and needs to compare. That
comparison is the actual job. The TUI owns it properly.

### 1.1 Non-goals

- **Not a replacement for `irscan.exe`.** The CLI stays the tool of record. This reads its
  engine as a library; it does not fork the engine.
- **Not a remediation console.** `irscan::remediate` exists and is powerful. Driving
  destructive actions from a keyboard-driven full-screen TUI, where a mis-aimed keypress is
  invisible and undo is a scripted file, is the wrong surface for it. The TUI shows what
  *would* be remediated and renders the undo record. It does not execute.
- **No network, no listening socket, no dynamic loading.** Inherited from the engine and
  non-negotiable: the tool must not become the thing it detects (spec SR-1 of the engine).

---

## 2. The constraint that shapes everything

The engine is raw Win32 FFI with **not a single `#[cfg(windows)]`** anywhere in
`ir-recon/src`. `windows-sys` does not compile on Linux or macOS. Therefore:

> **A crate that depends on `irscan` unconditionally does not cross-compile. Period.**

This is not a portability wish; it is a build-graph fact. The resolution is structural, and
it is the reason the dependency is declared the way it is:

```toml
[target.'cfg(windows)'.dependencies]
irscan = { path = "../../prov/ir-recon" }
```

Consequences, all of which are requirements:

- **FR-1** The TUI shell — theme, motion, layout, screens, state machine, report viewer,
  archive — must build and run on **every** platform, with no engine present. This is
  verifiable: the report JSON is the boundary, and the shell only ever sees decoded JSON.
- **FR-2** On Windows, the `Scan` action invokes the real engine in-process. On non-Windows,
  the same action opens an import dialog (paste a `irscan --json` report path) and the rest
  of the application is **byte-for-byte identical in behaviour**. The degradation is in
  capability, not in code paths, not in the UI, and not in what the user can do afterwards.
- **FR-3** No `#[cfg(windows)]` may appear in any module below the engine boundary. The
  boundary is exactly one module, `src/scan/engine.rs`, which exposes the same trait
  implementation on both platforms. This is pinned by a test (SR-8).

The reason this is worth the trouble: a triage box is frequently Linux. Being able to open a
report, read it, and search it — the majority of the reading work — must not require a
Windows host.

---

## 3. Functional requirements

Numbering is stable. Every FR has at least one test named after it.

### 3.1 Scanning

- **FR-4 — Run a scan.** The user can start a full or quick scan. The scan runs off the UI
  thread. The UI never blocks, never freezes, and always accepts `q`.
- **FR-5 — Live progress.** While a scan runs, the user sees, without scrolling or asking:
  which collector is running now, how many of 14 are done, elapsed time per collector, and
  how many findings it produced. Driven by `irscan::collect::run_all_with`'s `Progress`
  callback. A silent window of 30 seconds must be visually distinguishable from a hang.
- **FR-6 — Findings appear as they land.** When a collector finishes, its findings are
  appended to the live list and animate in. They arrive per-collector, not per-event; the
  engine's `Collector::run` yields its context only on completion, and the engine is not
  modified (deliberate, see §3.7).
- **FR-7 — Failures are visible, not fatal.** A collector that errors is shown as a failed
  row with its message. The scan continues. A scan that lost 3 of 14 collectors must make
  that fact impossible to miss, because "we found nothing" and "we could not look" are the
  two claims this tool exists to keep apart.

### 3.2 The report

- **FR-8 — Read the report.** Findings are grouped by category and sorted by severity
  (delegated to `irscan::report::sorted_findings` / `group_findings` so the TUI and the text
  report can never disagree). Severity distribution, host facts, and the verdict are
  visible.
- **FR-9 — Drill down.** Select a finding, see its evidence lines, its remediation text, and
  the lead. Navigate with `j`/`k` and arrows.
- **FR-10 — Filter and search.** Filter by severity, filter by category, and free-text search
  across title, category, evidence and remediation. Filters compose. The visible/total count
  is always on screen, so a filter that hides a HIGH finding cannot silently mislead.
- **FR-11 — Export.** Any report currently in view can be written to a `.txt` and a `.json`
  via `irscan::report::render_text` / `render_json`, so a TUI screenshot is never the only
  way a result leaves the tool. The written bytes are the same bytes the CLI would write.

### 3.3 The archive

- **FR-12 — Keep every scan.** Each completed scan is persisted with its host facts, verdict,
  severity counts and findings. History is a list the user can open, not a single "last scan"
  that gets overwritten.
- **FR-13 — Compare.** Select any two archived scans of the same host and see what changed:
  findings added, findings resolved, severity movement. This is the feature the TUI exists
  for. `irscan::monitor::{snapshot, diff}` supplies the primitive over identity sets; the
  TUI supplies the history it was missing.
- **FR-14 — Delete.** The user can delete an archived scan, with the path shown. Deletion is
  the one destructive action in the TUI and it is confined to the archive directory.

### 3.4 The live view

- **FR-15 — Live event log.** A continuously updating view of collector progress, findings as
  they land, engine warnings, and errors, with the same filters as the report and a
  pause-on-scroll so reading is possible.

### 3.5 The rules browser

- **FR-16 — Browse the rule set.** The bundled YARA rules and the LOLRMM signature table are
  browsable: name, kind, what they match, and whether each is enabled.
- **FR-17 — Edit a rule.** The user can edit a rule file in an in-app editor and save it.
  A save is **staged**: the UI says plainly that the change applies to the *next* scan, and
  until then the running scan's rules are untouched. The engine compiles its rule set once
  per scan and does not expose reload, so pretending otherwise would be a lie in the UI.
- **FR-18 — No hot reload.** Not implemented, and the help text does not imply it exists.

### 3.6 Presentation

- **FR-19 — A designed surface, not a default one.** A documented token system, a
  three-weight type ramp, a single 4-cell spacing scale, hairline rules, and a layout where
  no region moves when data arrives. Specified in `docs/design.md`, which is normative.
- **FR-20 — Motion with purpose.** Transitions, progress animation and list entry animation
  run at a fixed tick, are driven by pure functions of elapsed time, and never block input.
  Motion must be switchable off and must be off by default when `NO_COLOR` is set or the
  terminal is not a TTY.
- **FR-21 — Honest degradation.** See §5.

### 3.7 Deliberate non-changes to the engine

The engine is a separate repository with its own tests. It is **not modified.** Specifically:

- `Collector::run` keeps its signature. Adding a per-finding callback would produce a
  genuinely better live feed, and it was rejected because it changes a trait that fourteen
  collectors and a second front end depend on, for a stream the TUI can approximate well
  enough from the per-collector callback it already has. The trade is: findings animate in
  per collector, not one by one. This is stated in the UI.
- The collector list, its order, and the `Progress` payload are all used as-is. The list
  lives in the library so the surfaces cannot drift; that discipline is respected, not
  second-guessed.

---

## 4. The boundary

```
  keyboard ──> run loop (tokio) ──> app state ──> view model ──> render
                    │                    │
                    │                    └──> report store (JSON on disk)
                    └──> scan engine (spawn_blocking) ──> Progress events
```

Dependencies point strictly downward, matching the engine's own documented discipline
(`ir-recon/docs/architecture.md` §2).

| Module | May depend on | Must never |
|---|---|---|
| `theme` | — | touch `irscan`, the clock, or the filesystem |
| `motion` | — | touch `irscan`, the terminal, or global state |
| `report` (codec) | `serde` | touch the engine, the terminal, or the clock |
| `app` | `report`, `motion`, `theme` | touch the terminal or spawn tasks |
| `ui` | `app`'s view models, `theme`, `motion` | touch the engine, the filesystem, or the clock |
| `scan` | `irscan` (Windows only), `report` | touch the terminal |
| `run` | everything | — |

`ui` never imports `irscan`. That is what makes FR-1 real: on a machine with no engine, the
entire interface is present, functional, and testable against fixture reports.

---

## 5. Security requirements

The engine's threat model is a **hostile analysed machine**. Every string rendered by this
TUI originates from that host: registry values, service paths, task names, file names,
process command lines, network peers. That fact drives SR-1 through SR-4.

- **SR-1 — Sanitise every untrusted string at the boundary.** ANSI escapes, OSC 8
  hyperlinks, bidirectional overrides, C0/C1 control characters and NULs are stripped
  *before* the string reaches a widget, without exception. The engine's `text::sanitize`
  already does this for its own output and is used where it fits; the TUI applies its own
  pass as well, because "we sanitised it on the way in" is exactly the reasoning that fails
  when the second caller is added later.
- **SR-2 — Truncate to the cell budget, not to bytes.** A string is truncated by display
  width using `unicode-width`, not by `char` count or byte length, so a run of CJK text or a
  combining sequence cannot push a row over the terminal edge and wrap into the next line's
  layout. This is a correctness property, not cosmetics.
- **SR-3 — Path handling.** Archive paths are built by the application, never taken raw from
  a report field. Deletion resolves and verifies containment inside the archive directory
  before it removes anything; a traversal in a stored path is refused, not sanitised into
  something plausible.
- **SR-4 — No shell, ever.** No `std::process::Command`. Elevation, if it is offered at all,
  goes through the engine's `win::elevate`, which never builds a command line. Collected
  strings must not be able to reach a command interpreter.
- **SR-5 — Read-only by default.** The TUI adds no write access to the scanned host. It
  writes only inside its own archive directory.
- **SR-6 — Elevation is explicit and visible.** If a scan will lose coverage without
  administrator rights, the UI says so *before* the scan, naming the blind spots, and
  remembers the choice for the session. It never silently scans with reduced coverage.
- **SR-7 — No `unsafe` in the TUI.** The engine owns the only `unsafe` in the system. This is
  enforced by a test that fails the build if the token appears outside the boundary module.
- **SR-8 — One platform boundary.** Exactly one module may be platform-gated (FR-3). Two
  `#[cfg]` sites is a smell that the boundary is leaking, and the test counts them.

## 6. Quality requirements

- **QR-1 — Zero warnings.** `cargo build --release` clean; `cargo clippy --all-targets --
  -D warnings` clean. Inherited from the engine's `[lints]` block and extended here, not
  relaxed: `unsafe_op_in_unsafe_fn`, `unused_must_use`, `unwrap_used`, `expect_used`,
  `panic` are all denied in production code, and lifted only under `cfg(test)`.
- **QR-2 — `cargo fmt --all --check` reports no diff.**
- **QR-3 — Every FR above has a test named after its behaviour, not after its function.**
  Test names are sentences stating the guarantee.
- **QR-4 — Deterministic tests.** No test sleeps to synchronise, no test depends on the
  terminal size, no test depends on wall-clock time. Motion is tested by calling the
  easing functions with an explicit elapsed value.
- **QR-5 — The whole test suite runs on Linux and on Windows.** This is the same constraint
  as FR-1, applied to the build.
- **QR-6 — Colour degradation is tested, not assumed.** Truecolour, 256-colour, 16-colour
  and no-colour are each covered by a test that asserts what the user actually sees.
- **QR-7 — Headless rendering is testable.** `ratatui::backend::TestBackend` renders each
  screen to a buffer, so layout assertions run in CI with no terminal attached. This is how
  "no region moves when data arrives" is verified rather than hoped for.

## 7. Explicitly out of scope

Named so that their absence is a decision, not an oversight.

- Remediation execution (§1.1).
- Diffing two arbitrary hosts — comparison is same-host, by host identity.
- Per-finding export to STIX/SARIF. The report is text and JSON, as the engine writes it.
- Remote/SSH scanning. This reads a local host; it is not an agent.
- Configurable themes beyond the two shipped (dark, plain). A theme file is a nice idea and
  it is a second design system to keep coherent; one is enough.
- Mouse support. The whole interface is reachable from the keyboard, and mouse handling in a
  full-screen TUI mostly produces mis-clicks.
- Packaging/installer. `cargo build --release` produces the binary; distribution is the
  engine's existing problem.

## 8. Risks

| Risk | Consequence | Response |
|---|---|---|
| The report JSON schema drifts when the engine changes | Archive entries silently mis-parse; a scan reads back as an empty report | Codec is `#[serde(untagged)]`-tolerant on unknown fields; a contract test pins every key the shell reads (the engine's own trick, `desktop/src/scan.rs`) |
| `#[cfg(windows)]` leaks past the boundary | The app stops cross-compiling, and the leak is invisible in review | SR-8 counts `cfg` sites; the count is pinned |
| Animations make the UI feel busy or slow | A triage tool is used under time pressure | Motion is short, single-purpose, and off with `--no-motion` / `NO_COLOR` |
| A filter hides a HIGH finding | The operator reads "clean" when it is not | FR-10 requires the visible/total count on screen at all times; a filter that hides a HIGH raises a visible marker |
| Untrusted strings corrupt the display | Terminal hijack, misleading output | SR-1, SR-2, tested |
| Live-feed granularity is per-collector, not per-event | Live view feels chunky | Accepted (§3.7), stated in the UI rather than hidden |
