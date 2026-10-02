//! Garde-fou : `stokesforge-importer` fait le pont entre `stokesforge-import` (fichiers) et
//! `stokesforge-core` (modèle), utilisé par l'application, le CLI et les scripts. Il ne
//! doit connaître ni interface, ni commandes, ni session : chacun de ses
//! consommateurs décide de **comment** écrire dans le projet.

const ALLOWED_DEPENDENCIES: &[&str] = &[
    "stokesforge-import", // la lecture des fichiers
    "stokesforge-core",   // les colonnes du modèle
];

#[test]
fn only_allowed_dependencies_are_declared() {
    stokesforge_arch_guard::assert_only_allowed_dependencies(
        env!("CARGO_MANIFEST_DIR"),
        ALLOWED_DEPENDENCIES,
    );
}
