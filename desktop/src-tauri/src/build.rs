//! The identity of the binary that wrote a record (ADR 0084).
//!
//! A telemetry record outlives the process that wrote it: the accounts under
//! `<state>/perf/` and the step log are read days later, when the build that
//! produced them has been replaced twice and every version string in this
//! repository still says `0.1.0`. Without a stamp the only way to tell a record
//! of the fixed build from a record of the broken one is the file's timestamp
//! and the answer's own text — an inference, not a fact.
//!
//! The value comes from the build script, not from a constant: see
//! `build.rs` for what it names and why it has to watch what it names.

/// The commit the host was built from and the commit of the kernel it linked.
///
/// Shaped `keelson 0.1.0 a1b2c3d4 keel 5e6f7a8b`, with `-dirty` on a tree that
/// had uncommitted changes and `unknown` when the build had no checkout to ask.
pub const ID: &str = env!("KEELSON_BUILD");
