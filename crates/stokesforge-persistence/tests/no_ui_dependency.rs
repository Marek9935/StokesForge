//! Guard: `stokesforge-persistence` must never gain a UI dependency.
//! Rather than guessing what "a UI dependency" is, this test lists the
//! only allowed dependencies: any addition to `Cargo.toml` must be added
//! here explicitly, which forces a conscious decision rather than an import
//! that slips in.

const ALLOWED_DEPENDENCIES: &[&str] = &[
    "stokesforge-analysis", // `Operation` of a calculation: its tree shape goes through the file (#181)
    "stokesforge-core",
    // the import recipe of an imported column goes through the file: its
    // mirror (recipe_record.rs) converts from `ImportRecipe` (S2, #323)
    "stokesforge-import",
    // the PlotSpec carried by NodeKind::Graph (phase R3): the conversion to
    // its own schema (schema.rs) needs the full type, not only what
    // stokesforge-core re-exports of it
    "stokesforge-plot",
    "thiserror",
    "serde",
    "rmp-serde",
    "serde_bytes",
    "tempfile",
    // Generic capture of an unknown enum variant (C5, level 2,
    // `guide-de-correction-robustesse.md` §F3): `rmpv::Value` models any
    // MessagePack value, which makes it possible to try the strict decoding
    // of a known variant and, on failure, to keep the content as is rather
    // than fail the whole file. It is still not a UI dependency: it is the
    // same format that `rmp-serde` can already read/write, seen untyped.
    "rmpv",
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
