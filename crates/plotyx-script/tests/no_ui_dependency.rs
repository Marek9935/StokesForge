//! Même garde-fou que `plotyx-core` et `plotyx-session`, appliqué au fil de
//! script : ce crate a le droit de connaître Python — c'est sa raison d'être —
//! mais **jamais** l'interface. Plutôt que de deviner ce qu'est « une
//! dépendance UI », ce test liste les seules dépendances autorisées : toute
//! addition dans `Cargo.toml` doit être ajoutée ici explicitement, ce qui force
//! une décision consciente plutôt qu'un import qui se glisse.

const ALLOWED_DEPENDENCIES: &[&str] = &[
    "plotyx-core",     // le modèle
    "plotyx-commands", // le bus de commandes : tout passe par lui
    "plotyx-session",  // l'acteur, seul détenteur du `&mut Project`
    "plotyx-python",   // l'interpréteur embarqué et le pont de tableaux
    "plotyx-import",   // recettes et choix du modèle, sans interface
    "plotyx-importer", // conversion fichiers → colonnes commune au CLI et à l'application
    "pyo3",            // liaison CPython, version verrouillée avec plotyx-python
    "numpy",           // liaison de la C-API numpy, liée à celle de pyo3
    "thiserror",       // erreurs typées, sans surface UI
];

#[test]
fn only_allowed_dependencies_are_declared() {
    // Lecture commune (`plotyx-arch-guard`) : dépendances normales, de build,
    // conditionnelles (`target`) et renommées (`package`) — issue #24.
    plotyx_arch_guard::assert_only_allowed_dependencies(
        env!("CARGO_MANIFEST_DIR"),
        ALLOWED_DEPENDENCIES,
    );
}
