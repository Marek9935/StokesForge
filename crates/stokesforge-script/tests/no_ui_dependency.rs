//! Same guard as `stokesforge-core` and `stokesforge-session`, applied to the
//! script thread: this crate may know Python (that is its reason to exist) but
//! **never** the interface. Rather than guess what "a UI dependency" is, this
//! test lists the only allowed dependencies: any addition to `Cargo.toml` must
//! be added here explicitly, which forces a conscious decision instead of an
//! import that slips in.

const ALLOWED_DEPENDENCIES: &[&str] = &[
    "stokesforge-analysis", // pure analysis computations: `pyst.analysis`, no interface
    "stokesforge-core",     // the model
    "stokesforge-commands", // the command bus: everything goes through it
    "stokesforge-plot", // native description of graphs and graphic objects, no rendering or interface
    "stokesforge-session", // the actor, sole holder of the `&mut Project`
    "stokesforge-python", // the embedded interpreter and the array bridge
    "stokesforge-import", // recipes and template choices, no interface
    "stokesforge-importer", // file → column conversion shared by the CLI and the application
    "stokesforge-render-svg", // figure export (`pyst.export_graph`): SVG/PDF/PNG, neither egui nor winit
    "pyo3",                   // CPython binding, version locked with stokesforge-python
    "numpy",                  // binding of the numpy C-API, tied to pyo3's
    "thiserror",              // typed errors, no UI surface
];

#[test]
fn only_allowed_dependencies_are_declared() {
    // Shared reading (`stokesforge-arch-guard`): normal, build, conditional
    // (`target`) and renamed (`package`) dependencies (issue #24).
    stokesforge_arch_guard::assert_only_allowed_dependencies(
        env!("CARGO_MANIFEST_DIR"),
        ALLOWED_DEPENDENCIES,
    );
}
