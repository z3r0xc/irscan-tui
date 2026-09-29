//! Entry point. The only file that knows both the terminal and the runtime.

fn main() -> std::io::Result<()> {
    // The loop does not exist yet. Rather than exit silently, say so: a binary that
    // starts and does nothing is indistinguishable from a broken one, and the whole
    // point of the ASCII banner is that this tool is not a decoy.
    eprintln!("irscan-tui: not yet wired up; see docs/plan.md phase 7");
    Ok(())
}
