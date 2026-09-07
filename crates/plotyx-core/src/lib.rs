//! Modèle de données de PlotYx : arbre de projet (`Folder → Workbook →
//! Worksheet → Column`, `Graph`/`Matrix`/`Note` en feuilles) et rien
//! d'autre — voir `docs/rustrewrite/roadmap.md` phase 1.
//!
//! Règle non négociable, vérifiée par `tests/no_ui_dependency.rs` : ce
//! crate n'importe aucune dépendance UI. Toute mutation passe par
//! `plotyx-commands`, jamais par une méthode publique de ce crate appelée
//! directement hors commande.
//!
//! Le modèle lui-même (`Project`, `Folder`, `Workbook`, `Worksheet`,
//! `Column`) reste à construire — phase 1 de la roadmap.

// Squelette de phase 0 : le modèle réel (phase 1 de la roadmap) n'est pas
// encore construit. Ce test sert de canari CI en attendant.
#[cfg(test)]
mod tests {
    #[test]
    fn crate_compiles_and_links() {
        assert_eq!(2 + 2, 4);
    }
}
