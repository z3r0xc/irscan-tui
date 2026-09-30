//! Build script: this crate's own linker arguments.
//!
//! The engine's build script wires `/MANIFEST:EMBED` and
//! `/MANIFESTUAC:"level='requireAdministrator' uiAccess='false'"` into every binary
//! that links it, because the CLI is meant to run elevated and say so in the
//! manifest rather than asking. `rustc-link-arg-bins` applies to the binary
//! target of the crate being built, so those arguments land on *this* binary too
//! the moment it depends on the engine.
//!
//! That is wrong for a terminal program, and wrong in a way that is not obvious:
//!
//! * Every launch becomes a UAC elevation, whether or not the user asked for one.
//!   Windows starts an elevated conhost in a **separate window**, so the TUI opens
//!   detached from the terminal the user is sitting in - and `q` leaves them looking
//!   at a window they did not ask for.
//! * `--elevate` becomes meaningless, because the process is *already* elevated.
//!   The flag this project added to give the user a choice now has no choice in it.
//! * An operator who just wants to read a report cannot, without dismissing a
//!   UAC prompt, for a privilege level that reading does not need.
//!
//! So the arguments are re-issued here, later in the link order, as `asInvoker`.
//! Cargo appends a build script's link args after the dependency's, so the last
//! value wins and the manifest says what this program actually wants.
//!
//! Both arguments are needed, and the pair is not obvious:
//!
//! * `/MANIFEST:EMBED` is what makes the linker write a manifest resource at all.
//!   `/MANIFESTUAC` alone sets a value but produces no `.rsrc` section, so the
//!   binary keeps running however it was going to and nothing changes.
//! * The single quotes around each value are required by the linker's grammar.
//!   Without them mt.exe emits `level=requireAdministrator` unquoted, which is not
//!   well-formed XML, and Windows refuses to start the program with a
//!   side-by-side configuration error that never mentions the manifest.
//! * The double quotes the documentation shows must NOT be here. Through
//!   `-C link-arg=` they reach mt.exe literally and land inside the attribute name.
//!   The engine found this the hard way and says so in its own build.rs; the form
//!   below is the one that produces a valid `<requestedExecutionLevel
//!   level='asInvoker' uiAccess='false' />`. Adding the quotes back fails the link
//!   with `LNK1181: cannot open input file "uiAccess='false'.obj"` - the linker
//!   splits at the space and treats the tail as an object file.
//!
//! Verified, not assumed: the manifest test in src/main.rs reads the
//! embedded resource out of the built executable and asserts on the string. The
//! engine pins its own manifest the same way, in its `src/lib.rs`.

fn main() {
    // `bins` only. `rustc-link-arg-tests` is not a real directive - cargo rejects it
    // with "invalid instruction" - and the reason a test harness might want it does
    // not apply: the test binary never runs the TUI, so its manifest is irrelevant.
    println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg-bins=/MANIFESTUAC:level='asInvoker' uiAccess='false'");
    println!("cargo:rerun-if-changed=build.rs");
}
