//! Guard: `stokesforge-import` stays independent of `stokesforge-core`, of
//! commands, of the session and of any interface (`src/lib.rs`: `f64`/`NaN`
//! output vocabulary, conversion done by `stokesforge-app`). Rather than
//! guessing what "a forbidden dependency" is, this test lists the only allowed
//! dependencies: any addition to `Cargo.toml` must be added here explicitly,
//! which forces a conscious decision.

const ALLOWED_DEPENDENCIES: &[&str] = &[
    "serde",       // serializable recipes
    "serde_json",  // template storage (RecipeStore)
    "directories", // configuration folder of the templates
    "thiserror",   // typed errors, with no UI surface
];

#[test]
fn only_allowed_dependencies_are_declared() {
    stokesforge_arch_guard::assert_only_allowed_dependencies(
        env!("CARGO_MANIFEST_DIR"),
        ALLOWED_DEPENDENCIES,
    );
}
