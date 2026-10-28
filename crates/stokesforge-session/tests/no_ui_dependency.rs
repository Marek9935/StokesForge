//! Guard: `stokesforge-session` must never gain a UI or Python dependency.
//! The actor holds the process's only `&mut Project` and must be able to serve
//! the UI as well as a future Python script thread without knowing either —
//! see the crate docs. Rather than guessing what "a UI dependency" or "a
//! Python dependency" is, this test lists the only allowed dependencies: any
//! addition to `Cargo.toml` must be added here explicitly, which forces a
//! conscious decision rather than an import that slips in.

const ALLOWED_DEPENDENCIES: &[&str] = &[
    "stokesforge-analysis", // recalculation of analysis calculations (#181): reads `Operation`, nothing UI
    "stokesforge-core",
    "stokesforge-commands",
    "stokesforge-persistence",
    "thiserror",
];

#[test]
fn only_allowed_dependencies_are_declared() {
    // Shared reading (`stokesforge-arch-guard`): normal, build, conditional
    // (`target`) and renamed (`package`) dependencies — issue #24.
    stokesforge_arch_guard::assert_only_allowed_dependencies(
        env!("CARGO_MANIFEST_DIR"),
        ALLOWED_DEPENDENCIES,
    );
}
