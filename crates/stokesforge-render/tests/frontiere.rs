//! Le socle de rendu voit `epaint`, pas `egui`.
//!
//! C'est toute la raison d'être de ce crate : si sa liste de dépendances
//! gagnait `egui`, `stokesforge-render-svg` le gagnerait aussi, et l'export
//! cesserait de pouvoir se tester sans serveur graphique. Même discipline
//! que `stokesforge-core/tests/no_ui_dependency.rs` : on liste ce qui est permis
//! plutôt que de deviner ce qui est interdit.

const AUTORISEES: &[&str] = &[
    "stokesforge-core", // ResolvedPlot, Tree, Column
    "stokesforge-plot", // PlotSpec
    "plotters",         // sans ses fonts (default-features = false)
    "plotters-backend", // BackendTextStyle, DrawingBackend
    "epaint",           // la mesure, et rien d'autre de la pile UI
    // Résolution d'une police système par son nom (`fonts.rs`, embed de
    // polices du 2026-09-13) : ni egui ni plotters-svg, juste une lecture du
    // système de fichiers — même version que celle qu'`usvg` verrouille déjà
    // ailleurs dans le workspace (`stokesforge-render-svg` l'atteint par
    // réexport, pas en direct, pour ne pas la déclarer deux fois).
    "fontdb",
    // R8, feature `typst` (optionnelle, absente de `default`) : le `World`
    // Typst, la compilation et le cache générique vivent ici précisément
    // pour que ni `stokesforge-render-egui` ni `stokesforge-render-svg` n'aient à
    // réimplémenter le `World` chacun de leur côté
    // (`architecture-choisie.md` §8 — « où va quoi »). Contrairement à
    // `egui` ou `plotters-svg`, `typst`/`typst-layout` sont voulus ici, pas
    // un défaut qui se serait glissé.
    "typst",
    "typst-layout",
];

#[test]
fn seules_les_dependances_prevues_sont_declarees() {
    // Ce crate est la référence commune aux deux backends : il ne doit
    // connaître ni egui ni plotters-svg, sous aucune forme de dépendance
    // (normale, conditionnelle, renommée) — lecture commune, issue #24.
    stokesforge_arch_guard::assert_only_allowed_dependencies(
        env!("CARGO_MANIFEST_DIR"),
        AUTORISEES,
    );
}

#[test]
fn plotters_est_pris_sans_ses_fonts() {
    // `default-features = false` élimine font-kit, image, chrono et le scan
    // des fonts système (`architecture-choisie.md` §4.4). Le retrait n'est
    // sûr que parce que les deux backends surchargent `estimate_text_size` —
    // et il faut qu'il reste en place, sinon Plotters se remettrait à
    // mesurer tout seul, avec d'autres polices que l'écran.
    let manifest: toml::Value =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
            .expect("Cargo.toml lisible")
            .parse()
            .expect("Cargo.toml valide");
    let plotters = manifest
        .get("dependencies")
        .and_then(|d| d.get("plotters"))
        .expect("dépendance plotters");
    assert_eq!(
        plotters.get("default-features").and_then(|v| v.as_bool()),
        Some(false),
        "plotters a retrouvé ses features par défaut : les fonts système \
         reviennent, et avec elles une seconde source de mesure"
    );
}
