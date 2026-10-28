//! Guard: `stokesforge-analysis` is pure computation (decision #181,
//! 2026-10-08) — independent of any other crate of the workspace, of the
//! interface and of Python. This test lists the only allowed dependencies:
//! any addition in `Cargo.toml` must be added here explicitly, which forces
//! a conscious decision (and, for an algebra crate, a pass through
//! `cargo deny check licenses`).

const ALLOWED_DEPENDENCIES: &[&str] = &[
    "thiserror", // typed errors, no UI surface
    "libm",      // transcendental functions in pure Rust: same bits everywhere (#332)
];

#[test]
fn only_allowed_dependencies_are_declared() {
    stokesforge_arch_guard::assert_only_allowed_dependencies(
        env!("CARGO_MANIFEST_DIR"),
        ALLOWED_DEPENDENCIES,
    );
}
