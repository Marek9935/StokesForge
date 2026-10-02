//! Garde-fou : `stokesforge-session` ne doit jamais gagner de dépendance UI ou
//! Python. L'acteur détient le seul `&mut Project` du processus et doit
//! pouvoir servir aussi bien l'UI qu'un futur thread de script Python sans
//! connaître ni l'un ni l'autre — voir la doc de crate. Plutôt que de
//! deviner ce qu'est « une dépendance UI » ou « une dépendance Python »,
//! ce test liste les seules dépendances autorisées : toute addition dans
//! `Cargo.toml` doit être ajoutée ici explicitement, ce qui force une
//! décision consciente plutôt qu'un import qui se glisse.

const ALLOWED_DEPENDENCIES: &[&str] = &[
    "stokesforge-core",
    "stokesforge-commands",
    "stokesforge-persistence",
    "thiserror",
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
