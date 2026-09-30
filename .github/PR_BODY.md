## What this is

A terminal front end for [irscan](https://github.com/z3r0xc/irscan), the read-only Windows
endpoint triage tool. The existing CLI and Tauri desktop app are untouched; the engine is not
forked.

The reason it exists is **report history**. The engine keeps one "last scan". A triage operator
works a shift — scans, sits with a finding, scans again — and needs to know what changed. This
keeps every scan and makes that comparison the centre of the interface.

## The constraint that shaped it

`ir-recon` is raw Win32 FFI with **no `cfg(windows)` anywhere in it**, so `windows-sys` does not
compile off Windows. A crate depending on `irscan` unconditionally would not cross-compile at
all.

So `irscan` is a `cfg(windows)`-gated *dependency* rather than gated code, and only
`src/scan.rs` may name a platform. The consequence: the whole interface — theme, motion, the
report codec, all five screens, the archive, the comparison — **builds and is tested on every
platform**. On Linux, `s` says there is no engine and everything else is identical. A report
reader is a real second implementation of the same trait, not a stub.

That claim is enforced, not asserted:
`the_platform_boundary_is_the_only_place_a_platform_is_named` reads this crate's own source,
and a `shell-only` CI job builds the whole thing with the engine dependency *deleted*,
needing no secret.

## Documentation

- [`docs/spec.md`](docs/spec.md) — normative requirements, FR-xx and SR-xx, with a decision
  recorded for everything deliberately not built
- [`docs/design.md`](docs/design.md) — normative visual system and motion rules
- [`docs/plan.md`](docs/plan.md) — the phases, each ending green

## Design, in one rule

> **Severity is carried by glyph, weight and tag. Never by hue.**

A report gets pasted into tickets, projected, and read by colour-blind reviewers. Every
severity is three simultaneous signals, and the interface is verified **with colour switched
off**, where severity, focus and state all still read.

Monochrome is a designed case, not a fallback. Three things change there, and all three are
documented and tested: tokens collapse to `Reset` (which is *not* the same as emitting
nothing), an unfocused border is dropped rather than dimmed, and a meter's two halves differ
by glyph.

Layout is five fixed-height regions; only content grows. Rendered with 0 and with 400 findings
the chrome is byte-identical — pinned by a test, and verified at 100, 80, 60 and 40 columns.

## Safety

Every rendered string came off a machine an attacker owns. They are sanitised at chokepoints
rather than call sites (`push_log`, `fail`, the codec's `sanitised()`) and truncated by
**display cell** rather than characters or bytes, because a row that overflows its column
wraps and destroys the grid.

`ensure_inside` refuses a traversal rather than cleaning it. The Linux CI job found that the
first version accepted one in any path that did not exist yet, because `Path::starts_with`
compares components and `..` is a component.

No `unsafe` in this crate, no `std::process::Command` anywhere, no write outside the archive
directory. `cargo audit` is clean: five advisories all arrive via
`irscan-tui → irscan → yara-x → {wasmtime, rsa, bincode}`, none reachable from this code, and
pinned by advisory ID rather than suppressed by crate — so a *new* one in those crates still
fails the audit.

An independent security review of SR-1..SR-8 returned seven PASS, one FAIL and ten further
findings. All of the real ones are fixed, and the FAIL was the interesting one: the
reduced-coverage warning existed but read `report.host.admin` — the report's own claim about
itself — so it appeared *with the results*, after the operator had already committed to the
scan. It is now named on the dashboard before the first collector runs.

## Verification

```
cargo build --release          # zero warnings
cargo clippy --all-targets -- -D warnings
cargo fmt --all --check
cargo test                    # 165 tests
```

CI runs all four on Windows and Linux, plus a `shell-only` job with the engine removed and an
audit job.

## Note on the engine dependency in CI

The engine is a private repository, so CI fetches it with `secrets.IRSCAN_TOKEN` (a PAT with
`repo` scope). The job that proves the central cross-compile claim needs no secret at all, so
that guarantee cannot be taken away by an expired credential.

The alternative — making the engine public to satisfy CI — was rejected: it would publish a
tool whose whole purpose is endpoint triage.
