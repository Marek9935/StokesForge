//! Garde-fou : `stokesforge-analysis` est du calcul pur (décision #181,
//! 2026-10-08) — indépendant de tout autre crate du workspace, de l'interface
//! et de Python. Ce test liste les seules dépendances autorisées : toute
//! addition dans `Cargo.toml` doit être ajoutée ici explicitement, ce qui
//! force une décision consciente (et, pour une crate d'algèbre, un passage
//! par `cargo deny check licenses`).

const ALLOWED_DEPENDENCIES: &[&str] = &[
    "thiserror", // erreurs typées, sans surface UI
];

#[test]
fn only_allowed_dependencies_are_declared() {
    stokesforge_arch_guard::assert_only_allowed_dependencies(
        env!("CARGO_MANIFEST_DIR"),
        ALLOWED_DEPENDENCIES,
    );
}
