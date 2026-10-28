//! The rendering base sees `epaint`, not `egui`.
//!
//! This is the whole reason for this crate to exist: if its dependency list
//! gained `egui`, `stokesforge-render-svg` would gain it too, and the export
//! could no longer be tested without a graphical server. Same discipline as
//! `stokesforge-core/tests/no_ui_dependency.rs`: list what is allowed rather
//! than guess what is forbidden.

const ALLOWED: &[&str] = &[
    "stokesforge-core", // ResolvedPlot, Tree, Column
    "stokesforge-plot", // PlotSpec
    "plotters",         // without its fonts (default-features = false)
    "plotters-backend", // BackendTextStyle, DrawingBackend
    "epaint",           // measurement, and nothing else of the UI stack
    // Resolution of a system font by its name (`fonts.rs`, font embedding of
    // 2026-09-13): neither egui nor plotters-svg, just a read of the file
    // system — same version as the one `usvg` already locks elsewhere in the
    // workspace (`stokesforge-render-svg` reaches it by re-export, not
    // directly, so as not to declare it twice).
    "fontdb",
    // R8, feature `typst` (optional, absent from `default`): the Typst
    // `World`, the compilation and the generic cache live here precisely so
    // that neither `stokesforge-render-egui` nor `stokesforge-render-svg` has
    // to reimplement the `World` on its own side
    // (`architecture-choisie.md` §8 — « où va quoi »). Unlike `egui` or
    // `plotters-svg`, `typst`/`typst-layout` are wanted here, not a default
    // that slipped in.
    "typst",
    "typst-layout",
];

#[test]
fn only_the_expected_dependencies_are_declared() {
    // This crate is the common reference of the two backends: it must know
    // neither egui nor plotters-svg, under any form of dependency (normal,
    // conditional, renamed) — shared reading, issue #24.
    stokesforge_arch_guard::assert_only_allowed_dependencies(env!("CARGO_MANIFEST_DIR"), ALLOWED);
}

#[test]
fn plotters_is_taken_without_its_fonts() {
    // `default-features = false` removes font-kit, image, chrono and the scan
    // of system fonts (`architecture-choisie.md` §4.4). The removal is safe
    // only because both backends override `estimate_text_size` — and it must
    // stay in place, otherwise Plotters would start measuring on its own,
    // with other fonts than the screen.
    let manifest: toml::Value =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
            .expect("Cargo.toml is readable")
            .parse()
            .expect("Cargo.toml is valid");
    let plotters = manifest
        .get("dependencies")
        .and_then(|d| d.get("plotters"))
        .expect("plotters dependency");
    assert_eq!(
        plotters.get("default-features").and_then(|v| v.as_bool()),
        Some(false),
        "plotters got its default features back: the system fonts come \
         back, and with them a second source of measurement"
    );
}
