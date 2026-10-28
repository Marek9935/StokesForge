//! Plugs [`stokesforge_script::run_selftest`] into the ordinary test suite:
//! replays the end criterion of the foundation and asserts that every step
//! passed.
//!
//! One test only in this file, on purpose. `cargo test` compiles each file of
//! `tests/` into its **own binary**, hence its own process: this isolates the
//! check from `tests/script.rs` without having to share its interpreter access
//! lock. CPython starts only once per process, and [`run_selftest`] starts its
//! own script engine. A second test in this file would run on a thread of the
//! same process and would hit the engine's "one script at a time"; a single
//! test avoids it with no discipline to keep up.
//!
//! Requires the vendored prefix: run `scripts/python-vendor.sh` then
//! `scripts/python-test.sh` (which sets `STOKESFORGE_PYTHON_HOME` before
//! calling `cargo test`).

#![cfg(feature = "python")]

use std::path::PathBuf;

use stokesforge_python::interpreter::{self, EmbeddedPython};
use stokesforge_script::run_selftest;

/// Resolves the embedded prefix: `STOKESFORGE_PYTHON_HOME` if set by the test
/// script, otherwise `vendor/python/extracted/python` relative to the
/// manifest, the same resolution as `tests/script.rs`.
fn home() -> PathBuf {
    match std::env::var_os(interpreter::HOME_ENV) {
        Some(value) => PathBuf::from(value),
        None => {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vendor/python/extracted/python")
        }
    }
}

#[test]
fn the_foundation_end_criterion_passes_end_to_end_from_the_source_tree() {
    let report = run_selftest(EmbeddedPython::with_home(home()));

    for step in &report.steps {
        assert!(
            step.passed,
            "failed step: {} — {}\n\nfull report:\n{}",
            step.name,
            step.detail,
            report.render()
        );
    }
    assert!(report.passed(), "full report:\n{}", report.render());
}
