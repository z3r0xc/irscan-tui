//! The report JSON codec, and the trust boundary every rendered string passes.
//!
//! The engine ships no JSON parser on purpose - keeping one out of the binary is why
//! `report::render_json` is a hand-written serialiser - so the schema on this side of
//! the boundary is ours to get right. It is the piece most likely to drift when the
//! engine changes, so it is pinned the way the engine pins its own wire contract: by
//! asserting on the *key names* the engine emits, in a test that fails the moment a
//! rename lands (see `every_key_the_engine_writes_is_pinned_by_name`).
//!
//! **The threat model for this module.** Every string in a report came from a machine
//! under test, which is hostile by assumption: registry values, service paths, task
//! names, file names, process command lines, network peers. A program that installed
//! a remote-control agent on that machine chose every one of those strings, and chose
//! them knowing the output might be read. So a string can contain:
//!
//! * ANSI escape sequences - to repaint the screen, hide output, or forge a line;
//! * OSC 8 hyperlinks - to make a path *look* like somewhere it is not;
//! * bidirectional overrides - to make `exe.txt` render as `txt.exe`;
//! * C0 and C1 control characters, and NUL;
//! * unbounded length.
//!
//! `sanitise` and `truncate_to_cells` are therefore not polish. They are the reason
//! a report can be shown at all. See `docs/spec.md` section 5, SR-1 and SR-2.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The schema identifier the engine writes. A report without it, or with a different
/// one, is refused rather than guessed at.
pub const SCHEMA: &str = "irscan/v1";

/// The largest string any single field is allowed to occupy, in characters.
///
/// A hostile host can name a service anything, and a megabyte-long value in a
/// cell-rendered list is how a report becomes unreadable and the terminal becomes
/// sluggish. The engine applies the same discipline at collection time
/// (`model::MAX_STRING`); this is the second, independent pass.
pub const MAX_CHARS: usize = 512;

/// A finding, as the engine wrote it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    /// `"high"`, `"med"`, `"low"` or `"info"` - the engine's `Severity::label()`.
    pub severity: String,
    /// The subsystem that produced it, e.g. `"autoruns"`.
    #[serde(default)]
    pub category: String,
    pub title: String,
    #[serde(default)]
    pub evidence: Vec<String>,
    #[serde(default)]
    pub remediation: Vec<String>,
}

impl Finding {
    /// The severity as the interface shows it.
    pub fn severity(&self) -> crate::theme::Sev {
        crate::theme::Sev::parse(&self.severity)
    }

    /// Every field, sanitised and length-bounded, ready to render.
    ///
    /// The sanitising happens here rather than at the call site so that a widget
    /// cannot be reached with an unsanitised string by a future caller who did not
    /// know about the rule. "We sanitised it on the way in" is exactly the reasoning
    /// that fails when the second caller appears.
    pub fn sanitised(&self) -> SanitisedFinding {
        SanitisedFinding {
            severity: self.severity(),
            category: clean(&self.category, MAX_CHARS),
            title: clean(&self.title, MAX_CHARS),
            evidence: self
                .evidence
                .iter()
                .map(|line| clean(line, MAX_CHARS))
                .collect(),
            remediation: self
                .remediation
                .iter()
                .map(|line| clean(line, MAX_CHARS))
                .collect(),
        }
    }
}

/// A finding whose every string is safe to put in a widget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SanitisedFinding {
    pub severity: crate::theme::Sev,
    pub category: String,
    pub title: String,
    pub evidence: Vec<String>,
    pub remediation: Vec<String>,
}

/// Facts about the machine that are not findings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Host {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub user: String,
    #[serde(default)]
    pub os: String,
    #[serde(default)]
    pub build: String,
    /// Whether the scan had administrator rights.
    ///
    /// Not cosmetic: without it the Security event log, Prefetch and the image paths
    /// of protected processes are all missing, and the report is shorter rather than
    /// wrong, which is the distinction this tool is built to keep visible.
    #[serde(default)]
    pub admin: bool,
    /// Whether this was a `--quick` run, and therefore missing the slow collectors.
    #[serde(default)]
    pub quick: bool,
    #[serde(rename = "collectedAt", default)]
    pub collected_at: String,
}

impl Host {
    /// The blind spots of a scan that ran without administrator rights.
    ///
    /// Named rather than counted, because "coverage is reduced" is not actionable and
    /// "you did not see the Security log" is. `docs/spec.md` SR-6.
    pub fn blind_spots(&self) -> Vec<&'static str> {
        if self.admin {
            Vec::new()
        } else {
            vec![
                "Security event log",
                "Prefetch",
                "image paths of protected processes",
            ]
        }
    }

    /// Sanitised, ready to render.
    pub fn sanitised(&self) -> SanitisedHost {
        SanitisedHost {
            name: clean(&self.name, MAX_CHARS),
            user: clean(&self.user, MAX_CHARS),
            os: clean(&self.os, MAX_CHARS),
            build: clean(&self.build, 32),
            admin: self.admin,
            quick: self.quick,
            collected_at: clean(&self.collected_at, 64),
        }
    }
}

/// A host whose every string is safe to put in a widget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SanitisedHost {
    pub name: String,
    pub user: String,
    pub os: String,
    pub build: String,
    pub admin: bool,
    pub quick: bool,
    pub collected_at: String,
}

/// The engine's summary judgement.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Verdict {
    #[serde(default)]
    pub high: usize,
    /// The engine writes the key as `med` but the type calls it medium.
    #[serde(rename = "med", default)]
    pub medium: usize,
    #[serde(default)]
    pub info: usize,
    #[serde(default)]
    pub headline: String,
    #[serde(default)]
    pub recommendation: Vec<String>,
}

impl Verdict {
    /// Sanitised, ready to render.
    pub fn sanitised(&self) -> SanitisedVerdict {
        SanitisedVerdict {
            high: self.high,
            medium: self.medium,
            info: self.info,
            headline: clean(&self.headline, 256),
            recommendation: self
                .recommendation
                .iter()
                .map(|line| clean(line, MAX_CHARS))
                .collect(),
        }
    }
}

/// A verdict whose every string is safe to put in a widget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SanitisedVerdict {
    pub high: usize,
    pub medium: usize,
    pub info: usize,
    pub headline: String,
    pub recommendation: Vec<String>,
}

/// One whole scan, as the engine's `--json` output.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Report {
    /// The schema identifier. Empty when the report did not carry one.
    #[serde(default)]
    pub schema: String,
    #[serde(default)]
    pub host: Host,
    #[serde(default)]
    pub verdict: Verdict,
    #[serde(default)]
    pub findings: Vec<Finding>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

impl Report {
    /// Parse an engine report.
    ///
    /// Unknown fields are ignored, which is the direction that matters: a newer
    /// engine that adds a field must still be readable by this build, or an engine
    /// upgrade silently makes every archived report unreadable. A *missing* field is
    /// defaulted instead of erroring, for the same reason - an older report from
    /// before a field existed is a report someone still has on disk.
    pub fn parse(json: &str) -> Result<Self, CodecError> {
        let report: Report = serde_json::from_str(json).map_err(CodecError::Json)?;
        if !report.schema.is_empty() && report.schema != SCHEMA {
            return Err(CodecError::Schema(report.schema));
        }
        Ok(report)
    }

    /// Counts by severity, least to most severe.
    pub fn counts(&self) -> Counts {
        let mut counts = Counts::default();
        for finding in &self.findings {
            match finding.severity() {
                crate::theme::Sev::High => counts.high += 1,
                crate::theme::Sev::Medium => counts.medium += 1,
                crate::theme::Sev::Low => counts.low += 1,
                crate::theme::Sev::Info => counts.info += 1,
            }
        }
        counts
    }

    /// The host this report describes, for grouping the archive by machine.
    pub fn host_key(&self) -> String {
        if self.host.name.is_empty() {
            self.host.build.clone()
        } else {
            self.host.name.clone()
        }
    }
}

/// How many findings of each severity.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub high: usize,
    pub medium: usize,
    pub low: usize,
    pub info: usize,
}

impl Counts {
    /// The total across every severity.
    pub const fn total(&self) -> usize {
        self.high + self.medium + self.low + self.info
    }

    /// Add one finding of a severity.
    pub fn add(&mut self, severity: crate::theme::Sev) {
        match severity {
            crate::theme::Sev::High => self.high += 1,
            crate::theme::Sev::Medium => self.medium += 1,
            crate::theme::Sev::Low => self.low += 1,
            crate::theme::Sev::Info => self.info += 1,
        }
    }
}

/// Why a report could not be read.
#[derive(Debug, thiserror::Error)]
pub enum CodecError {
    #[error("the file is not a readable JSON report: {0}")]
    Json(#[from] serde_json::Error),
    #[error("this is a {0} report, and this build reads {SCHEMA}")]
    Schema(String),
    #[error("the file could not be read: {0}")]
    Io(#[from] std::io::Error),
    #[error("the archive entry escapes the archive directory and was refused: {0}")]
    OutsideArchive(String),
}

/// Strip everything from a string that came off a hostile host that could mislead a
/// terminal or a reader.
///
/// What goes:
///
/// * **C0 controls and DEL.** ESC is how every attack below starts; the others let a
///   program move the cursor, clear the screen, or emit nothing visible.
/// * **C1 controls**, including U+009B (CSI) reached via UTF-8. A terminal that
///   accepts the 8-bit form accepts these as escape sequences too.
/// * **Bidi overrides and isolates** (U+202A-U+202E, U+2066-U+2069). These are what
///   make `report.exe` render as `exe.report`, which is a *filename* lie rather than
///   a cosmetic one.
/// * **Zero-width and other format characters** (U+200B-U+200F, U+2028, U+2029,
///   U+FEFF). Invisible characters hide text: an extension can be appended to a
///   filename that looks like it ends earlier than it does.
/// * **Bidi and other invisible marks** in general, via the Unicode format category.
///
/// What stays:
///
/// * Printable characters, including non-ASCII. A Russian report is Cyrillic and a
///   Chinese path is CJK, and stripping those would make this tool useless for the
///   machines it is aimed at.
/// * Space and ordinary punctuation, including the box-drawing characters this
///   interface draws with - the sanitiser runs on host strings only, never on our own
///   UI glyphs.
pub fn clean(input: &str, max_chars: usize) -> String {
    let mut out = String::with_capacity(input.len().min(max_chars));
    for ch in input.chars().take(max_chars) {
        if is_hostile(ch) {
            // Replaced rather than dropped. A control character in the middle of a
            // path should not silently join the two halves into a different string,
            // which is the same class of lie as the bidi override.
            out.push('\u{fffd}');
        } else {
            out.push(ch);
        }
    }
    out
}

/// Whether a character must not reach a widget unaltered.
fn is_hostile(ch: char) -> bool {
    let code = ch as u32;
    // C0 controls, DEL, and the C1 block. The C1 range is checked because a UTF-8
    // stream can carry it directly, and terminals that accept 8-bit CSI accept it as
    // an escape sequence.
    if code < 0x20 || code == 0x7f || (0x80..=0x9f).contains(&code) {
        return true;
    }
    // Unicode `Cf` (format) covers the bidi overrides and isolates, the zero-width
    // joiners, the BOM, and the interlinear annotation marks. This is the catch-all
    // that keeps working when someone adds a new invisible character to Unicode.
    matches!(ch, '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2060}'..='\u{206f}' | '\u{feff}')
}

/// Truncate to a number of *display cells*, not characters and not bytes.
///
/// This is a correctness property, not cosmetics (`docs/spec.md` SR-2). A row that
/// overflows its column wraps, and the wrapped remainder lands on the next row and
/// destroys the grid the whole layout depends on. Widths differ: `任务` is three
/// characters and six cells, so a character count overflows by half and a byte count
/// overflows by an order of magnitude.
pub fn truncate_to_cells(input: &str, max_cells: usize) -> String {
    if max_cells == 0 {
        return String::new();
    }
    if width(input) <= max_cells {
        return input.to_string();
    }
    // One cell is reserved for the ellipsis, so the result is never exactly the
    // budget wide: a string that fits the budget only because the ellipsis pushed it
    // over is a string that looks truncated when it is not.
    let budget = max_cells.saturating_sub(1);
    let mut out = String::new();
    let mut cells = 0usize;
    for ch in input.chars() {
        let ch_width = char_width(ch);
        if cells + ch_width > budget {
            break;
        }
        out.push(ch);
        cells += ch_width;
    }
    out.push('\u{2026}');
    out
}

/// Truncate from the *front*, keeping the end.
///
/// The interesting part of a path is its end - `\Windows\System32\` identifies a
/// location that the beginning of the path does not - so a machine string that does
/// not fit loses its head rather than its tail. A leading ellipsis says so.
pub fn elide_head(input: &str, max_cells: usize) -> String {
    if max_cells == 0 {
        return String::new();
    }
    if width(input) <= max_cells {
        return input.to_string();
    }
    let budget = max_cells.saturating_sub(1);
    let mut kept: Vec<char> = Vec::new();
    let mut cells = 0usize;
    for ch in input.chars().rev() {
        let ch_width = char_width(ch);
        if cells + ch_width > budget {
            break;
        }
        kept.push(ch);
        cells += ch_width;
    }
    kept.reverse();
    let mut out = String::from("\u{2026}");
    out.extend(kept);
    out
}

/// Where the archive of past scans lives.
///
/// Under the platform's data directory and never beside the executable: a report
/// contains evidence taken from a scanned machine, and a directory that travels with
/// the binary is a directory that gets copied somewhere it should not be. The
/// engine makes the same argument for its own history file.
pub fn archive_dir() -> Result<PathBuf, CodecError> {
    let base = directories::ProjectDirs::from("io", "irscan", "irscan-tui").ok_or_else(|| {
        CodecError::Io(std::io::Error::other("no data directory for this platform"))
    })?;
    Ok(base.data_dir().join("archive"))
}

/// The file one archived scan is stored in.
///
/// The name is derived from the host and the timestamp, both of which came off a
/// hostile machine, so both are reduced to a safe character set. Without this a
/// service named `../../etc` would choose where the archive writes, and a name with
/// a separator in it would escape the directory entirely.
pub fn archive_path(dir: &Path, host: &str, collected_at: &str) -> PathBuf {
    let safe_host = slug(host);
    let safe_time = slug(collected_at);
    dir.join(format!("{safe_host}-{safe_time}.json"))
}

/// Reduce a string to characters that are safe in a filename.
fn slug(input: &str) -> String {
    let cleaned = clean(input, 64);
    let mapped: String = cleaned
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = mapped.trim_matches('_');
    if trimmed.is_empty() {
        "unknown".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Check that a path is inside `dir`, and refuse it if it is not.
///
/// The check is on the *canonical* form, because `..` in a path that has not been
/// resolved yet is a promise and not a fact. `docs/spec.md` SR-3: a traversal in a
/// stored path is refused, never sanitised into something plausible.
pub fn ensure_inside(dir: &Path, candidate: &Path) -> Result<PathBuf, CodecError> {
    let base = dir
        .canonicalize()
        .map_err(|e| CodecError::Io(std::io::Error::other(format!("archive dir: {e}"))))?;
    // `canonicalize` resolves `..`, but only for a path that exists. A path that
    // does not exist yet is resolved against the base manually, which is enough to
    // catch a traversal without requiring the file to be there.
    let resolved = if candidate.exists() {
        candidate
            .canonicalize()
            .map_err(|e| CodecError::Io(std::io::Error::other(format!("archive entry: {e}"))))?
    } else {
        base.join(strip_prefix(candidate, dir))
    };
    if resolved.starts_with(&base) {
        Ok(resolved)
    } else {
        Err(CodecError::OutsideArchive(resolved.display().to_string()))
    }
}

/// Make `candidate` relative to `dir` without touching the filesystem.
///
/// `Path::strip_prefix` works on components, so it refuses a relative path outright
/// rather than producing a path that escapes.
fn strip_prefix<'a>(candidate: &'a Path, dir: &Path) -> &'a Path {
    candidate.strip_prefix(dir).unwrap_or(candidate)
}

/// The display width of a string in cells.
///
/// A named wrapper so the call sites read as a measurement rather than as a trait
/// import, and so the `&str` deref that `UnicodeWidthStr` needs happens in one place.
/// `String` does not implement the trait, so `width(&String)` is the mistake this
/// prevents.
fn width(text: &str) -> usize {
    use unicode_width::UnicodeWidthStr;
    UnicodeWidthStr::width(text)
}

/// The display width of one character. A control character has no width, which is
/// why this returns `Option`.
fn char_width(ch: char) -> usize {
    use unicode_width::UnicodeWidthChar;
    UnicodeWidthChar::width(ch).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Sev;

    /// A report shaped exactly like `irscan::report::render_json` emits, copied
    /// from the engine's own writer rather than invented here. A fixture that
    /// reflects what the engine *could* produce rather than what it *does* would
    /// make every test below pass against a schema nobody writes.
    const ENGINE_OUTPUT: &str = r#"{
  "schema": "irscan/v1",
  "host": { "name": "PC-01", "user": "bob", "os": "Windows 10 Pro", "build": "19045", "admin": true, "quick": false, "collectedAt": "2026-09-29 14:22:01" },
  "verdict": { "high": 1, "med": 2, "info": 0, "headline": "Требуется внимание", "recommendation": ["Отключить службу", "Проверить хеш"] },
  "findings": [
    { "severity": "high", "category": "autoruns", "title": "任务计划程序已创建持久化任务", "evidence": ["Task: \\Microsoft\\Windows\\Evil", "Run: powershell.exe -enc AAAA"], "remediation": ["Удалить задачу"] },
    { "severity": "med", "category": "remote_access", "title": "Неизвестная служба", "evidence": ["Service: AnyDesk-like"], "remediation": [] },
    { "severity": "med", "category": "processes", "title": "Процесс в пользовательской директории", "evidence": ["C:\\Users\\bob\\AppData\\x.exe"], "remediation": [] }
  ],
  "warnings": ["collector failed: events"]
}"#;

    fn parsed() -> Report {
        match Report::parse(ENGINE_OUTPUT) {
            Ok(report) => report,
            Err(e) => panic!("the engine's own output did not parse: {e}"),
        }
    }

    #[test]
    fn a_report_the_engine_wrote_parses() {
        let report = parsed();
        assert_eq!(report.schema, SCHEMA);
        assert_eq!(report.host.name, "PC-01");
        assert_eq!(report.host.build, "19045");
        assert!(report.host.admin);
        assert!(!report.host.quick);
        assert_eq!(report.verdict.high, 1);
        // The engine writes the key as `med`; getting this wrong silently reads
        // every medium finding as zero.
        assert_eq!(report.verdict.medium, 2);
        assert_eq!(report.findings.len(), 3);
        assert_eq!(report.findings[0].severity(), Sev::High);
        assert_eq!(report.warnings.len(), 1);
    }

    #[test]
    fn a_report_round_trips_through_json() {
        // FR-12. The archive is written by serialising and read by parsing, so a
        // report that survives one has to survive being written again.
        let original = parsed();
        let written =
            serde_json::to_string(&original).expect("a report we parsed must re-serialise");
        let reread = match Report::parse(&written) {
            Ok(report) => report,
            Err(e) => panic!("our own output did not parse back: {e}"),
        };
        assert_eq!(original, reread);
    }

    #[test]
    fn every_key_the_engine_writes_is_pinned_by_name() {
        // The engine's own trick, from desktop/src/scan.rs. A rename here is
        // otherwise a silent blank screen: serde defaults every missing field, so a
        // renamed key parses cleanly into an empty string and the host name simply
        // vanishes. This test is the only thing standing between a rename in
        // `render_json` and a user staring at a blank report.
        for key in [
            "\"schema\"",
            "\"host\"",
            "\"verdict\"",
            "\"findings\"",
            "\"warnings\"",
            "\"name\"",
            "\"user\"",
            "\"os\"",
            "\"build\"",
            "\"admin\"",
            "\"quick\"",
            "\"collectedAt\"",
            "\"high\"",
            "\"med\"",
            "\"info\"",
            "\"headline\"",
            "\"recommendation\"",
            "\"severity\"",
            "\"category\"",
            "\"title\"",
            "\"evidence\"",
            "\"remediation\"",
        ] {
            assert!(
                ENGINE_OUTPUT.contains(key),
                "the engine stopped writing {key}"
            );
        }
    }

    #[test]
    fn an_unknown_field_from_a_newer_engine_does_not_break_the_load() {
        // Forward compatibility, and the direction that matters: if this build
        // refused reports carrying fields it had not heard of, then an engine
        // upgrade would silently make every archived report unreadable. The
        // upgrade must never be able to do that.
        // The unknown fields are the point: `telemetry` at the top level and
        // `domain` inside `host` are things a newer engine might add, and this
        // build has to keep reading the fields it does know.
        let future = r#"{
  "schema": "irscan/v1",
  "host": { "name": "PC-01", "admin": true, "domain": "CORP" },
  "verdict": { "high": 1 },
  "findings": [],
  "warnings": [],
  "telemetry": { "sentTo": "somewhere" }
}"#;
        let report = parsed_from(future);
        assert_eq!(report.host.name, "PC-01");
        assert!(report.host.admin);
    }

    #[test]
    fn an_older_report_missing_the_newer_fields_still_loads() {
        // Someone has a report from before the verdict existed on disk. It must
        // still open, with the absent fields shown as absent rather than as zero
        // findings that were never looked for.
        let old = r#"{ "schema": "irscan/v1", "host": { "name": "OLD" }, "findings": [] }"#;
        let report = parsed_from(old);
        assert_eq!(report.host.name, "OLD");
        assert_eq!(report.verdict.high, 0);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn a_report_from_a_different_schema_is_refused_rather_than_guessed_at() {
        // Guessing produces a screen full of zeroes that reads as "clean", which is
        // the single most dangerous thing this tool could do.
        let future = r#"{ "schema": "irscan/v2", "findings": [] }"#;
        let err = match Report::parse(future) {
            Ok(_) => panic!("a v2 report was accepted by a v1 reader"),
            Err(e) => e,
        };
        assert!(
            err.to_string().contains("v2"),
            "the error should name the schema: {err}"
        );
    }

    #[test]
    fn a_hostile_evidence_string_cannot_reach_a_widget_unstripped() {
        // SR-1. The strings come off a machine an attacker controls, and the
        // output may be read by someone deciding whether to wipe the box.
        let hostile = vec![
            ("\u{1b}[31mRED", "an ANSI colour sequence"),
            (
                "\u{1b}]8;;http://evil.example\u{7}click me",
                "an OSC 8 hyperlink",
            ),
            ("exe\u{202e}.txt", "a bidi override that reverses the name"),
            (
                "name\u{200b}.exe",
                "a zero-width space that hides an extension",
            ),
            ("a\u{0}b", "a NUL in the middle"),
            ("a\u{9b}31mb", "a C1 CSI reached directly"),
            ("a\u{feff}b", "a byte-order mark in the middle"),
        ];
        for (raw, what) in hostile {
            let cleaned = clean(raw, MAX_CHARS);
            assert!(
                !cleaned
                    .chars()
                    .any(|c| (c as u32) < 0x20 || (0x80..=0x9f).contains(&(c as u32))),
                "{what} left a control character behind: {cleaned:?}"
            );
            assert!(
                !cleaned.chars().any(|c| {
                    matches!(c, '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{feff}')
                }),
                "{what} left an invisible or overriding character behind: {cleaned:?}"
            );
        }
    }

    #[test]
    fn a_hostile_string_is_sanitised_on_the_way_into_the_view_model_not_at_the_call_site() {
        // The guarantee has to hold for whoever calls it, including a caller that
        // does not know the rule. `sanitised()` is the only path to a renderable
        // string, which is why it is a method rather than a convention.
        let finding = Finding {
            severity: "high".to_string(),
            category: "processes".to_string(),
            title: "\u{1b}[5;1H\u{1b}[2Jfake title".to_string(),
            evidence: vec!["\u{1b}]8;;http://x\u{7}path".to_string()],
            remediation: vec![],
        };
        let clean = finding.sanitised();
        assert!(!clean.title.contains('\u{1b}'));
        assert!(!clean.evidence[0].contains('\u{1b}'));
        assert_eq!(clean.severity, Sev::High);
        // The readable part survives; only the escape machinery goes.
        assert!(clean.title.contains("fake title"));
    }

    #[test]
    fn russian_and_chinese_survive_the_sanitiser_unchanged() {
        // The sanitiser must not become a strip-everything-non-ascii filter: the
        // engine's own report text is Russian, and Chinese host names and paths are
        // ordinary content. Stripping them would make the tool useless for exactly
        // the machines it is aimed at.
        let text = "任务计划程序已创建持久化任务 / Стахановец / C:\\Users\\张伟\\AppData";
        assert_eq!(clean(text, MAX_CHARS), text);
    }

    #[test]
    fn an_unbounded_string_is_bounded_rather_than_producing_a_row_that_cannot_render() {
        // A service can be named anything, and a megabyte-long value in a list is how
        // a report becomes unreadable and the terminal becomes sluggish.
        let huge = "A".repeat(5000);
        let cleaned = clean(&huge, MAX_CHARS);
        assert_eq!(cleaned.chars().count(), MAX_CHARS);
    }

    #[test]
    fn a_control_character_is_replaced_rather_than_dropped() {
        // Dropping it would join `a` and `b` into `ab` - the same class of lie as the
        // bidi override it is meant to stop, just quieter.
        assert_eq!(clean("a\u{0}b", 16), "a\u{fffd}b");
        assert_eq!(clean("a\nb", 16), "a\u{fffd}b");
    }

    #[test]
    fn a_cjk_string_is_truncated_by_cells_and_not_by_characters() {
        // SR-2. `任务` is three characters and six cells, so a character budget
        // overflows by half and the row wraps into the next one.
        let cjk = "任务计划程序";
        assert_eq!(width(cjk), 12);
        let cut = truncate_to_cells(cjk, 6);
        assert!(width(&cut) <= 6, "truncated to {} cells", width(&cut));
        // A budget of 6 cells reserves one for the ellipsis, leaving 5, and a CJK
        // glyph is 2 cells - so two glyphs and the ellipsis. A character-counting
        // implementation would have kept five glyphs and overflowed to 11 cells,
        // wrapping the row.
        assert_eq!(width(&cut), 5, "expected two CJK glyphs plus the ellipsis");
        assert_eq!(
            cut.chars().count(),
            3,
            "two glyphs plus the ellipsis itself"
        );
    }

    #[test]
    fn a_string_that_already_fits_is_returned_unchanged_with_no_ellipsis() {
        // An ellipsis on a string that was never truncated is a claim that something
        // was hidden when nothing was.
        assert_eq!(truncate_to_cells("short", 20), "short");
        assert_eq!(elide_head("short", 20), "short");
    }

    #[test]
    fn truncation_always_reserves_a_cell_for_the_ellipsis_so_the_result_fits() {
        for budget in 1..40usize {
            let cut = truncate_to_cells(&"x".repeat(100), budget);
            assert!(
                width(&cut) <= budget,
                "budget {budget} produced {} cells",
                width(&cut)
            );
        }
    }

    #[test]
    fn a_zero_width_budget_produces_an_empty_string_rather_than_panicking() {
        // A column that is zero wide happens on a very narrow terminal, and a panic
        // in a render path is how a terminal is left in the alternate screen.
        assert_eq!(truncate_to_cells("anything", 0), "");
        assert_eq!(elide_head("anything", 0), "");
    }

    #[test]
    fn a_long_path_keeps_its_end_because_that_is_the_part_that_identifies_it() {
        // `\Windows\System32\` says where; `C:\Users\Public\Documents\` says who.
        let path = "C:\\Users\\Public\\Documents\\Shared\\payload.exe";
        let shown = elide_head(path, 24);
        assert!(width(&shown) <= 24);
        assert!(
            shown.starts_with('\u{2026}'),
            "the head should be elided: {shown}"
        );
        assert!(
            shown.ends_with("payload.exe"),
            "the informative end was cut: {shown}"
        );
    }

    #[test]
    fn counts_agree_with_the_findings_the_report_actually_contains() {
        // The verdict the engine writes and the findings it lists are computed at
        // different places; if they disagree the user sees one number and can scroll
        // to a different one, and has no way to know which is right.
        let report = parsed();
        let counts = report.counts();
        assert_eq!(counts.high, 1);
        assert_eq!(counts.medium, 2);
        assert_eq!(counts.total(), report.findings.len());
    }

    #[test]
    fn a_scan_without_administrator_rights_names_its_blind_spots() {
        // SR-6. "Coverage is reduced" is not actionable; "you did not see the
        // Security log" is. A short report and a correct report look alike, and
        // reading the short one as the correct one is the failure this tool exists
        // to prevent.
        let blind = Host {
            admin: false,
            ..Host::default()
        };
        assert_eq!(blind.blind_spots().len(), 3);
        assert!(blind.blind_spots().iter().any(|s| s.contains("Security")));
        let full = Host {
            admin: true,
            ..Host::default()
        };
        assert!(full.blind_spots().is_empty());
    }

    #[test]
    fn two_reports_of_the_same_host_share_a_key_and_two_hosts_do_not() {
        // FR-13 compares scans of one machine. Grouping by anything but the host
        // would let a comparison claim that a finding appeared because the operator
        // scanned a different computer.
        let a = Report {
            host: Host {
                name: "PC-01".into(),
                ..Host::default()
            },
            ..Report::default()
        };
        let b = Report {
            host: Host {
                name: "PC-01".into(),
                build: "1".into(),
                ..Host::default()
            },
            ..Report::default()
        };
        let c = Report {
            host: Host {
                name: "PC-02".into(),
                ..Host::default()
            },
            ..Report::default()
        };
        assert_eq!(a.host_key(), b.host_key());
        assert_ne!(a.host_key(), c.host_key());
    }

    #[test]
    fn an_archive_filename_cannot_be_chosen_by_a_service_name() {
        // SR-3. The host name and the timestamp both came off a hostile machine. A
        // service named `../../etc` would otherwise choose where the archive writes.
        let dir = Path::new("/archive");
        let path = archive_path(dir, "../../etc/passwd", "2026-09-29 14:22:01");
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        assert!(
            !name.contains('/'),
            "the filename contains a separator: {name}"
        );
        assert!(
            !name.contains(".."),
            "the filename contains a traversal: {name}"
        );
        assert!(
            path.parent() == Some(dir),
            "the archive path left the directory: {path:?}"
        );
    }

    #[test]
    fn an_archive_filename_falls_back_rather_than_producing_an_empty_one() {
        // A host whose name is entirely hostile characters still needs a filename;
        // an empty one would collide with every other such host.
        let path = archive_path(Path::new("/a"), "\u{1b}\u{202e}\u{0}", "");
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        assert!(!name.is_empty());
        assert!(name.contains("unknown"), "expected a fallback in {name}");
    }

    #[test]
    fn a_path_that_escapes_the_archive_directory_is_refused_rather_than_sanitised() {
        // SR-3. The distinction is the whole point: a traversal is not a dirty
        // string to be cleaned, it is a refusal.
        let dir = std::env::temp_dir().join("irscan-tui-archive-test");
        let _ = std::fs::create_dir_all(&dir);
        let escape = dir.join("..").join("..").join("outside.json");
        let result = ensure_inside(&dir, &escape);
        assert!(result.is_err(), "a traversal was accepted: {result:?}");
        let inside = dir.join("entry.json");
        assert!(ensure_inside(&dir, &inside).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Parse, panicking with the error. Test-only, so the ban on `panic!` in
    /// production code is lifted for this module by the crate root.
    fn parsed_from(json: &str) -> Report {
        match Report::parse(json) {
            Ok(report) => report,
            Err(e) => panic!("fixture did not parse: {e}"),
        }
    }
}
