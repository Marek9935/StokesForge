//! Guard of #263 (work package #254): `stokesforge-app-state` (the application
//! state, its texts and its errors put into sentences) must **never** depend on
//! a UI library. That is its whole reason for being: the "egui only draws"
//! boundary holds through the compiler, and the state tests (`tests/shell.rs`…)
//! compile without a window.
//!
//! Rather than guessing what "a UI dependency" is, this test lists the only
//! allowed dependencies: any addition to `Cargo.toml` must be added here
//! explicitly, which forces a conscious decision. No `egui`, no `eframe`, no
//! `winit`, no `wgpu`, and neither `stokesforge-clipboard` nor
//! `stokesforge-render-egui`, one of which pulls in winit (smithay patch) and
//! the other egui: the offer of the figure to the clipboard stays in
//! `stokesforge-app`.

const ALLOWED_DEPENDENCIES: &[&str] = &[
    "stokesforge-analysis",    // analysis forms: bounds, prerequisites, settings
    "stokesforge-core",        // the model
    "stokesforge-commands",    // the command bus: everything goes through it
    "stokesforge-import",      // import recipes, no interface
    "stokesforge-importer",    // instrument files -> model columns
    "stokesforge-session",     // the actor, sole holder of the `&mut Project`
    "stokesforge-persistence", // `.stf` format
    "stokesforge-plot",        // native description of graphs, no rendering
    // `RenderError`, formulas, graph geometry: generic over the backend, sees
    // only `epaint` for text measurement (deliberate choice)
    "stokesforge-render",
    "stokesforge-render-svg", // SVG/PDF/PNG export, no graphics server
    // Python bridge, `python` feature only
    "stokesforge-script",
    "stokesforge-python",
    // native file picker (`file_picker`): only brings `raw-window-handle` here,
    // neither egui nor winit
    "rfd",
    "directories", // configuration folder (preferences, colors)
    "serde",       // saved preferences and colors
    "serde_json",
    // multilingual interface (Fluent): the catalog and `tr!`
    "i18n-embed",
    "i18n-embed-fl",
    "rust-embed",
    "fluent-syntax", // pseudo-language `en-XA`
];

#[test]
fn only_allowed_dependencies_are_declared() {
    // Shared reading (`stokesforge-arch-guard`): normal, build, conditional
    // (`target`) and renamed (`package`) dependencies, issue #24.
    stokesforge_arch_guard::assert_only_allowed_dependencies(
        env!("CARGO_MANIFEST_DIR"),
        ALLOWED_DEPENDENCIES,
    );
}
