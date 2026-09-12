//! Garde-fou : `plotyx-persistence` ne doit jamais gagner de dépendance UI.
//! Plutôt que de deviner ce qu'est « une dépendance UI », ce test liste les
//! seules dépendances autorisées — toute addition dans `Cargo.toml` doit être
//! ajoutée ici explicitement, ce qui force une décision consciente plutôt
//! qu'un import qui se glisse.

const ALLOWED_DEPENDENCIES: &[&str] = &[
    "plotyx-core",
    // le PlotSpec porté par NodeKind::Graph (phase R3) : la conversion vers
    // son propre schéma (schema.rs) a besoin du type complet, pas seulement
    // de ce que plotyx-core en réexporte
    "plotyx-plot",
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
    let manifest = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
        .expect("Cargo.toml lisible");
    let parsed: toml::Value = manifest.parse().expect("Cargo.toml valide");

    let Some(deps) = parsed.get("dependencies").and_then(|d| d.as_table()) else {
        return; // pas de section [dependencies] du tout : rien à vérifier
    };

    for name in deps.keys() {
        assert!(
            ALLOWED_DEPENDENCIES.contains(&name.as_str()),
            "dépendance `{name}` non listée dans ALLOWED_DEPENDENCIES : \
             plotyx-persistence doit rester sans dépendance UI (voir roadmap phase 3)"
        );
    }
}
