//! StokesForge's Python bridge: **embedded** CPython interpreter, isolated from the system.
//!
//! This crate is the **only** one in the repository allowed to start the interpreter and to
//! give access to the GIL. It does not know StokesForge's data model: it
//! only handles scalar types and slices of `f64`.
//!
//! # Feature `python`
//!
//! Everything that touches PyO3 is behind the `python` Cargo feature, absent from
//! `default`. Without it, this crate compiles empty and pulls in no PyO3
//! dependency: `cargo build`, `cargo test` and CI stay green on a machine
//! without a vendored interpreter.
//!
//! With the feature, compilation requires the `PYO3_CONFIG_FILE` environment
//! variable produced by `scripts/python-config.sh` (see the README,
//! pitfall no. 1).
//!
//! # Lifecycle
//!
//! Initialization is **explicit and lazy** (PyO3's `auto-initialize`
//! is not enabled): see [`interpreter::EmbeddedPython::initialize`].
//! The interpreter is **never finalized**: `Py_Finalize` is a known
//! source of crashes on exit and brings nothing to a process that is
//! ending. Accepted consequence: Python `atexit` handlers do not run.

#![cfg_attr(
    not(test),
    warn(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

#[cfg(feature = "python")]
pub mod array;
#[cfg(feature = "python")]
pub mod interpreter;

/// Tells whether this binary embeds the Python bridge (`python` feature).
///
/// Lets the rest of the application degrade its interface cleanly
/// (script console hidden, Matplotlib renderer unavailable) without having to
/// test the feature itself.
pub const PYTHON_ENABLED: bool = cfg!(feature = "python");

#[cfg(test)]
mod tests {
    use super::PYTHON_ENABLED;

    /// Compiles and passes in both feature configurations.
    #[test]
    fn python_enabled_reflects_the_feature() {
        assert_eq!(PYTHON_ENABLED, cfg!(feature = "python"));
    }
}
