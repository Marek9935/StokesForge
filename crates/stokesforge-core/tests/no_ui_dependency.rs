//! Phase 1 guard (`docs/archive/rustrewrite/roadmap.md`): `stokesforge-core`
//! must never gain a UI dependency. Rather than guessing what
//! "a UI dependency" is, this test lists the only allowed dependencies —
//! any addition to `Cargo.toml` must be added here explicitly,
//! which forces a conscious decision rather than an import that slips in.

const ALLOWED_DEPENDENCIES: &[&str] = &[
    "thiserror",      // typed errors, no UI surface
    "stokesforge-id", // node identity only (NodeId), zero transitive dependencies
    // the PlotSpec that NodeKind::Graph carries (phase R3) — no drawing
    // dependency: cargo tree -p stokesforge-plot still shows only stokesforge-id
    "stokesforge-plot",
    // the types of an analysis operation that the model records (#181):
    // pure computation, with no dependency other than thiserror (see its own guard)
    "stokesforge-analysis",
];

#[test]
fn only_allowed_dependencies_are_declared() {
    // Shared reading (`stokesforge-arch-guard`): normal, build,
    // conditional (`target`) and renamed (`package`) dependencies — issue #24.
    stokesforge_arch_guard::assert_only_allowed_dependencies(
        env!("CARGO_MANIFEST_DIR"),
        ALLOWED_DEPENDENCIES,
    );
}
