//! Same guard as `stokesforge-core`: `stokesforge-commands` must never gain a
//! UI dependency. Rather than guess what "a UI dependency" is, this test lists
//! the only allowed dependencies. Any addition to `Cargo.toml` must be added
//! here explicitly, which forces a conscious decision rather than an import
//! that slips in.

const ALLOWED_DEPENDENCIES: &[&str] = &[
    "stokesforge-analysis", // `Operation`/`OperationKind` carried by `Command::AddCalculation` and its labels; pure Rust, no UI
    "stokesforge-core",     // the model: one-way dependency, no UI surface
    "stokesforge-plot", // `PlotSpec`, carried by `Command::AddGraph`/`SetPlotSpec`; no drawing dependency (invariant 3)
    "thiserror",        // typed errors, no UI surface
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
