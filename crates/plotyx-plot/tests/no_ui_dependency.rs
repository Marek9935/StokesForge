//! Même garde-fou que `plotyx-core` et `plotyx-commands` : `plotyx-plot` ne
//! doit jamais gagner de dépendance de dessin. Plutôt que de deviner ce qu'est
//! "une dépendance de rendu", ce test liste les seules dépendances autorisées —
//! toute addition dans `Cargo.toml` doit être ajoutée ici explicitement, ce qui
//! force une décision consciente plutôt qu'un import qui se glisse.
//!
//! C'est le critère de sortie de la phase R2 rendu exécutable : `cargo tree -p
//! plotyx-plot` ne doit montrer ni `plotters`, ni `egui`, ni `typst`
//! (`docs/newarchitecture/roadmap-rendu.md` §2.1).
//!
//! Depuis la phase R3, la dépendance listée est `plotyx-id`, pas
//! `plotyx-core` : `plotyx-core` a dû se mettre à dépendre de `plotyx-plot`
//! (pour porter un `PlotSpec` sur `NodeKind::Graph`), ce qui aurait fait un
//! cycle Cargo direct avec le sens inverse. `NodeId` a donc été descendu dans
//! `plotyx-id`, un crate minimal sans dépendance sous les deux — voir
//! `crates/plotyx-id/src/lib.rs` et `src/lib.rs` de ce crate. Le critère de
//! sortie R2 tient toujours, et plus étroitement qu'avant : ce crate ne voit
//! même plus `Tree` ni `Column`, seulement `NodeId`.

const ALLOWED_DEPENDENCIES: &[&str] = &[
    "plotyx-id", // identité de nœud seule : dépendance à sens unique
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
             plotyx-plot décrit un graphe, il ne le dessine pas \
             (voir roadmap-rendu phase R2)"
        );
    }
}
