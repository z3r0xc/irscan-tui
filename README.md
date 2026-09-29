# IRScan TUI

A terminal interface for [IRScan](https://github.com/z3r0xc/irscan) — the read-only
Windows endpoint triage tool that finds unauthorised remote-control and employee-monitoring
agents.

The point of it is the **report history**. The engine keeps one "last scan". A triage operator
works a shift: scans a machine, sits with the finding, scans again, and needs to know what
changed. This keeps every scan and makes that comparison the centre of the interface.

## What it does

- **Run a scan** and watch it run — per-collector progress, elapsed time, and findings as
  they land. A collector that failed says so, because "we found nothing" and "we could not
  look" are the two claims this tool exists to keep apart.
- **Read a report** with severity ordering, drill-down into evidence and remediation,
  composable filters and free-text search. The visible and total counts are always on
  screen, and a filter that hides a HIGH finding is marked.
- **Keep an archive** of every scan and **compare** any two of the same host: what appeared,
  what resolved, and what changed severity. This is the feature the front end exists for.
- **Watch the live log** of collectors, findings, warnings and failures.
- **Browse and stage rule edits**, which apply to the *next* scan and say so plainly.

## Running it

```
cargo build --release
./target/release/irscan-tui                 # start empty
./target/release/irscan-tui --open report.json
```

Keys: `q` quit · `1`–`5` or `tab` screens · `s` scan · `/` search · `j`/`k` move ·
`x` compare · `d` delete · `e` export · `?` help.

Flags: `--open PATH`, `--archive-dir PATH`, `--no-motion`, `--ascii`, `-V`, `-h`.

## Cross-platform, and what that means

The engine is raw Win32 FFI with **no `cfg(windows)` anywhere in it**, so `windows-sys` does
not compile off Windows. `irscan` is therefore declared as a `cfg(windows)`-gated
*dependency* rather than the code being gated, and only `src/scan.rs` may mention a platform.

The consequence is that **the whole interface builds and is tested on every platform**:
theme, motion, the report codec, all five screens, the archive and the comparison. On
Linux or macOS, `s` reports that this build has no engine and the rest is unchanged — a
report reader is a real second implementation of the same trait, not a stub, and importing,
reading, searching, filtering, comparing and exporting all behave identically. Only
*scanning a live Windows host* is Windows-only.

This is not a claim in a document. `the_platform_boundary_is_the_only_place_a_platform_is_named`
reads this crate's own source and fails if a `cfg` appears anywhere else, and the Linux CI
job is what proves the dependency really is platform-gated.

## The design

[`docs/design.md`](docs/design.md) is normative. One rule runs through all of it:

> **Severity is carried by glyph, weight and tag. Never by hue.**

A report gets pasted into tickets, projected, and read by colour-blind reviewers. So every
severity is three simultaneous signals, and the interface is verified with colour switched
*off* — where severity, focus and state all still read.

Monochrome is not a fallback, it is a case that is designed for. Three things change there
and are documented: tokens collapse to `Reset` (which is not the same as emitting nothing),
an unfocused border is dropped rather than dimmed, and a meter's two halves differ by glyph.

## Layout that does not move

Five regions with fixed heights: header, tab bar, content, status, key hints. Only content
grows. Rendered with zero findings and with 400, the chrome is byte-identical — pinned by a
test, because a report that reflows as data arrives is a report nobody can scan.

Verified at 100, 80, 60 and 40 columns.

## Safety

Every string rendered came off a machine an attacker controls. They are sanitised at the
boundary — ANSI escapes, OSC 8 hyperlinks, bidirectional overrides, zero-width characters
and C1 controls all removed — and truncated by **display cell**, not by characters or bytes,
because a row that overflows its column wraps and destroys the grid.

A report can also name the file it wants you to delete. Path containment is checked against
the archive directory before anything is removed, and a traversal is refused rather than
cleaned.

There is no `unsafe` in this crate, and no `std::process::Command` anywhere: the engine owns
the only unsafe in the system, and no collected string can reach a command interpreter.

`cargo audit` is clean. Five advisories in the tree all arrive by one path —
`irscan-tui → irscan → yara-x → {wasmtime, rsa, bincode}` — and none is reachable from this
crate's code. They are pinned by advisory ID rather than suppressed by crate, so a *new*
one fails the audit. See [`.cargo/audit.toml`](.cargo/audit.toml).

## Relationship to the existing front ends

`irscan` ships a CLI and a Tauri desktop application. Neither is modified and the engine is
not forked. This adds a third front end for the terminal case. `irscan::collect::default_set`
stays in the library so the three surfaces cannot drift in which checks they run, and a test
here asserts that this front end does not re-assemble the list.

## Development

```
cargo build --release
cargo clippy --all-targets -- -D warnings
cargo fmt --all --check
cargo test
```

The engine is a sibling checkout at `../prov/ir-recon`. Point `Cargo.toml` elsewhere if
yours sits somewhere else.

CI runs all four on Windows and Linux. Requirements are in
[`docs/spec.md`](docs/spec.md) with stable FR-xx and SR-xx ids; the plan is in
[`docs/plan.md`](docs/plan.md).

## Licence

MIT OR Apache-2.0, matching the engine.
