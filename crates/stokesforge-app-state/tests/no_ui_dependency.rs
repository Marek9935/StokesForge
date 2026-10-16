//! Garde-fou de #263 (chantier #254) : `stokesforge-app-state` — l'état de
//! l'application, ses textes et ses erreurs mises en phrase — ne doit **jamais**
//! dépendre d'une bibliothèque d'interface. C'est toute sa raison d'être : que la
//! frontière « egui ne sert qu'à dessiner » tienne par le compilateur, et que
//! les tests de l'état (`tests/shell.rs`…) compilent sans fenêtre.
//!
//! Plutôt que de deviner ce qu'est « une dépendance UI », ce test liste les
//! seules dépendances autorisées : toute addition dans `Cargo.toml` doit être
//! ajoutée ici explicitement, ce qui force une décision consciente. Ni `egui`,
//! ni `eframe`, ni `winit`, ni `wgpu` — ni `stokesforge-clipboard` ni
//! `stokesforge-render-egui`, dont l'un tire winit (patch smithay) et l'autre
//! egui : l'offre de la figure au presse-papiers reste dans `stokesforge-app`.

const ALLOWED_DEPENDENCIES: &[&str] = &[
    "stokesforge-analysis", // formulaires d'analyse : bornes, prérequis, réglages
    "stokesforge-core",     // le modèle
    "stokesforge-commands", // le bus de commandes : tout passe par lui
    "stokesforge-import",   // recettes d'import, sans interface
    "stokesforge-importer", // fichiers d'appareil → colonnes du modèle
    "stokesforge-session",  // l'acteur, seul détenteur du `&mut Project`
    "stokesforge-persistence", // format `.stf`
    "stokesforge-plot",     // description native des graphes, sans rendu
    // `RenderError`, les formules, la géométrie de graphe : générique sur le
    // backend, ne voit qu'`epaint` pour la mesure de texte (choix assumé)
    "stokesforge-render",
    "stokesforge-render-svg", // export SVG/PDF/PNG, sans serveur graphique
    // pont Python, feature `python` seulement
    "stokesforge-script",
    "stokesforge-python",
    // sélecteur de fichiers natif (`file_picker`) : n'apporte ici que
    // `raw-window-handle`, ni egui ni winit
    "rfd",
    "directories", // dossier de configuration (préférences, couleurs)
    "serde",       // préférences et couleurs enregistrées
    "serde_json",
    // interface multilingue (Fluent) : le catalogue et `tr!`
    "i18n-embed",
    "i18n-embed-fl",
    "rust-embed",
    "fluent-syntax", // pseudo-langue `en-XA`
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
