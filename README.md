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
> **Not built:** the in-app rule editor (FR-16–18) and report export (FR-11). Both say
> so on screen rather than pretending — a `rules` screen promising "press e to edit" that
> does nothing is worse than one that admits the editor is not here, and an export button
> reporting success while writing nothing is a lie told about a result someone is about to
> act on. Edit `rules/` in the engine directly, or pipe `irscan --json` out of the CLI.

## Running it

```bash
git clone https://github.com/z3r0xc/irscan-tui
cd irscan-tui

# The engine is a path dependency. Put it beside this repository, or point
# Cargo.toml at wherever yours lives:
#   [target.'cfg(windows)'.dependencies]
#   irscan = { path = "../irscan/ir-recon" }
git clone https://github.com/z3r0xc/irscan ../irscan   # or adjust the path

cargo build --release
./target/release/irscan-tui
```

One executable, **7.3 MB**, nothing to install beside it.

It does import `VCRUNTIME140.dll`, the Visual C++ runtime, which is **not** on a clean
Windows install — it arrives with the redistributable. So a binary copied to a fresh
machine may refuse to start. Build it self-contained and that goes away:

```bash
RUSTFLAGS="-C target-feature=+crt-static" cargo build --release
```

That produces a 7.4 MB binary whose only imports are Windows system libraries, all of
which are present on every install. The engine already does this — see
`prov/.cargo/config.toml`, which sets the same flag and explains why there — so this
project is consistent with it rather than inventing a second answer.

```bash
./target/release/irscan-tui --open report.json    # read a report produced earlier
./target/release/irscan-tui --elevate             # ask for administrator rights first
./target/release/irscan-tui --no-motion           # no animation
```

Keys: `q` quit · `1`–`5` or `tab` screens · `s` scan · `S` cancel · `/` search ·
`j`/`k` move · `J` clear filter · `x` compare · `d` delete · `e` export · `?` help.

### Administrator rights, and why they matter

**A scan without them cannot see the Security event log, Prefetch, or the image paths of
protected processes.** The report that comes back is *shorter*, not *wrong*, and a short
report reads like a clean one — which is the confusion this tool is built to prevent. So:

- The header says `not admin · reduced coverage` for as long as that is true.
- The scan screen **names** the three blind spots before the first collector runs.
- `--elevate` asks for administrator rights through a UAC prompt and hands the terminal to
  the elevated copy. Declining is fine — the scan runs, and the blind spots are still named.

Windows Terminal does not run elevated by default, so `irscan-tui --elevate` is the
difference between full coverage and three holes nobody was told about.

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

`cargo audit` is a fifth gate, run in CI with the same command that runs locally, so
[`.cargo/audit.toml`](.cargo/audit.toml) is the single allow-list rather than one file for a
developer and a list of action inputs for CI.

CI fetches the private engine with `secrets.IRSCAN_TOKEN` (a PAT with `repo` scope). The job
that proves the cross-compile claim — `shell-only`, which deletes the engine dependency and
builds everything — needs no secret, so that guarantee does not depend on a credential that
can expire.

## Licence

MIT OR Apache-2.0, matching the engine.
