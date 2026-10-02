//! Garde-fou : `stokesforge-persistence` ne doit jamais gagner de dépendance UI.
//! Plutôt que de deviner ce qu'est « une dépendance UI », ce test liste les
//! seules dépendances autorisées — toute addition dans `Cargo.toml` doit être
//! ajoutée ici explicitement, ce qui force une décision consciente plutôt
//! qu'un import qui se glisse.

const ALLOWED_DEPENDENCIES: &[&str] = &[
    "stokesforge-core",
    // le PlotSpec porté par NodeKind::Graph (phase R3) : la conversion vers
    // son propre schéma (schema.rs) a besoin du type complet, pas seulement
    // de ce que stokesforge-core en réexporte
    "stokesforge-plot",
    "thiserror",
    "serde",
    "rmp-serde",
    "serde_bytes",
    "tempfile",
    // Capture générique d'une variante d'énumération inconnue (C5, niveau 2,
    // `guide-de-correction-robustesse.md` §F3) : `rmpv::Value` modélise
    // n'importe quelle valeur MessagePack, ce qui permet de tenter le
    // décodage strict d'une variante connue puis, à l'échec, de conserver le
    // contenu tel quel plutôt que de faire échouer tout le fichier. Ce
    // n'est toujours pas une dépendance UI : c'est le même format que
    // `rmp-serde` sait déjà lire/écrire, vu de façon non typée.
    "rmpv",
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
