//! Entry point. The only file that knows both the terminal and the runtime.

use irscan_tui::run::{self, Options};

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    // `--help` and a bad flag are not errors: they are a request, and printing to
    // stdout and exiting 0 is what a user who typed `irscan-tui --help` expects.
    // Anything else would make a shell script treat a help request as a failure.
    let options = match Options::parse(&args) {
        // `Ok(None)` is `--help`: a request, answered on stdout with a zero exit so
        // a script reading the flags is not told the flag was rejected.
        Ok(None) => {
            println!("{}", run::usage());
            return Ok(());
        }
        Ok(Some(options)) => options,
        Err(message) => {
            // A bad flag is also printed to stdout rather than stderr: the message
            // ends with the usage, and mixing streams would split them apart.
            println!("{message}");
            return Ok(());
        }
    };
    if options.version {
        println!("{}", run::NAME);
        return Ok(());
    }
    // Elevation happens before the terminal is taken, because the terminal cannot
    // survive being handed to another process. `--elevate` asks for a UAC prompt and
    // hands over; this copy then exits, and the elevated one is the program. A
    // declined prompt is not an error - the user made a decision - so the scan runs
    // with reduced coverage and the interface says what it will not see, which is
    // SR-6's whole point.
    if options.elevate && !options.elevated_already {
        if let Some(reason) = irscan_tui::scan::platform::relaunch_elevated() {
            eprintln!("irscan-tui: {reason}");
        } else {
            // The elevated copy is running. This one has nothing left to do.
            return Ok(());
        }
    }

    let mut app = run::build(&options);
    // FR-2: `--open` is the whole interface on a host with no engine, so it is
    // wired here rather than left as a parsed flag nobody reads. The failure is
    // reported through the app's notice, not to stderr: the terminal is already in
    // the alternate screen at this point, and a message the user never sees is
    // worse than one they see next to the interface.
    if let Some(path) = options.open.as_ref() {
        let engine = irscan_tui::scan::engine();
        match engine.import(path) {
            Ok(report) => app.load_report(report),
            Err(e) => app.apply(irscan_tui::app::Event::ScanFailed(e.headline())),
        }
    }

    // `ratatui::init` takes the terminal, sets raw mode, enters the alternate
    // screen and hides the cursor; `ratatui::restore` puts all four back. The panic
    // hook is what makes a panic recoverable: without it a panic in a render path
    // leaves the user's terminal in the alternate screen with the cursor hidden and
    // no echo, and the only way out is `reset`. This is why the release profile
    // unwinds rather than aborting.
    let terminal = ratatui::init();

    let outcome = run::run(terminal, app);
    ratatui::restore();
    let app = outcome?;

    // A scan that ran is worth keeping, whether or not it found anything: the next
    // scan of the same machine is compared against it, and that comparison is the
    // reason this front end exists.
    persist(&options, &app, options.open.is_some());

    Ok(())
}

/// Write the current report into the archive, if a scan produced one.
///
/// Skipped when the report came from `--open`, for a reason that is about the
/// archive rather than about tidiness: the filename is derived from the host name
/// and the collected-at stamp, so re-saving an opened report would overwrite the
/// archive entry of the scan it was read from. Reading a report must not modify the
/// archive, or opening a report twice would destroy the history the interface
/// exists to keep.
fn persist(options: &Options, app: &irscan_tui::app::App, opened: bool) {
    if opened {
        return;
    }
    let Some(report) = &app.report else {
        return;
    };
    let dir = match options
        .archive_dir
        .clone()
        .map(Ok)
        .unwrap_or_else(irscan_tui::report::archive_dir)
    {
        Ok(dir) => dir,
        Err(e) => {
            eprintln!("irscan-tui: the archive is unavailable: {e}");
            return;
        }
    };
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("irscan-tui: cannot create {}: {e}", dir.display());
        return;
    }
    let path =
        irscan_tui::report::archive_path(&dir, &report.host_key(), &report.host.collected_at);
    match serde_json::to_string(report) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&path, json) {
                eprintln!("irscan-tui: cannot write {}: {e}", path.display());
            }
        }
        Err(e) => eprintln!("irscan-tui: cannot serialise the report: {e}"),
    }
}


#[cfg(test)]
mod manifest {
    //! The manifest is pinned by reading it out of the built executable.
    //!
    //! The engine does this for itself in its `src/lib.rs`, and the reason applies
    //! identically here: the thing that decides whether Windows shows a UAC prompt
    //! and opens the program in a separate window is a linker flag in a build script
    //! of a *dependency*. That is invisible in a diff of this crate and completely
    //! decisive about how the program feels to use.
    //!
    //! The test skips when there is no executable to read, so `cargo test` on a fresh
    //! checkout does not fail for a reason that has nothing to do with the code. It
    //! is not a silent pass in the way that would matter: building before testing is
    //! what CI and a developer both do, and the assertion runs.
    use std::path::Path;

    /// The built executable, if this test run can see one.
    fn exe() -> Option<std::path::PathBuf> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/debug/irscan-tui.exe");
        path.exists().then_some(path)
    }

    /// The whole file as text. A manifest is a resource, not plain text at a fixed
    /// offset, and a PE resource section holds it verbatim, so a lossy read finds it.
    fn as_text(path: &Path) -> String {
        let bytes = std::fs::read(path).expect("the executable should be readable");
        String::from_utf8_lossy(&bytes).into_owned()
    }

    #[test]
    fn the_manifest_asks_for_no_privilege_so_the_tui_opens_in_the_terminal_it_was_typed_in() {
        // The engine embeds `level='requireAdministrator'`, which makes Windows start
        // an elevated conhost in a SEPARATE WINDOW on every launch. A full-screen
        // program that opens detached from the shell the user is sitting in is not a
        // terminal program that happens to be elevated; it is a window that happens
        // to be full-screen. It also makes --elevate meaningless, because the
        // process is already up, and it makes reading a report require dismissing a
        // UAC prompt for a privilege level that reading does not need.
        let path = match exe() {
            Some(path) => path,
            None => {
                eprintln!("no built executable; run `cargo build` first");
                return;
            }
        };
        let text = as_text(&path);
        let missing_invoker = text.contains("asInvoker") == false;
        assert!(
            missing_invoker == false,
            "the binary does not ask for asInvoker, so every launch prompts for UAC and \
             opens in a separate window. See build.rs."
        );
        let still_admin = text.contains("requireAdministrator");
        assert!(
            still_admin == false,
            "the binary still asks for administrator, inherited from the engine's build \
             script. See build.rs."
        );
    }

    #[test]
    fn a_missing_executable_skips_rather_than_failing_for_an_unrelated_reason() {
        // Documented as behaviour rather than left to chance: the assertion above is
        // only meaningful when there is a binary to read, and a test that fails
        // because someone ran `cargo test` on a clean checkout is a test people
        // learn to ignore.
        if exe().is_none() {
            eprintln!("skipped: build first to check the manifest");
        }
    }
}
