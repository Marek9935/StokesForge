//! Garde-fou de la phase 1 (`docs/archive/rustrewrite/roadmap.md`) : `stokesforge-core`
//! ne doit jamais gagner de dépendance UI. Plutôt que de deviner ce qu'est
//! "une dépendance UI", ce test liste les seules dépendances autorisées —
//! toute addition dans `Cargo.toml` doit être ajoutée ici explicitement,
//! ce qui force une décision consciente plutôt qu'un import qui se glisse.

const ALLOWED_DEPENDENCIES: &[&str] = &[
    "thiserror",      // erreurs typées, sans surface UI
    "stokesforge-id", // identité de nœud seule (NodeId), zéro dépendance transitive
    // le PlotSpec que NodeKind::Graph porte (phase R3) — pas de dépendance de
    // dessin : cargo tree -p stokesforge-plot ne montre toujours que stokesforge-id
    "stokesforge-plot",
];

#[test]
fn only_allowed_dependencies_are_declared() {
    // Lecture commune (`stokesforge-arch-guard`) : dépendances normales, de build,
    // conditionnelles (`target`) et renommées (`package`) — issue #24.
    stokesforge_arch_guard::assert_only_allowed_dependencies(
        env!("CARGO_MANIFEST_DIR"),
        ALLOWED_DEPENDENCIES,
    );
}
