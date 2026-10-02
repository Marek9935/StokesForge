//! Même garde-fou que `stokesforge-core` et `stokesforge-commands` : `stokesforge-plot` ne
//! doit jamais gagner de dépendance de dessin. Plutôt que de deviner ce qu'est
//! "une dépendance de rendu", ce test liste les seules dépendances autorisées —
//! toute addition dans `Cargo.toml` doit être ajoutée ici explicitement, ce qui
//! force une décision consciente plutôt qu'un import qui se glisse.
//!
//! C'est le critère de sortie de la phase R2 rendu exécutable : `cargo tree -p
//! stokesforge-plot` ne doit montrer ni `plotters`, ni `egui`, ni `typst`
//! (`docs/archive/newarchitecture/roadmap-rendu.md` §2.1).
//!
//! Depuis la phase R3, la dépendance listée est `stokesforge-id`, pas
//! `stokesforge-core` : `stokesforge-core` a dû se mettre à dépendre de `stokesforge-plot`
//! (pour porter un `PlotSpec` sur `NodeKind::Graph`), ce qui aurait fait un
//! cycle Cargo direct avec le sens inverse. `NodeId` a donc été descendu dans
//! `stokesforge-id`, un crate minimal sans dépendance sous les deux — voir
//! `crates/stokesforge-id/src/lib.rs` et `src/lib.rs` de ce crate. Le critère de
//! sortie R2 tient toujours, et plus étroitement qu'avant : ce crate ne voit
//! même plus `Tree` ni `Column`, seulement `NodeId`.

const ALLOWED_DEPENDENCIES: &[&str] = &[
    "stokesforge-id", // identité de nœud seule : dépendance à sens unique
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
