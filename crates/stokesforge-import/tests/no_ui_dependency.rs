//! Garde-fou : `stokesforge-import` reste indépendant de `stokesforge-core`, des
//! commandes, de la session et de toute interface (`src/lib.rs` : vocabulaire de
//! sortie `f64`/`NaN`, conversion faite par `stokesforge-app`). Plutôt que de deviner
//! ce qu'est « une dépendance interdite », ce test liste les seules dépendances
//! autorisées — toute addition dans `Cargo.toml` doit être ajoutée ici
//! explicitement, ce qui force une décision consciente.

const ALLOWED_DEPENDENCIES: &[&str] = &[
    "serde",       // recettes sérialisables
    "serde_json",  // stockage des modèles (RecipeStore)
    "directories", // dossier de configuration des modèles
    "thiserror",   // erreurs typées, sans surface UI
];

#[test]
fn only_allowed_dependencies_are_declared() {
    stokesforge_arch_guard::assert_only_allowed_dependencies(
        env!("CARGO_MANIFEST_DIR"),
        ALLOWED_DEPENDENCIES,
    );
}
