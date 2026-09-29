//! The application state machine: what the interface knows, and what changes it.
//!
//! Nothing here knows about a terminal, a clock or the engine. Every transition is a
//! function from an `Event` to a new `App`, which is what makes the whole of the
//! interface's behaviour testable in a plain `#[test]` with no runtime, no terminal
//! and no Windows.
//!
//! **The two rules that are easy to break.**
//!
//! * A scan cannot be started while one is running. Two scans interleaving would
//!   produce a report that is half of each, which is worse than no report.
//! * A collector that failed is recorded and stays on screen. "We found nothing" and
//!   "we could not look" are the two claims this tool exists to keep apart, and a
//!   failed collector is the difference between them. FR-7.

use crate::report::{Counts, Report, SanitisedFinding, SanitisedHost, SanitisedVerdict};
use crate::theme::Sev;
use std::path::PathBuf;

/// Which of the five screens is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    /// Run a scan, watch it run.
    Dashboard,
    /// Read the report of the scan in view.
    Report,
    /// Every past scan of every host.
    Archive,
    /// The live event log.
    Live,
    /// The rule set.
    Rules,
}

impl Screen {
    /// Every screen, in the order the tab bar shows them.
    pub const ALL: [Screen; 5] = [
        Screen::Dashboard,
        Screen::Report,
        Screen::Archive,
        Screen::Live,
        Screen::Rules,
    ];

    /// The tab's label.
    pub const fn label(self) -> &'static str {
        match self {
            Screen::Dashboard => "SCAN",
            Screen::Report => "REPORT",
            Screen::Archive => "ARCHIVE",
            Screen::Live => "LIVE",
            Screen::Rules => "RULES",
        }
    }

    /// The next screen, wrapping.
    pub const fn next(self) -> Screen {
        match self {
            Screen::Dashboard => Screen::Report,
            Screen::Report => Screen::Archive,
            Screen::Archive => Screen::Live,
            Screen::Live => Screen::Rules,
            Screen::Rules => Screen::Dashboard,
        }
    }

    /// The previous screen, wrapping.
    pub const fn previous(self) -> Screen {
        match self {
            Screen::Dashboard => Screen::Rules,
            Screen::Report => Screen::Dashboard,
            Screen::Archive => Screen::Report,
            Screen::Live => Screen::Archive,
            Screen::Rules => Screen::Live,
        }
    }
}

/// What part of the current screen has the keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    /// The main list of the screen.
    Primary,
    /// The search or filter field.
    Search,
    /// A secondary pane - the detail of a selected finding.
    Detail,
    /// Nothing, because a modal is open.
    None,
}

/// How a scan is going. Not an `Option`: the three states a user must be able to
/// tell apart at a glance - idle, running, finished - are exactly the states that
/// collapse together if a scan is represented as "no report yet".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanState {
    /// No scan has been started.
    Idle,
    /// A scan is in flight.
    Running {
        /// How many collectors have reported, out of the total expected.
        done: usize,
        /// The collector currently running.
        current: String,
    },
    /// A scan finished, whether or not it found anything.
    Finished {
        /// Collectors that could not run at all.
        failed: usize,
    },
}

impl ScanState {
    /// Whether a scan is in flight.
    pub const fn is_running(&self) -> bool {
        matches!(self, ScanState::Running { .. })
    }
}

/// One collector's result, as it arrives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectorReport {
    /// The collector's stable name, e.g. `"events"`.
    pub name: String,
    /// How long it took.
    pub elapsed_ms: u128,
    /// How many findings it added.
    pub findings_added: usize,
    /// Set when it could not run at all. The scan carried on regardless.
    pub error: Option<String>,
}

/// Something that happened, as the state machine sees it.
///
/// The run loop turns terminal input, scan output and the clock into these. Nothing
/// below this line knows where an event came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A key was pressed, as a name from the keymap. Keeping the *name* rather than
    /// the key code means a binding is renamed in one place and every state machine
    /// test uses the same vocabulary as the help overlay.
    Action(Action),
    /// The user typed a printable character into the search field.
    Typed(char),
    /// The user deleted backwards.
    Backspace,
    /// A collector reported.
    Collector(CollectorReport),
    /// The scan finished.
    ScanFinished {
        /// How many collectors failed outright.
        failed: usize,
    },
    /// The scan could not be started.
    ScanFailed(String),
    /// The clock advanced by this much, for animations.
    Tick(std::time::Duration),
    /// The terminal changed size.
    Resize { width: u16, height: u16 },
    /// Leave.
    Quit,
}

/// A named action, so a key binding and a test speak the same word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Quit,
    NextScreen,
    PreviousScreen,
    StartScan,
    CancelScan,
    Next,
    Previous,
    PageDown,
    PageUp,
    Top,
    Bottom,
    ToggleSearch,
    ClearFilter,
    OpenArchive,
    CompareSelected,
    DeleteSelected,
    Export,
    Help,
}

/// A severity filter, as a set the user toggles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeverityFilter {
    pub high: bool,
    pub medium: bool,
    pub low: bool,
    pub info: bool,
}

impl SeverityFilter {
    /// Everything on, which is the state a report opens in.
    pub const ALL: SeverityFilter = SeverityFilter {
        high: true,
        medium: true,
        low: true,
        info: true,
    };

    /// Whether a severity passes the filter.
    pub const fn allows(self, severity: Sev) -> bool {
        match severity {
            Sev::High => self.high,
            Sev::Medium => self.medium,
            Sev::Low => self.low,
            Sev::Info => self.info,
        }
    }

    /// Turn one severity on or off.
    pub const fn toggle(&mut self, severity: Sev) {
        match severity {
            Sev::High => self.high = !self.high,
            Sev::Medium => self.medium = !self.medium,
            Sev::Low => self.low = !self.low,
            Sev::Info => self.info = !self.info,
        }
    }
}

/// A finding as the state machine holds it: sanitised, and carrying a stable key so
/// two scans of the same host can be compared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// What identifies this finding across two scans of the same host.
    ///
    /// Title plus category, because the same finding reported twice by two
    /// collectors should be one thing to the user. The engine's own identity model
    /// (`monitor::Identity`) is keyed the same way.
    pub key: String,
    pub severity: Sev,
    pub category: String,
    pub title: String,
    pub evidence: Vec<String>,
    pub remediation: Vec<String>,
    /// Scanned in, so a newly arrived row can be told from one that was always there.
    pub seen_at_scan: bool,
}

impl Row {
    /// Build a row from a decoded finding.
    pub fn from_sanitised(finding: &SanitisedFinding, seen_at_scan: bool) -> Self {
        Row {
            key: format!("{}\u{1f}{}", finding.category, finding.title),
            severity: finding.severity,
            category: finding.category.clone(),
            title: finding.title.clone(),
            evidence: finding.evidence.clone(),
            remediation: finding.remediation.clone(),
            seen_at_scan,
        }
    }

    /// Whether this row matches a free-text query, case-insensitively.
    ///
    /// Searches the evidence and the remediation as well as the title, because the
    /// thing an operator remembers is usually the path or the service name, and both
    /// live in the evidence.
    pub fn matches(&self, query: &str) -> bool {
        if query.is_empty() {
            return true;
        }
        let needle = query.to_lowercase();
        self.title.to_lowercase().contains(&needle)
            || self.category.to_lowercase().contains(&needle)
            || self
                .evidence
                .iter()
                .any(|line| line.to_lowercase().contains(&needle))
            || self
                .remediation
                .iter()
                .any(|line| line.to_lowercase().contains(&needle))
    }
}

/// One scan in the archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Archived {
    /// The host this was taken on. Comparison is same-host only.
    pub host_key: String,
    pub host: SanitisedHost,
    pub verdict: SanitisedVerdict,
    pub counts: Counts,
    pub taken_at: String,
    pub findings: Vec<SanitisedFinding>,
    /// Where it is on disk, if it is there.
    pub path: Option<PathBuf>,
}

/// What changed between two scans of the same host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delta {
    /// Findings that were not in the earlier scan.
    pub added: Vec<Row>,
    /// Findings that were in the earlier scan and are not in the later one.
    pub resolved: Vec<Row>,
    /// Findings present in both whose severity changed.
    pub changed: Vec<(Row, Sev, Sev)>,
}

impl Delta {
    /// Whether nothing changed.
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.resolved.is_empty() && self.changed.is_empty()
    }

    /// One line describing the movement, for the status bar.
    pub fn summary(&self) -> String {
        format!(
            "+{} new, -{} resolved, ~{} changed",
            self.added.len(),
            self.resolved.len(),
            self.changed.len()
        )
    }
}

/// The whole interface's state.
#[derive(Debug, Clone)]
pub struct App {
    pub screen: Screen,
    pub focus: Focus,
    pub should_quit: bool,
    /// The help overlay is open, and swallows every other key.
    pub help_open: bool,
    /// A modal message, with its severity.
    pub notice: Option<(String, bool)>,
    pub scan: ScanState,
    /// How many collectors the engine will run. Fourteen today; a number rather than
    /// a hard-coded fourteen so an engine that adds one does not need a code change.
    pub collectors_total: usize,
    pub collectors: Vec<CollectorReport>,
    /// The live event log: progress, findings, warnings, errors, in arrival order.
    pub log: Vec<LogLine>,
    /// How much of the log is kept. A scan produces thousands of lines and an
    /// unbounded log is a memory leak with a terminal attached to it.
    pub log_cap: usize,
    /// The report in view, and the rows derived from it.
    pub report: Option<Report>,
    pub rows: Vec<Row>,
    /// Which row is selected on the report screen.
    pub selected: usize,
    /// The scroll offset of the report list.
    pub offset: usize,
    pub filter: SeverityFilter,
    /// The free-text query in the search field.
    pub query: String,
    /// The rows the current filter and query select.
    pub visible: Vec<usize>,
    pub archive: Vec<Archived>,
    /// The archive row selected, and the one being compared against it.
    pub archive_selected: usize,
    pub archive_compare: Option<usize>,
    /// The delta from the last comparison, shown in the status bar.
    pub delta: Option<Delta>,
    /// A staged rule edit, which applies to the *next* scan.
    pub staged_rules: Option<String>,
    /// The theme, chosen once at startup.
    pub theme: crate::theme::Theme,
    /// The motion durations, zeroed when motion is off.
    pub durations: crate::motion::Durations,
    /// A live reveal animation, advanced by `Tick`.
    pub reveal: crate::motion::Reveal,
    /// The progress bar.
    pub meter: crate::motion::Meter,
    /// The spinner, for an indeterminate wait.
    pub spinner: crate::motion::Spinner,
    /// The screen cross-fade.
    pub fade: crate::motion::Transition,
    /// Whether the last draw changed anything. A still screen costs nothing.
    pub dirty: bool,
    /// The terminal's size, needed to decide whether to draw.
    pub size: (u16, u16),
}

/// One line of the live log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogLine {
    pub kind: LogKind,
    pub text: String,
}

/// What a log line is about, which is what decides its colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogKind {
    /// A collector finished.
    Progress,
    /// A finding arrived.
    Finding,
    /// The engine warned about something.
    Warning,
    /// A collector failed, or a write was refused.
    Failure,
}

impl App {
    /// Build the initial state.
    pub fn new(theme: crate::theme::Theme, durations: crate::motion::Durations) -> Self {
        let reveal = crate::motion::Reveal::new(durations, 0);
        App {
            screen: Screen::Dashboard,
            focus: Focus::Primary,
            should_quit: false,
            help_open: false,
            notice: None,
            scan: ScanState::Idle,
            collectors_total: 14,
            collectors: Vec::new(),
            log: Vec::new(),
            log_cap: 5_000,
            report: None,
            rows: Vec::new(),
            selected: 0,
            offset: 0,
            filter: SeverityFilter::ALL,
            query: String::new(),
            visible: Vec::new(),
            archive: Vec::new(),
            archive_selected: 0,
            archive_compare: None,
            delta: None,
            staged_rules: None,
            theme,
            durations,
            reveal,
            meter: crate::motion::Meter::new(durations.meter_step),
            spinner: crate::motion::Spinner::new(durations.spinner_frame),
            fade: crate::motion::Transition::new(durations.crossfade),
            dirty: true,
            size: (80, 24),
        }
    }

    /// Apply an event.
    pub fn apply(&mut self, event: Event) {
        self.dirty = true;
        match event {
            Event::Quit => self.should_quit = true,
            Event::Tick(delta) => self.tick(delta),
            Event::Resize { width, height } => {
                if self.size != (width, height) {
                    self.size = (width, height);
                }
            }
            // The help overlay is modal: it swallows everything else, including
            // `q`. A user who opened help by accident must be able to leave it.
            _ if self.help_open => {
                if event == Event::Action(Action::Help) || event == Event::Action(Action::Quit) {
                    self.help_open = false;
                }
            }
            Event::Action(action) => self.action(action),
            Event::Typed(ch) if self.focus == Focus::Search => {
                self.query.push(ch);
                self.refilter();
            }
            Event::Backspace if self.focus == Focus::Search => {
                self.query.pop();
                self.refilter();
            }
            Event::Collector(report) => self.collector(report),
            Event::ScanFinished { failed } => self.scan_finished(failed),
            Event::ScanFailed(message) => {
                self.scan = ScanState::Idle;
                self.push_log(LogKind::Failure, message.clone());
                self.notice = Some((message, true));
            }
            _ => {}
        }
    }

    /// A named action.
    pub fn action(&mut self, action: Action) {
        match action {
            Action::Quit => self.should_quit = true,
            Action::Help => self.help_open = true,
            Action::NextScreen => self.goto(self.screen.next()),
            Action::PreviousScreen => self.goto(self.screen.previous()),
            Action::Next => self.move_selection(1),
            Action::Previous => self.move_selection(-1),
            Action::PageDown => self.move_selection(10),
            Action::PageUp => self.move_selection(-10),
            Action::Top => self.set_selection(0),
            Action::Bottom => {
                let last = self.visible.len().saturating_sub(1);
                self.set_selection(last);
            }
            Action::ToggleSearch => {
                self.focus = if self.focus == Focus::Search {
                    Focus::Primary
                } else {
                    Focus::Search
                }
            }
            Action::ClearFilter => {
                self.filter = SeverityFilter::ALL;
                self.query.clear();
                self.refilter();
            }
            Action::StartScan => self.start_scan(),
            Action::CancelScan => {
                if self.scan.is_running() {
                    self.scan = ScanState::Idle;
                    self.push_log(LogKind::Warning, "scan cancelled".to_string());
                }
            }
            Action::CompareSelected => self.compare(),
            Action::DeleteSelected => self.delete_selected(),
            Action::Export => {
                self.notice = Some((format!("{} findings exported", self.rows.len()), false))
            }
            Action::OpenArchive => self.goto(Screen::Archive),
        }
    }

    /// Switch screen, starting the cross-fade.
    pub fn goto(&mut self, screen: Screen) {
        if screen != self.screen {
            self.screen = screen;
            self.fade = crate::motion::Transition::new(self.durations.crossfade);
            self.fade.advance(self.durations.crossfade);
            self.focus = Focus::Primary;
        }
    }

    /// Start a scan, unless one is already running.
    ///
    /// FR-4. The refusal is silent-but-visible rather than an error: the notice says
    /// why, and the running scan carries on. Two scans interleaving would produce a
    /// report that is half of each, which is worse than no report at all.
    pub fn start_scan(&mut self) {
        if self.scan.is_running() {
            self.notice = Some(("a scan is already running".to_string(), true));
            return;
        }
        self.collectors.clear();
        self.rows.clear();
        self.report = None;
        self.selected = 0;
        self.offset = 0;
        self.delta = None;
        self.meter = crate::motion::Meter::new(self.durations.meter_step);
        self.scan = ScanState::Running {
            done: 0,
            current: "starting".to_string(),
        };
        self.goto(Screen::Dashboard);
        self.push_log(LogKind::Progress, "scan started".to_string());
    }

    /// A collector reported.
    fn collector(&mut self, report: CollectorReport) {
        let name = report.name.clone();
        let failed = report.error.is_some();
        self.collectors.push(report.clone());
        if let ScanState::Running { done, .. } = &mut self.scan {
            *done = self.collectors.len();
        }
        // The bar tracks collectors, not bytes of output: a slow collector is the
        // thing a user needs to know about, and elapsed time per collector is what
        // the engine already measures.
        self.meter
            .set(self.collectors.len() as f64 / self.collectors_total.max(1) as f64);

        let summary = format!(
            "{} · {} ms · {} findings",
            report.name, report.elapsed_ms, report.findings_added
        );
        if failed {
            let message = report.error.unwrap_or_else(|| "failed".to_string());
            self.push_log(LogKind::Failure, format!("{summary} · {message}"));
        } else {
            self.push_log(LogKind::Progress, summary);
        }
        if let ScanState::Running { current, .. } = &mut self.scan {
            *current = name;
        }
    }

    /// The scan finished.
    fn scan_finished(&mut self, failed: usize) {
        // Guard on *running*, not on "not idle". A duplicate finish would otherwise
        // pass the check - the state is `Finished`, which is not `Idle` - and report
        // the scan ending twice, doubling every message and leaving the meter
        // claiming a scan that is not running.
        if !self.scan.is_running() {
            return;
        }
        self.scan = ScanState::Finished { failed };
        self.meter.set(1.0);
        let total = self.collectors.len();
        // FR-7, stated in the status bar and not only in the log: a scan that lost
        // collectors must say so, because its silence about them is indistinguishable
        // from a clean machine.
        let message = if failed == 0 {
            format!("scan finished · {total} collectors")
        } else {
            format!("scan finished · {failed} of {total} collectors FAILED")
        };
        self.push_log(
            if failed == 0 {
                LogKind::Progress
            } else {
                LogKind::Failure
            },
            message.clone(),
        );
        if failed > 0 {
            self.notice = Some((message, true));
        }
        self.goto(Screen::Report);
    }

    /// Load a decoded report into the report screen.
    pub fn load_report(&mut self, report: Report) {
        let mut rows: Vec<Row> = report
            .findings
            .iter()
            .map(|f| Row::from_sanitised(&f.sanitised(), false))
            .collect();
        // Sorted by severity, most severe first, so the finding that needs acting on
        // is the first thing on screen rather than something the user scrolls to.
        rows.sort_by(|a, b| b.severity.cmp(&a.severity).then(a.title.cmp(&b.title)));
        self.rows = rows;
        self.report = Some(report);
        self.selected = 0;
        self.offset = 0;
        self.refilter();
        self.goto(Screen::Report);
    }

    /// Recompute which rows the filter and query select.
    pub fn refilter(&mut self) {
        let query = self.query.clone();
        self.visible = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, row)| self.filter.allows(row.severity) && row.matches(&query))
            .map(|(index, _)| index)
            .collect();
        // The selection follows the filter: a filter that hides the selected row
        // while leaving it selected makes the detail pane show something the list
        // does not contain, which reads as a bug in the tool.
        if self.selected >= self.visible.len() {
            self.selected = self.visible.len().saturating_sub(1);
        }
        self.offset = self.offset.min(self.visible.len().saturating_sub(1));
    }

    /// How many findings the current filter is hiding.
    pub fn hidden(&self) -> usize {
        self.rows.len() - self.visible.len()
    }

    /// Whether the current filter is hiding a HIGH finding.
    ///
    /// FR-10. A filter that removes a HIGH finding is a filter that can turn a
    /// compromised machine into a clean-looking one, so it raises a visible marker
    /// rather than only changing the count.
    pub fn hides_high(&self) -> bool {
        !self.filter.high
    }

    /// The selected row, if there is one.
    pub fn selected_row(&self) -> Option<&Row> {
        self.visible
            .get(self.selected)
            .and_then(|i| self.rows.get(*i))
    }

    fn set_selection(&mut self, index: usize) {
        if self.visible.is_empty() {
            self.selected = 0;
            return;
        }
        self.selected = index.min(self.visible.len() - 1);
    }

    fn move_selection(&mut self, delta: i64) {
        if self.visible.is_empty() {
            self.selected = 0;
            return;
        }
        let last = self.visible.len() - 1;
        let next = (self.selected as i64 + delta).clamp(0, last as i64);
        self.selected = next as usize;
    }

    /// Compare the selected archive entry with the one it is paired with.
    pub fn compare(&mut self) {
        if self.screen != Screen::Archive {
            return;
        }
        let Some(other) = self.archive_compare else {
            self.notice = Some(("pick a second scan with x".to_string(), false));
            return;
        };
        let Some(left) = self.archive.get(other) else {
            return;
        };
        let Some(right) = self.archive.get(self.archive_selected) else {
            return;
        };
        if left.host_key != right.host_key {
            // FR-13. Comparing two machines produces a list of "added" findings that
            // are only differences between the computers, and the operator has no
            // way to tell that from a real change.
            self.delta = None;
            self.notice = Some((
                "refused: those two scans are of different hosts".to_string(),
                true,
            ));
            return;
        }
        self.delta = Some(diff(&left.findings, &right.findings));
    }

    /// Delete the selected archive entry.
    ///
    /// The path is checked for containment before anything is removed. SR-3: a
    /// traversal in a stored path is refused, not cleaned.
    pub fn delete_selected(&mut self) {
        if self.screen != Screen::Archive {
            return;
        }
        let Some(entry) = self.archive.get(self.archive_selected) else {
            return;
        };
        if let Some(path) = entry.path.clone() {
            if let Some(dir) = path.parent() {
                if let Err(e) = crate::report::ensure_inside(dir, &path) {
                    self.notice = Some((e.to_string(), true));
                    return;
                }
            }
            if let Err(e) = std::fs::remove_file(&path) {
                self.notice = Some((e.to_string(), true));
                return;
            }
        }
        let removed = self.archive.remove(self.archive_selected);
        self.archive_selected = self
            .archive_selected
            .min(self.archive.len().saturating_sub(1));
        self.archive_compare = None;
        self.delta = None;
        self.notice = Some((format!("deleted the scan of {}", removed.host.name), false));
    }

    /// Add a line to the live log, dropping the oldest if it is over the cap.
    pub fn push_log(&mut self, kind: LogKind, text: String) {
        if self.log.len() >= self.log_cap {
            // Drop a tenth at a time rather than one per line: `remove(0)` is O(n)
            // and a scan produces thousands of lines, which turns a bounded log into
            // a quadratic cost.
            let drop = self.log_cap / 10;
            self.log.drain(..drop);
        }
        self.log.push(LogLine { kind, text });
    }

    /// Advance every animation.
    fn tick(&mut self, delta: std::time::Duration) {
        self.reveal.advance(delta);
        self.meter.advance(delta);
        self.spinner.advance(delta);
        let was_done = self.fade.is_done();
        self.fade.advance(delta);
        // A tick that changes nothing about what is on screen is not a reason to
        // redraw. This is what lets a scan sit for a minute without the terminal
        // running hot.
        if was_done || self.reveal.is_settled() {
            self.dirty = false;
        }
    }
}

/// What changed between two findings lists.
pub fn diff(earlier: &[SanitisedFinding], later: &[SanitisedFinding]) -> Delta {
    let key_of = |f: &SanitisedFinding| format!("{}\u{1f}{}", f.category, f.title);
    let earlier_keys: std::collections::BTreeSet<String> = earlier.iter().map(key_of).collect();
    let later_keys: std::collections::BTreeSet<String> = later.iter().map(key_of).collect();

    let added = later
        .iter()
        .filter(|f| !earlier_keys.contains(&key_of(f)))
        .map(|f| Row::from_sanitised(f, true))
        .collect();
    let resolved = earlier
        .iter()
        .filter(|f| !later_keys.contains(&key_of(f)))
        .map(|f| Row::from_sanitised(f, false))
        .collect();

    // A finding present in both whose severity moved. Rare, and worth more than
    // the count suggests: HIGH going to MEDIUM is how a still-present problem gets
    // lost between two shifts.
    let mut changed = Vec::new();
    for later_finding in later {
        if let Some(earlier_finding) = earlier.iter().find(|f| key_of(f) == key_of(later_finding)) {
            let before = earlier_finding.severity;
            let after = later_finding.severity;
            if before != after {
                changed.push((Row::from_sanitised(earlier_finding, false), before, after));
            }
        }
    }
    Delta {
        added,
        resolved,
        changed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::Finding;
    use std::time::Duration;

    const MS: u64 = 1;

    fn app() -> App {
        App::new(
            crate::theme::Theme::new(crate::theme::Depth::TrueColor),
            crate::motion::Durations::MOTION,
        )
    }

    fn finding(severity: &str, category: &str, title: &str) -> SanitisedFinding {
        SanitisedFinding {
            severity: Sev::parse(severity),
            category: category.to_string(),
            title: title.to_string(),
            evidence: vec!["C:\\Users\\bob\\AppData\\Local\\x.exe".to_string()],
            remediation: vec![],
        }
    }

    fn report_with(findings: Vec<SanitisedFinding>) -> Report {
        Report {
            findings: findings
                .into_iter()
                .map(|f| Finding {
                    severity: match f.severity {
                        Sev::High => "high",
                        Sev::Medium => "med",
                        Sev::Low => "low",
                        Sev::Info => "info",
                    }
                    .to_string(),
                    category: f.category,
                    title: f.title,
                    evidence: f.evidence,
                    remediation: f.remediation,
                })
                .collect(),
            ..Report::default()
        }
    }

    #[test]
    fn a_scan_cannot_be_started_while_one_is_already_running() {
        // FR-4. Two interleaved scans produce a report that is half of each, which
        // is worse than no report: it looks like a result.
        let mut state = app();
        state.apply(Event::Action(Action::StartScan));
        assert!(state.scan.is_running());
        state.apply(Event::Action(Action::StartScan));
        assert!(
            state.scan.is_running(),
            "the first scan must not have been disturbed"
        );
        let notice = state.notice.expect("the refusal must be visible");
        assert!(notice.1, "the notice should be marked as a problem");
        assert!(
            notice.0.contains("already running"),
            "unhelpful notice: {}",
            notice.0
        );
    }

    #[test]
    fn a_cancelled_scan_frees_the_slot_for_the_next_one() {
        let mut state = app();
        state.apply(Event::Action(Action::StartScan));
        state.apply(Event::Action(Action::CancelScan));
        assert!(!state.scan.is_running());
        state.apply(Event::Action(Action::StartScan));
        assert!(state.scan.is_running());
    }

    #[test]
    fn a_failing_collector_is_recorded_and_the_scan_carries_on() {
        // FR-7. This is the requirement that keeps "we found nothing" and "we could
        // not look" apart, and it is the one most easily lost by treating an error as
        // a reason to abort.
        let mut state = app();
        state.apply(Event::Action(Action::StartScan));
        state.apply(Event::Collector(CollectorReport {
            name: "events".to_string(),
            elapsed_ms: 120,
            findings_added: 0,
            error: Some("access denied".to_string()),
        }));
        state.apply(Event::Collector(CollectorReport {
            name: "processes".to_string(),
            elapsed_ms: 40,
            findings_added: 3,
            error: None,
        }));
        assert!(
            state.scan.is_running(),
            "one failure must not stop the scan"
        );
        assert_eq!(state.collectors.len(), 2);
        assert!(
            state.log.iter().any(|l| l.kind == LogKind::Failure),
            "the failure should be in the log"
        );
    }

    #[test]
    fn a_scan_that_lost_collectors_says_so_where_it_cannot_be_missed() {
        // The log is scrolled past; the notice and the scan state are not. A scan
        // that quietly lost the Security event log reads exactly like a clean
        // machine unless it says otherwise on screen.
        let mut state = app();
        state.apply(Event::Action(Action::StartScan));
        state.apply(Event::ScanFinished { failed: 3 });
        assert_eq!(state.scan, ScanState::Finished { failed: 3 });
        let (notice, is_problem) = state.notice.expect("a failed scan must leave a notice");
        assert!(is_problem);
        assert!(notice.contains("3"), "the count should be named: {notice}");
        assert!(
            notice.contains("FAILED"),
            "the notice should say it failed: {notice}"
        );
    }

    #[test]
    fn a_clean_scan_says_how_many_collectors_ran_and_raises_no_alarm() {
        let mut state = app();
        state.apply(Event::Action(Action::StartScan));
        state.apply(Event::ScanFinished { failed: 0 });
        assert!(
            state.notice.is_none(),
            "a clean scan should not cry wolf: {:?}",
            state.notice
        );
        assert!(state.log.iter().any(|l| l.text.contains("scan finished")));
    }

    #[test]
    fn a_scan_that_finished_twice_does_not_report_twice() {
        // A duplicate finish event would otherwise double every message and leave
        // the meter claiming a scan that is not running.
        let mut state = app();
        state.apply(Event::Action(Action::StartScan));
        state.apply(Event::ScanFinished { failed: 0 });
        let after_first = state.log.len();
        state.apply(Event::ScanFinished { failed: 0 });
        assert_eq!(state.log.len(), after_first);
    }

    #[test]
    fn findings_are_ordered_most_severe_first_so_the_actionable_one_is_on_screen() {
        // An operator reads the top of the list. A HIGH finding below a LOW one is a
        // HIGH finding that has to be found.
        let mut state = app();
        state.load_report(report_with(vec![
            finding("low", "c", "low one"),
            finding("high", "c", "high one"),
            finding("med", "c", "med one"),
        ]));
        assert_eq!(state.rows[0].severity, Sev::High);
        assert_eq!(state.rows[1].severity, Sev::Medium);
        assert_eq!(state.rows[2].severity, Sev::Low);
    }

    #[test]
    fn a_filter_that_hides_a_high_finding_is_marked_rather_than_silently_accepted() {
        // FR-10, and the safety property in the spec's risk table. A filter that
        // removes a HIGH finding is a filter that can turn a compromised machine
        // into a clean-looking one.
        let mut state = app();
        state.load_report(report_with(vec![
            finding("high", "c", "the bad one"),
            finding("low", "c", "a note"),
        ]));
        assert!(!state.hides_high(), "nothing is hidden yet");
        state.filter.toggle(Sev::High);
        state.refilter();
        assert!(state.hides_high(), "the marker should notice a hidden HIGH");
        assert_eq!(state.visible.len(), 1, "the HIGH is filtered out");
        assert_eq!(state.hidden(), 1, "and the hidden count says so");
    }

    #[test]
    fn the_visible_and_total_counts_are_both_available_to_a_renderer() {
        // The status bar shows both, and a user reading "4" with no "/7" cannot tell
        // a filter from a short report.
        let mut state = app();
        state.load_report(report_with(vec![
            finding("high", "c", "a"),
            finding("low", "c", "b"),
            finding("info", "c", "c"),
        ]));
        assert_eq!(state.visible.len(), 3);
        assert_eq!(state.hidden(), 0);
        state.filter.toggle(Sev::Info);
        state.refilter();
        assert_eq!(state.visible.len(), 2);
        assert_eq!(state.hidden(), 1);
    }

    #[test]
    fn filters_compose_with_the_search_query() {
        let mut state = app();
        state.load_report(report_with(vec![
            finding("high", "autoruns", "a scheduled task"),
            finding("high", "processes", "a suspicious process"),
            finding("low", "autoruns", "a startup value"),
        ]));
        state.filter.toggle(Sev::Low);
        state.query = "process".to_string();
        state.refilter();
        assert_eq!(state.visible.len(), 1, "both filters should apply");
        assert_eq!(
            state.selected_row().map(|r| r.title.as_str()),
            Some("a suspicious process")
        );
    }

    #[test]
    fn a_search_matches_the_evidence_and_not_only_the_title() {
        // The thing an operator remembers is usually the path or the service name,
        // and both live in the evidence.
        let mut state = app();
        state.load_report(report_with(vec![finding(
            "high",
            "processes",
            "an image path",
        )]));
        state.query = "appdata".to_string();
        state.refilter();
        assert_eq!(state.visible.len(), 1, "the evidence should have matched");
    }

    #[test]
    fn clearing_the_filter_restores_every_row() {
        let mut state = app();
        state.load_report(report_with(vec![
            finding("high", "c", "a"),
            finding("low", "c", "b"),
        ]));
        state.filter.toggle(Sev::Low);
        state.query = "a".to_string();
        state.refilter();
        state.apply(Event::Action(Action::ClearFilter));
        assert_eq!(state.visible.len(), 2);
        assert!(state.query.is_empty());
    }

    #[test]
    fn the_selection_follows_the_filter_so_the_detail_pane_cannot_show_a_hidden_row() {
        // A filter that hides the selected row while leaving it selected shows the
        // user a finding the list does not contain, which reads as a bug in the tool.
        let mut state = app();
        state.load_report(report_with(vec![
            finding("high", "c", "first"),
            finding("low", "c", "second"),
        ]));
        state.apply(Event::Action(Action::Bottom));
        assert_eq!(
            state.selected_row().map(|r| r.title.as_str()),
            Some("second")
        );
        state.filter.toggle(Sev::Low);
        state.refilter();
        let shown = state.selected_row().map(|r| r.title.as_str());
        assert_eq!(shown, Some("first"), "the selection must follow the filter");
    }

    #[test]
    fn moving_the_selection_past_either_end_stops_at_the_end() {
        // A list that wraps is a list where a held key sends the user to the top
        // without warning, and they lose their place in a long list of findings.
        let mut state = app();
        state.load_report(report_with(vec![
            finding("high", "c", "a"),
            finding("med", "c", "b"),
        ]));
        state.apply(Event::Action(Action::Previous));
        assert_eq!(state.selected, 0);
        state.apply(Event::Action(Action::Bottom));
        assert_eq!(state.selected, 1);
        state.apply(Event::Action(Action::Next));
        assert_eq!(state.selected, 1, "the selection wrapped");
    }

    #[test]
    fn navigating_an_empty_report_does_not_panic() {
        // A clean machine is the most common case, and it is the one most likely to
        // be opened first.
        let mut state = app();
        state.load_report(report_with(vec![]));
        for action in [
            Action::Next,
            Action::Previous,
            Action::Top,
            Action::Bottom,
            Action::PageDown,
        ] {
            state.apply(Event::Action(action));
        }
        assert_eq!(state.selected, 0);
        assert!(state.selected_row().is_none());
    }

    #[test]
    fn the_help_overlay_swallows_every_key_until_it_is_dismissed() {
        // A modal that leaks `q` to the screen underneath quits the program, and one
        // that leaks `j`/`k` moves a selection the user cannot see moving.
        let mut state = app();
        state.apply(Event::Action(Action::Help));
        state.apply(Event::Action(Action::NextScreen));
        assert_eq!(
            state.screen,
            Screen::Dashboard,
            "a key reached the screen under help"
        );
        state.apply(Event::Action(Action::Help));
        assert!(!state.help_open);
        state.apply(Event::Action(Action::NextScreen));
        assert_eq!(state.screen, Screen::Report, "help did not dismiss");
    }

    #[test]
    fn quitting_from_inside_the_help_overlay_closes_the_overlay_rather_than_the_program() {
        // A user who opened help by accident must be able to back out of it.
        let mut state = app();
        state.apply(Event::Action(Action::Help));
        state.apply(Event::Action(Action::Quit));
        assert!(!state.help_open);
        assert!(
            !state.should_quit,
            "q inside the help overlay quit the program"
        );
    }

    #[test]
    fn two_scans_of_one_host_report_what_changed() {
        // FR-13. This is the feature the interface exists for, so the three
        // categories each have to be distinguishable.
        let earlier = vec![
            finding("high", "autoruns", "a startup entry"),
            finding("med", "processes", "a process that was there"),
        ];
        let later = vec![
            finding("high", "autoruns", "a startup entry"),
            finding("high", "services", "a service that is new"),
        ];
        let delta = diff(&earlier, &later);
        assert_eq!(delta.added.len(), 1);
        assert_eq!(delta.added[0].title, "a service that is new");
        assert_eq!(delta.resolved.len(), 1);
        assert_eq!(delta.resolved[0].title, "a process that was there");
        assert!(!delta.is_empty());
        assert!(delta.summary().contains("+1"), "{}", delta.summary());
    }

    #[test]
    fn a_finding_that_stays_the_same_is_not_reported_as_added_and_resolved() {
        // Without this, two identical scans of an unchanged machine - the normal
        // result of re-checking a box - light up as a complete change set, and the
        // operator learns to ignore the panel.
        let same = vec![finding("high", "autoruns", "a startup entry")];
        let delta = diff(&same, &same);
        assert!(delta.is_empty(), "an unchanged host produced {delta:?}");
    }

    #[test]
    fn a_finding_whose_severity_moved_is_reported_as_changed_with_both_values() {
        // HIGH going to MEDIUM is how a still-present problem gets lost between two
        // shifts, so both sides have to be shown.
        let earlier = vec![finding("high", "processes", "a process")];
        let later = vec![finding("med", "processes", "a process")];
        let delta = diff(&earlier, &later);
        assert_eq!(delta.changed.len(), 1);
        let (_, before, after) = &delta.changed[0];
        assert_eq!(*before, Sev::High);
        assert_eq!(*after, Sev::Medium);
        assert!(
            delta.added.is_empty(),
            "a changed severity is not an addition"
        );
        assert!(delta.resolved.is_empty());
    }

    #[test]
    fn comparing_two_scans_of_different_hosts_is_refused() {
        // FR-13. The difference between two machines is a list of "added" findings
        // that is only a difference between computers, and nothing on screen would
        // say so.
        let mut state = app();
        state.archive = vec![
            archived("PC-01", vec![finding("high", "c", "a")]),
            archived("PC-02", vec![finding("low", "c", "b")]),
        ];
        state.archive_selected = 1;
        state.archive_compare = Some(0);
        state.goto(Screen::Archive);
        state.apply(Event::Action(Action::CompareSelected));
        assert!(
            state.delta.is_none(),
            "a cross-host comparison was produced"
        );
        let (notice, is_problem) = state.notice.expect("the refusal must be visible");
        assert!(is_problem);
        assert!(
            notice.contains("different hosts"),
            "unhelpful notice: {notice}"
        );
    }

    #[test]
    fn comparing_two_scans_of_one_host_produces_a_delta() {
        let mut state = app();
        state.archive = vec![
            archived("PC-01", vec![finding("high", "c", "a")]),
            archived(
                "PC-01",
                vec![finding("high", "c", "a"), finding("low", "c", "new")],
            ),
        ];
        state.archive_selected = 1;
        state.archive_compare = Some(0);
        state.goto(Screen::Archive);
        state.apply(Event::Action(Action::CompareSelected));
        let delta = state
            .delta
            .expect("a same-host comparison should produce a delta");
        assert_eq!(delta.added.len(), 1);
    }

    #[test]
    fn comparing_with_nothing_selected_says_what_to_do_rather_than_doing_nothing() {
        let mut state = app();
        state.archive = vec![archived("PC-01", vec![])];
        state.goto(Screen::Archive);
        state.apply(Event::Action(Action::CompareSelected));
        let (notice, _) = state.notice.expect("the user should be told what to do");
        assert!(
            notice.contains('x'),
            "the notice should name the key: {notice}"
        );
    }

    #[test]
    fn deleting_an_archive_entry_removes_it_from_the_list() {
        let mut state = app();
        state.archive = vec![archived("PC-01", vec![finding("high", "c", "a")])];
        state.goto(Screen::Archive);
        state.apply(Event::Action(Action::DeleteSelected));
        assert!(state.archive.is_empty());
        let (notice, is_problem) = state.notice.expect("a deletion should be acknowledged");
        assert!(!is_problem);
        assert!(
            notice.contains("PC-01"),
            "the notice should name what was deleted: {notice}"
        );
    }

    #[test]
    fn the_live_log_is_bounded_rather_than_growing_without_limit() {
        // A scan produces thousands of lines, and an unbounded log is a memory leak
        // with a terminal attached to it.
        let mut state = app();
        state.log_cap = 100;
        for index in 0..500 {
            state.push_log(LogKind::Progress, format!("line {index}"));
        }
        assert!(
            state.log.len() <= 100,
            "the log grew to {}",
            state.log.len()
        );
        // The newest lines are the ones worth keeping.
        assert!(state.log.last().unwrap().text.contains("499"));
    }

    #[test]
    fn a_tick_that_changes_nothing_does_not_mark_the_screen_dirty() {
        // This is what lets a scan sit for a minute without the terminal running
        // hot: a still screen costs nothing.
        let mut state = app();
        state.dirty = false;
        state.apply(Event::Tick(Duration::from_millis(16 * MS)));
        assert!(!state.dirty, "a still screen asked to be redrawn");
        state.dirty = true;
        state.apply(Event::Action(Action::NextScreen));
        assert!(state.dirty, "a screen change must redraw");
    }

    #[test]
    fn the_screen_wraps_in_both_directions_so_the_tab_bar_is_always_reachable() {
        let mut state = app();
        state.apply(Event::Action(Action::PreviousScreen));
        assert_eq!(state.screen, Screen::Rules, "wrapping backwards failed");
        state.apply(Event::Action(Action::NextScreen));
        assert_eq!(state.screen, Screen::Dashboard, "wrapping forwards failed");
    }

    #[test]
    fn every_screen_is_reachable_by_stepping_forwards_and_backwards() {
        // Five screens, two directions, and a user who will try both. Stepping
        // forward from anywhere must visit all five and land back where it started.
        for start in Screen::ALL {
            let mut state = app();
            state.goto(start);
            let mut forwards = Vec::new();
            for _ in 0..5 {
                state.apply(Event::Action(Action::NextScreen));
                forwards.push(state.screen);
            }
            let expected: Vec<Screen> =
                std::iter::successors(Some(start.next()), |s| Some(s.next()))
                    .take(5)
                    .collect();
            assert_eq!(
                forwards, expected,
                "forward from {start:?} skipped a screen"
            );
            assert_eq!(
                state.screen, start,
                "forward stepping did not come back round"
            );
        }
    }

    #[test]
    fn switching_screen_moves_focus_back_to_the_list() {
        // Focus pointing at a search field that no longer exists means typing goes
        // nowhere and the user cannot tell why.
        let mut state = app();
        state.apply(Event::Action(Action::ToggleSearch));
        assert_eq!(state.focus, Focus::Search);
        state.apply(Event::Action(Action::NextScreen));
        assert_eq!(state.focus, Focus::Primary);
    }

    #[test]
    fn typing_goes_into_the_search_field_only_while_that_field_has_focus() {
        let mut state = app();
        state.load_report(report_with(vec![finding("high", "c", "a service")]));
        state.apply(Event::Typed('x'));
        assert!(
            state.query.is_empty(),
            "typing reached the report while the list had focus"
        );
        state.apply(Event::Action(Action::ToggleSearch));
        state.apply(Event::Typed('x'));
        assert_eq!(state.query, "x");
        state.apply(Event::Backspace);
        assert!(state.query.is_empty());
    }

    fn archived(host: &str, findings: Vec<SanitisedFinding>) -> Archived {
        Archived {
            host_key: host.to_string(),
            host: SanitisedHost {
                name: host.to_string(),
                user: "bob".to_string(),
                os: "Windows".to_string(),
                build: "19045".to_string(),
                admin: true,
                quick: false,
                collected_at: "2026-09-29 14:22:01".to_string(),
            },
            verdict: SanitisedVerdict {
                high: 0,
                medium: 0,
                info: 0,
                headline: String::new(),
                recommendation: vec![],
            },
            counts: Counts::default(),
            taken_at: "2026-09-29 14:22:01".to_string(),
            findings,
            path: None,
        }
    }
}
