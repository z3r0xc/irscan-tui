//! The platform boundary, and the only module in this crate allowed to mention a
//! platform.
//!
//! **Why the boundary is here.** The engine is raw Win32 FFI with not a single
//! `cfg(windows)` anywhere in it, so `windows-sys` does not compile on Linux or
//! macOS and a crate that depended on `irscan` unconditionally would not
//! cross-compile at all. So `irscan` is declared in `Cargo.toml` as a
//! `cfg(windows)`-gated *dependency* rather than the code being gated, and the
//! consequence is that the whole interface - theme, motion, report codec, every
//! screen, the archive, the comparison - builds and is tested on every platform.
//! Only running a live scan against a Windows host is Windows-only. See
//! `docs/spec.md` section 2.
//!
//! **Why the boundary is enforced by tests and not by review.** Both invariants
//! below are invisible in a diff. A `#[cfg]` added in a hurry is one line nobody
//! notices, and the cost of finding it is a broken build on a machine nobody has.
//! The engine pins its own manifest in exactly this way
//! (`src/lib.rs`, `the_build_passes_the_manifest_to_the_linker`), and the same
//! trick is used on this crate's own source.

use crate::report::{CodecError, Report};
use std::path::Path;

/// Why a scan could not be run.
#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error("this build has no engine: it can open and read reports, but only a Windows host can be scanned from here")]
    NoEngine,
    #[error("the engine could not be started: {0}")]
    Engine(String),
    #[error("the report could not be read: {0}")]
    Report(#[from] CodecError),
    /// A file could not be read. Named rather than folded into `Report`, because
    /// "the file is not there" and "the file is not a report" are different
    /// problems with different fixes, and the user is told which one they have.
    #[error("the file could not be read: {0}")]
    Io(#[from] std::io::Error),
}

impl ScanError {
    /// A short line for the status bar.
    pub fn headline(&self) -> String {
        match self {
            ScanError::NoEngine => "no engine on this platform".to_string(),
            ScanError::Engine(message) => format!("engine failed: {message}"),
            ScanError::Report(e) => format!("report unreadable: {e}"),
            ScanError::Io(e) => format!("file unreadable: {e}"),
        }
    }
}

/// A scan's outcome: the report, or the reason there is not one.
pub type ScanResult = Result<Report, ScanError>;

/// What the interface needs from a scanner, on every platform.
///
/// This trait is what keeps `app` free of `#[cfg]`. The state machine asks for
/// `Engine`; which implementation it gets is decided in `platform::engine()`, and
/// the non-Windows one imports a report instead of running a scan. The two differ
/// in capability, not in shape, which is why the rest of the interface does not
/// change between them.
pub trait Engine {
    /// A name for the status bar, so the user can told which of the two they have.
    fn name(&self) -> &'static str;

    /// Whether this build can scan the machine it is running on.
    fn can_scan(&self) -> bool;

    /// Open a report the engine wrote earlier, or one someone else produced.
    fn import(&self, path: &Path) -> ScanResult;

    /// What a scan without administrator rights would miss.
    ///
    /// Named, not counted: "coverage is reduced" is not actionable and "you did
    /// not see the Security log" is. SR-6.
    fn blind_spots(&self) -> Vec<&'static str>;
}

/// An engine that opens reports but cannot scan.
///
/// The non-Windows implementation, and the honest description of what a build
/// without `irscan` can do. It is not a stub: importing, reading, searching,
/// filtering, comparing and exporting all work exactly as they do on Windows,
/// which is the majority of the work a triage operator does once a scan exists.
#[derive(Debug, Default, Clone, Copy)]
pub struct ReportReader;

impl Engine for ReportReader {
    fn name(&self) -> &'static str {
        "report reader"
    }

    fn can_scan(&self) -> bool {
        false
    }

    fn import(&self, path: &Path) -> ScanResult {
        let text = std::fs::read_to_string(path)?;
        Ok(Report::parse(&text)?)
    }

    fn blind_spots(&self) -> Vec<&'static str> {
        vec!["a live host: this build has no engine to scan with"]
    }
}

/// The engine for the platform this binary was built for.
pub fn engine() -> Box<dyn Engine> {
    platform::engine()
}

/// The platform's engine: the one and only place `#[cfg]` appears in this crate.
pub mod platform {
    use super::{Engine, ReportReader, ScanResult};
    use std::path::Path;

    /// Only the Windows arm needs the engine's own types, and only the report
    /// schema is needed outside it - so the import is here rather than at the top
    /// of the file, where a non-Windows build would carry a use it never takes.
    #[cfg(windows)]
    use super::Report;

    /// The engine this platform has.
    pub fn engine() -> Box<dyn Engine> {
        Box::new(LocalEngine)
    }

    /// On Windows: the real engine, in process.
    #[cfg(windows)]
    pub struct LocalEngine;

    #[cfg(windows)]
    impl Engine for LocalEngine {
        fn name(&self) -> &'static str {
            "irscan"
        }

        fn can_scan(&self) -> bool {
            true
        }

        fn import(&self, path: &Path) -> ScanResult {
            ReportReader.import(path)
        }

        fn blind_spots(&self) -> Vec<&'static str> {
            // Whether this process holds administrator rights is the engine's
            // business, and it owns `win::is_elevated` for exactly that reason.
            // Guessing here would mean a second, disagreeing answer to a question
            // the user is being asked on screen.
            if irscan::win::is_elevated() {
                Vec::new()
            } else {
                unprivileged_blind_spots()
            }
        }
    }

    /// Everywhere else: the report reader.
    #[cfg(not(windows))]
    pub struct LocalEngine;

    #[cfg(not(windows))]
    impl Engine for LocalEngine {
        fn name(&self) -> &'static str {
            "report reader"
        }

        fn can_scan(&self) -> bool {
            false
        }

        fn import(&self, path: &Path) -> ScanResult {
            ReportReader.import(path)
        }

        fn blind_spots(&self) -> Vec<&'static str> {
            ReportReader.blind_spots()
        }
    }

    /// The progress the engine reports while it runs.
    ///
    /// The same shape on both platforms, so the run loop is written once and the
    /// non-Windows build still compiles the code that would consume it.
    #[cfg(windows)]
    pub type ScanProgress = irscan::collect::Progress;

    #[cfg(not(windows))]
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ScanProgress {
        pub collector: &'static str,
        pub elapsed_ms: u128,
        pub findings_added: usize,
        pub error: Option<String>,
    }

    /// What a scan without administrator rights would miss.
    pub fn unprivileged_blind_spots() -> Vec<&'static str> {
        vec![
            "Security event log",
            "Prefetch",
            "image paths of protected processes",
        ]
    }

    /// Run a scan on this host.
    ///
    /// Separate from the `Engine` trait because it is the one operation a report
    /// reader cannot do, and putting it on the trait would force every platform to
    /// carry a method that can only ever return `NoEngine`.
    #[cfg(windows)]
    pub fn scan<F>(quick: bool, on_progress: F) -> ScanResult
    where
        F: FnMut(&ScanProgress),
    {
        use irscan::collect;
        use irscan::model::ScanContext;
        use irscan::report::{self, HostInfo};

        let mut ctx = ScanContext::default();
        // The collector list comes from the library, not from here. The engine
        // deliberately keeps one list so its three front ends cannot drift in which
        // checks they run, and re-assembling it here would undo that.
        let collectors = collect::default_set(quick, Vec::new());
        // A collector that fails does not stop the scan; `run_all_with` records the
        // failure on the context and carries on, and the count it returns is the
        // number that could not run at all.
        let failures = collect::run_all_with(&collectors, &mut ctx, on_progress);

        let host = HostInfo {
            name: std::env::var("COMPUTERNAME").unwrap_or_default(),
            user: std::env::var("USERNAME").unwrap_or_default(),
            os: std::env::consts::OS.to_string(),
            build: String::new(),
            elevated: irscan::win::is_elevated(),
            quick,
            collected_at: irscan::win::local_time_string(),
            ..HostInfo::default()
        };
        let verdict = irscan::rules::verdict(&ctx.findings, ctx.warnings.len());
        let json = report::render_json(&host, &ctx, &verdict);
        let _ = failures;

        Ok(Report::parse(&json)?)
    }

    /// Everywhere else: refused, with the reason.
    #[cfg(not(windows))]
    pub fn scan<F>(_quick: bool, _on_progress: F) -> ScanResult
    where
        F: FnMut(&ScanProgress),
    {
        Err(super::ScanError::NoEngine)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_platform_boundary_is_the_only_place_a_platform_is_named() {
        // FR-3 and SR-8. This is the invariant that makes the cross-compile claim in
        // docs/spec.md true rather than aspirational, and it is invisible in a diff:
        // a `cfg` added in a hurry is one line nobody notices, and the cost of
        // finding it is a broken build on a machine nobody has.
        //
        // The engine does the same thing to itself, in `src/lib.rs`:
        // `the_build_passes_the_manifest_to_the_linker` asserts on build.rs's own
        // source for the same reason.
        // The search is for the *attribute*, not the text. A doc comment that
        // explains why the boundary exists has to be able to say `cfg(windows)`
        // without tripping its own check, and prose about a platform is not a
        // platform dependency.
        let offenders: Vec<String> = source_files()
            .into_iter()
            .filter(|path| !path.ends_with("scan.rs"))
            .filter(|path| platform_attribute_count(path) > 0)
            .collect();
        assert!(
            offenders.is_empty(),
            "these files carry a platform attribute and so break the cross-compile: {offenders:?}"
        );
    }

    #[test]
    fn no_unsafe_outside_the_engine_boundary() {
        // SR-7. The engine owns the only `unsafe` in the system - it is raw FFI and
        // documents every block with a SAFETY comment. A TUI that grew its own would
        // mean a second place where an invariant is asserted by comment rather than
        // by type, which is exactly the failure the engine's `win` module exists to
        // prevent.
        let offenders: Vec<String> = source_files()
            .into_iter()
            .filter(|path| {
                let text = std::fs::read_to_string(path).unwrap_or_default();
                // `unsafe` as a word, not as part of an identifier, and not in a
                // comment: the check is deliberately blunt, because a false positive
                // costs one edit to a doc comment and a false negative costs a
                // memory-safety bug in a program that renders attacker-controlled
                // strings.
                text.lines().any(|line| {
                    let code = line.split("//").next().unwrap_or("");
                    code.split_whitespace().any(|word| word == "unsafe")
                })
            })
            .collect();
        assert!(
            offenders.is_empty(),
            "unsafe found outside the engine: {offenders:?}"
        );
    }

    #[test]
    fn the_engine_runs_its_own_collector_list_rather_than_one_assembled_here() {
        // The engine keeps the list in the library so its three front ends cannot
        // drift in which checks they run. Re-assembling it here would silently undo
        // that discipline, and the drift would only show up as a discrepancy between
        // a CLI run and a TUI run of the same machine - the exact case where the
        // user needs them to agree.
        let source = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/scan.rs"))
            .expect("the boundary module should be readable");
        // Only the non-test half is searched. A test that names a collector in order
        // to assert it is *not* named there would otherwise fail on its own literal,
        // which is a test that cannot be written at all rather than one that passes.
        let code: String = source
            .split("#[cfg(test)]")
            .next()
            .unwrap_or_default()
            .to_string();
        assert!(
            code.contains("collect::default_set"),
            "the scan should use the engine's own collector list"
        );
        for collector in ["AccountsCollector", "ProcessesCollector", "EventsCollector"] {
            assert!(
                !code.contains(collector),
                "{collector} is named in the scan, so the list has been re-assembled and can drift"
            );
        }
    }

    #[test]
    fn a_build_with_no_engine_says_so_rather_than_pretending_to_scan() {
        // FR-2. The degradation is in capability, not in code and not in the UI: a
        // user on a Linux triage box must be able to open a report, read it and
        // search it, and must be told plainly that pressing scan will not work
        // rather than being left waiting for a scan that will never start.
        let engine = engine();
        if !engine.can_scan() {
            assert!(
                !engine.name().is_empty(),
                "the status bar needs a name either way"
            );
            let spots = engine.blind_spots();
            assert!(
                !spots.is_empty(),
                "a build that cannot scan must say what it cannot do"
            );
        }
    }

    #[test]
    fn a_missing_report_file_is_an_error_and_not_a_panic() {
        // Importing is the primary action on a non-Windows build, so it is the one
        // most likely to be handed a path that is not there.
        let engine = ReportReader;
        let result = engine.import(Path::new("definitely-not-a-real-report-file.json"));
        let err = match result {
            Ok(_) => panic!("a missing file was read as a report"),
            Err(e) => e,
        };
        assert!(
            !err.headline().is_empty(),
            "the status bar needs something to show"
        );
    }

    #[test]
    fn a_file_that_is_not_a_report_is_refused_rather_than_shown_as_empty() {
        // An empty report and a report that failed to parse look identical on
        // screen, and one of them is a clean machine and the other is a mistake.
        let dir = std::env::temp_dir().join("irscan-tui-scan-test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("not-a-report.json");
        std::fs::write(&path, b"this is not json at all").expect("the fixture should be writable");
        let result = ReportReader.import(&path);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            result.is_err(),
            "a non-report file was accepted as a report"
        );
    }

    #[test]
    fn a_report_with_the_wrong_schema_is_refused_by_the_import_path_too() {
        // The refusal has to happen at every door, not just in `Report::parse`'s
        // direct callers, or a future caller can walk past it.
        let dir = std::env::temp_dir().join("irscan-tui-schema-test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("v2.json");
        std::fs::write(&path, br#"{"schema":"irscan/v2","findings":[]}"#)
            .expect("the fixture should be writable");
        let result = ReportReader.import(&path);
        let _ = std::fs::remove_dir_all(&dir);
        let err = match result {
            Ok(_) => panic!("a v2 report was imported by a v1 reader"),
            Err(e) => e,
        };
        assert!(
            err.headline().contains("unreadable"),
            "unhelpful error: {}",
            err.headline()
        );
    }

    #[test]
    fn the_unprivileged_blind_spots_name_what_is_missed_rather_than_saying_coverage_is_reduced() {
        // SR-6. "Coverage is reduced" is not actionable. Naming the Security event
        // log tells the operator what to go and look at another way.
        let spots = platform::unprivileged_blind_spots();
        assert!(spots.iter().any(|s| s.contains("Security")));
        assert!(spots.iter().any(|s| s.contains("Prefetch")));
        assert!(
            !spots.iter().any(|s| s.contains("reduced")),
            "the spots should be named, not summarised"
        );
    }

    /// How many real `#[cfg(...)]` platform attributes a file carries.
    ///
    /// Comments are stripped first, and the test module is skipped, because a doc
    /// comment explaining this very boundary has to be able to name the platform it
    /// is keeping code away from. Prose about a platform is not a platform
    /// dependency, and a check that cannot tell the two apart is a check somebody
    /// will disable.
    fn platform_attribute_count(path: &str) -> usize {
        let text = std::fs::read_to_string(path).unwrap_or_default();
        let code: String = text
            .split("#[cfg(test)]")
            .next()
            .unwrap_or_default()
            .lines()
            .map(|line| {
                let mut out = String::new();
                let mut in_string = false;
                let mut escaped = false;
                for ch in line.chars() {
                    if escaped {
                        out.push(ch);
                        escaped = false;
                        continue;
                    }
                    match ch {
                        '\\' if in_string => {
                            out.push(ch);
                            escaped = true;
                        }
                        '"' => {
                            in_string = !in_string;
                            out.push(ch);
                        }
                        '/' if !in_string => break,
                        _ => out.push(ch),
                    }
                }
                out
            })
            .collect::<Vec<String>>()
            .join("\n");
        code.lines()
            .filter(|line| {
                let trimmed = line.trim_start();
                trimmed.starts_with("#[cfg(")
                    && (trimmed.contains("windows") || trimmed.contains("unix"))
            })
            .count()
    }

    /// Every `.rs` file in `src/`, so the two source-scanning tests above cannot
    /// pass by looking at fewer files than they should.
    fn source_files() -> Vec<String> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut found = Vec::new();
        collect_rs(&root, &mut found);
        assert!(found.len() >= 7, "only found {} source files", found.len());
        found
    }

    fn collect_rs(dir: &Path, out: &mut Vec<String>) {
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_rs(&path, out);
            } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                out.push(path.to_string_lossy().to_string());
            }
        }
    }
}
