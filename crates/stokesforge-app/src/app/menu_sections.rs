//! La brique commune des sections de menu (#155, grammaire de
//! `docs/menus.md`) : tout menu, barre de menus ou clic droit, range ses
//! entrées en **sections** (action principale, créer, presse-papiers,
//! organiser, supprimer, sortie, `Properties…`) et ne pose un trait fin
//! qu'**entre deux sections présentes** — jamais en tête, en fin ni en double.
//!
//! Pourquoi une brique plutôt que des `ui.separator()` à la main : un menu
//! dont une section dépend de la cible (« Show Data » seulement sur une
//! courbe) laissait un trait orphelin dès que la section disparaissait. Ici
//! l'appelant dit seulement si la section **existe** pour cette cible ; le
//! trait suit. Le seul `ui.separator()` d'un menu est celui de
//! [`section_separator`] (garde `aucun_separateur_brut_dans_un_menu`,
//! `tests/gardes.rs`, sans dérogation).
//!
//! « Absente » (sans objet pour la cible) n'est pas « grisée » (pertinente
//! mais momentanément impossible, avec info-bulle `.disabled`) : une section
//! absente ne s'appelle pas du tout, une section grisée se peint avec ses
//! entrées grisées.

use eframe::egui;

/// Compte les sections déjà peintes d'un menu pour séparer la suivante.
#[derive(Debug, Default)]
pub(super) struct MenuSections {
    painted: usize,
    separators: usize,
}

impl MenuSections {
    pub(super) fn new() -> Self {
        Self::default()
    }

    /// Peint une section si elle `present` pour la cible, précédée d'un trait
    /// fin quand une section l'a déjà été. `paint` n'est pas appelée pour une
    /// section absente.
    pub(super) fn section(
        &mut self,
        ui: &mut egui::Ui,
        present: bool,
        paint: impl FnOnce(&mut egui::Ui),
    ) {
        if !present {
            return;
        }
        if self.painted > 0 {
            section_separator(ui);
            self.separators += 1;
        }
        paint(ui);
        self.painted += 1;
    }
}

/// Le trait fin entre deux sections : le seul séparateur d'un menu.
fn section_separator(ui: &mut egui::Ui) {
    ui.add(egui::Separator::default().spacing(4.0));
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{contexte, image_de};
    use super::*;

    fn separateurs(presentes: &[bool]) -> (usize, usize) {
        let context = contexte();
        let mut sections = MenuSections::new();
        image_de(&context, egui::vec2(300.0, 300.0), Vec::new(), |ui| {
            sections = MenuSections::new();
            for &presente in presentes {
                sections.section(ui, presente, |ui| {
                    ui.label("x");
                });
            }
        });
        (sections.painted, sections.separators)
    }

    #[test]
    fn un_trait_seulement_entre_deux_sections_presentes() {
        assert_eq!(separateurs(&[true, true, true]), (3, 2));
        assert_eq!(separateurs(&[true]), (1, 0));
        assert_eq!(separateurs(&[]), (0, 0));
    }

    #[test]
    fn une_section_absente_ne_laisse_ni_trait_en_tete_ni_en_fin_ni_double() {
        assert_eq!(separateurs(&[false, true, false]), (1, 0));
        assert_eq!(separateurs(&[true, false, false, true]), (2, 1));
        assert_eq!(separateurs(&[false, false]), (0, 0));
    }

    #[test]
    fn une_section_absente_n_appelle_pas_son_contenu() {
        let context = contexte();
        let mut appels = 0;
        image_de(&context, egui::vec2(300.0, 300.0), Vec::new(), |ui| {
            appels = 0;
            let mut sections = MenuSections::new();
            sections.section(ui, false, |_| appels += 1);
            sections.section(ui, true, |_| appels += 1);
        });
        assert_eq!(appels, 1);
    }
}
