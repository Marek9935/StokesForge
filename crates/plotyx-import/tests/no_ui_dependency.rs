//! Garde-fou : `plotyx-import` reste indépendant de `plotyx-core`, des
//! commandes, de la session et de toute interface (`src/lib.rs` : vocabulaire de
//! sortie `f64`/`NaN`, conversion faite par `plotyx-app`). Plutôt que de deviner
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
    plotyx_arch_guard::assert_only_allowed_dependencies(
        env!("CARGO_MANIFEST_DIR"),
        ALLOWED_DEPENDENCIES,
    );
}
