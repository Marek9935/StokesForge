//! The StokesForge **script thread**: lifecycle of the embedded interpreter,
//! interruption, and the `pyst` surface for navigation, reading and writing
//! offered to Python scripts.
//!
//! It is the last piece of the foundation: it makes the work package's
//! completion criterion true, or not.
//!
//! > A Python script reads a column **without copying**, writes another one
//! > **through the bus**, and `Ctrl+Z` undoes what the script just wrote.
//!
//! # The three GIL rules, and how the compiler enforces them
//!
//! 1. **The interface thread never touches Python.** Nothing here makes it
//!    do so: [`ScriptEngine::run`] can be called from any thread and
//!    delegates to the script thread.
//! 2. **The script thread releases the GIL while it waits for the actor.**
//!    Every call of the `pyst` surface goes through `Python::detach`. PyO3's
//!    `Ungil` marker **forbids at compile time** that a GIL-bound value enters
//!    the waiting block or comes out of it: this is not a discipline to
//!    keep, it is a compile error if it is missed.
//! 3. **The actor never takes the GIL.** `stokesforge-session` does not depend
//!    on `stokesforge-python`: the type of a Python object could not even be
//!    named there. This is what makes mutual blocking structurally
//!    impossible: without this rule, the script would hold the GIL while
//!    waiting for the actor and the actor would wait for the GIL.
//!
//! **The hidden path**: a request or a reply that carried a Python object
//! would be dropped **on the actor thread**; but dropping a Python object
//! decrements its reference count, which is a Python operation. Hence the
//! rule that closes the invariant: **no Python object crosses the channel**,
//! neither on the way in, nor on the way back, nor as a buried field. The two
//! guards above enforce it without human vigilance.
//!
//! **Accepted limit**: a script running a long **pure** Python loop keeps the
//! GIL between two interpreter checkpoints. This is inherent to CPython and
//! this crate does not claim to solve it.
//!
//! # The script thread is a permanent service
//!
//! The thread that initializes the interpreter becomes CPython's **main
//! thread**. Since the interface thread never touches Python, it must be the
//! script thread, so a thread created *per run* would kill CPython's main
//! thread with the first script. The thread is therefore created **once** by
//! [`ScriptEngine::spawn`] and lives until [`ScriptEngine::shutdown`]. **One
//! script at a time**: a second [`ScriptEngine::run`] while a script is
//! running returns [`ScriptError::Busy`].
//!
//! # Grouped writes (#67)
//!
//! Creations, renames, deletions, metadata and values go through the existing
//! commands. A successful run is undone in a single Ctrl+Z; an uncaught error
//! and an interruption cancel the current group. The reservation stays lazy:
//! a read run does not open it.
//!
//! # Feature `python`
//!
//! Everything sits behind the Cargo feature `python`, absent from `default`.
//! Without it, this crate compiles **empty** and pulls in no PyO3 dependency:
//! `cargo build`, `cargo test` and `cargo tree` stay clean on a machine
//! with no vendored interpreter.

#![cfg_attr(
    not(test),
    warn(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

#[cfg(feature = "python")]
mod analysis;
#[cfg(feature = "python")]
mod bridge;
#[cfg(feature = "python")]
mod capture;
#[cfg(feature = "python")]
mod engine;
#[cfg(feature = "python")]
mod error;
#[cfg(feature = "python")]
mod graph;
#[cfg(feature = "python")]
mod graph_size;
#[cfg(feature = "python")]
mod groups;
#[cfg(feature = "python")]
mod outcome;
#[cfg(feature = "python")]
mod selftest;
#[cfg(feature = "python")]
mod set_graph;
#[cfg(feature = "python")]
mod sheet_edit;
#[cfg(feature = "python")]
mod surface;

#[cfg(feature = "python")]
pub use engine::ScriptEngine;
#[cfg(feature = "python")]
pub use error::ScriptError;
#[cfg(feature = "python")]
pub use outcome::ScriptOutcome;
#[cfg(feature = "python")]
pub use selftest::{run_selftest, SelfTestReport, SelfTestStep};
#[cfg(feature = "python")]
pub use surface::{
    BrokenReferenceError, Handle, InvalidValueError, UnknownPathError, PYST_API_VERSION,
};

/// Tells whether this binary embeds the script thread (feature `python`).
///
/// Lets the interface hide its script console without having to test the
/// feature itself.
pub const SCRIPT_ENABLED: bool = cfg!(feature = "python");

#[cfg(test)]
mod tests {
    use super::SCRIPT_ENABLED;

    /// Compiles and passes in both feature configurations.
    #[test]
    fn script_enabled_reflects_the_feature() {
        assert_eq!(SCRIPT_ENABLED, cfg!(feature = "python"));
    }
}
