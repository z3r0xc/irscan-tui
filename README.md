# IRScan TUI

A terminal user interface for [IRScan](https://github.com/z3r0xc/irscan) — the read-only
Windows endpoint triage tool that finds unauthorised remote-control and employee-monitoring
agents.

The point of it is the **report history**. The engine keeps one "last scan". A triage operator
works a shift: scans a machine, sits with the finding, scans again, and needs to know what
changed. This keeps every scan and makes that comparison the centre of the interface.

## Status

Requirements are in [`docs/spec.md`](docs/spec.md) — normative, FR-xx and SR-xx, with a
decision recorded for everything deliberately not built. The visual system is specified in
[`docs/design.md`](docs/design.md).

Implementation is in progress against that spec.

## Build

```
cargo build --release
cargo clippy --all-targets -- -D warnings
cargo fmt --all --check
cargo test
```

The engine is a **platform-gated dependency**. `ir-recon` is raw Win32 FFI with no
`cfg(windows)` anywhere, so it does not compile off Windows; declaring it under
`[target.'cfg(windows)'.dependencies]` keeps the interface itself cross-platform. On a
non-Windows host the application still runs in full — it opens and reads, searches, filters
and compares reports produced elsewhere. Only *running a scan on a live Windows host* is
Windows-only. See `docs/spec.md` §2.

## Relationship to the existing front ends

`irscan` ships a CLI and a Tauri desktop application. Neither is modified, and the engine is
not forked. This adds a third front end for the terminal case; `irscan::collect::default_set`
stays in the library so the three surfaces cannot drift in which checks they run.
