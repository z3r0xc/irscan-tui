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

    let app = run::build(&options);

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
    persist(&options, &app);

    Ok(())
}

/// Write the current report into the archive, if there is one.
fn persist(options: &Options, app: &irscan_tui::app::App) {
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
