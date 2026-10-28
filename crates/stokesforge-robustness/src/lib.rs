//! Robustness bench: this crate holds no production code.
//!
//! It carries two things, and nothing else:
//!
//! - `tests/`: twenty-two batteries that test the base at its limits:
//!   extreme numeric values, malformed then semantically hostile `.stf`
//!   files, history macros, concurrency between actor clients, randomly
//!   drawn properties (proptest), the Rust <-> Python boundary in both
//!   directions, then (pass of the afternoon of 2026-09-11, pure Rust) the
//!   version counter contract (`b10`), the scope of name uniqueness (`b11`),
//!   commands accepted with no effect (`b12`), the bounds of a `PlotSpec`
//!   coming from disk (`b13`) and a sweep that rechecks three invariants
//!   after each step of a randomly drawn sequence (`b14`), the cell <-> text
//!   translation (`b15`), series whose columns differ in length (`b16`), the
//!   cost of common operations as the project grows (`b17`), what stays
//!   possible while a macro is open (`b18`), what a file written by a newer
//!   version does to this reader (`b19`), two writers on the same file
//!   (`b20`), cost and identity on the write path (`b21`) and (#193) the
//!   targeted recalculation of formulas checked, step by step, against the
//!   full recalculation of a twin session (`b22`);
//! - `src/bin/alloc_probe.rs` and `src/bin/stack_probe.rs`: two out-of-process
//!   probes, because the cases they test end in an `abort()` that nothing
//!   catches: an allocation failure for the first, a stack overflow for the
//!   second.
//!
//! # Two kinds of test, and how to tell them apart
//!
//! **The guards** pass, and must keep passing: they are ordinary
//! non-regression tests, which run in `cargo test --workspace` like the
//! others.
//!
//! **The reproductions** fail, deliberately. Each one reproduces an anomaly
//! found on 2026-09-11 and recorded in one of that day's two reports,
//! `docs/archive/rustrewrite/rapport-robustesse-2026-09-11.md` (A1 to C2,
//! batteries 1 to 9) and
//! `docs/archive/rustrewrite/rapport-robustesse-2026-09-11-suite.md` (A7 to
//! D2, batteries 10 to 20), under the identifier (A1, B2, ...) that its
//! `#[ignore]` attribute carries. They are marked `#[ignore]` so that the
//! pipeline keeps its value as a signal (a CI that is red all the time says
//! nothing anymore) and are run with:
//!
//! ```text
//! cargo test --workspace -- --ignored
//! ```
//!
//! **A reproduction that starts passing is the signal we are waiting for**:
//! the fix is good, and it is time to remove its `#[ignore]` so that the guard
//! takes over.
//!
//! # Where to start
//!
//! `docs/archive/rustrewrite/guide-de-correction-robustesse.md` sorts the
//! thirty anomalies **by file**, separates the mechanical fixes from the
//! decisions to make, gives an order that minimizes rework, and lists six
//! fix pitfalls checked on the repository (including this one: a fix of C3
//! that made an empty block *fail* would break undo).
//!
//! # The `python` feature
//!
//! Batteries `b7` and `b9` are behind `#![cfg(feature = "python")]`: without
//! the feature, they compile empty. They require the three environment
//! variables of `scripts/python-test.sh`, which already runs them.

#![cfg_attr(
    not(test),
    warn(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]
