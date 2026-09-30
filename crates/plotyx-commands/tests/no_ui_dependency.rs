//! Même garde-fou que `plotyx-core` : `plotyx-commands` ne doit jamais gagner
//! de dépendance UI. Plutôt que de deviner ce qu'est "une dépendance UI", ce
//! test liste les seules dépendances autorisées — toute addition dans
//! `Cargo.toml` doit être ajoutée ici explicitement, ce qui force une décision
//! consciente plutôt qu'un import qui se glisse.

const ALLOWED_DEPENDENCIES: &[&str] = &[
    "plotyx-core", // le modèle : dépendance à sens unique, sans surface UI
    "plotyx-plot", // `PlotSpec`, porté par `Command::AddGraph`/`SetPlotSpec` ; sans dépendance de dessin (invariant 3)
    "thiserror",   // erreurs typées, sans surface UI
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
