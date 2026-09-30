//! Garde-fou : `plotyx-importer` fait le pont entre `plotyx-import` (fichiers) et
//! `plotyx-core` (modèle), utilisé par l'application, le CLI et les scripts. Il ne
//! doit connaître ni interface, ni commandes, ni session : chacun de ses
//! consommateurs décide de **comment** écrire dans le projet.

const ALLOWED_DEPENDENCIES: &[&str] = &[
    "plotyx-import", // la lecture des fichiers
    "plotyx-core",   // les colonnes du modèle
];

#[test]
fn only_allowed_dependencies_are_declared() {
    plotyx_arch_guard::assert_only_allowed_dependencies(
        env!("CARGO_MANIFEST_DIR"),
        ALLOWED_DEPENDENCIES,
    );
}
