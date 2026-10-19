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
//!
//! Un choix exclusif dans un menu (Type ▸, Role ▸, langue, texte du SVG) se
//! peint par [`menu_choice`] : une **coche seule** devant l'option courante,
//! sans radio ni fond de sélection (S-C3, #227).

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

/// Une option d'un choix exclusif dans un menu (S-C3, `docs/menus.md`) : une
/// coche dans la marge devant l'option courante, rien devant les autres —
/// jamais un radio ni le fond d'un bouton sélectionné. La marge est réservée
/// pour toutes les options, qui restent alignées. Au survol, le fond d'une
/// entrée de menu. L'accessibilité la décrit comme un bouton radio, coché ou
/// non.
pub(super) fn menu_choice(
    ui: &mut egui::Ui,
    selected: bool,
    text: impl Into<egui::WidgetText>,
) -> egui::Response {
    let spacing = ui.spacing();
    let (padding, mark, gap) = (
        spacing.button_padding,
        spacing.icon_width,
        spacing.icon_spacing,
    );
    let galley = text.into().into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        egui::TextStyle::Button,
    );
    let size = egui::vec2(
        padding.x * 2.0 + mark + gap + galley.size().x,
        galley.size().y.max(mark) + padding.y * 2.0,
    );
    let width = size.x.max(ui.available_width());
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, size.y), egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::RadioButton,
            ui.is_enabled(),
            selected,
            galley.text(),
        )
    });
    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact(&response);
        if response.hovered() || response.has_focus() {
            ui.painter()
                .rect_filled(rect, visuals.corner_radius, visuals.weak_bg_fill);
        }
        let box_rect = egui::Rect::from_center_size(
            egui::pos2(rect.left() + padding.x + mark / 2.0, rect.center().y),
            egui::Vec2::splat(mark),
        )
        .shrink(mark / 5.0);
        if selected {
            ui.painter().add(egui::Shape::line(
                vec![
                    egui::pos2(box_rect.left(), box_rect.center().y),
                    egui::pos2(box_rect.center().x - 1.0, box_rect.bottom()),
                    egui::pos2(box_rect.right(), box_rect.top()),
                ],
                visuals.fg_stroke,
            ));
        }
        let text_pos = egui::pos2(
            rect.left() + padding.x + mark + gap,
            rect.center().y - galley.size().y / 2.0,
        );
        ui.painter().galley(text_pos, galley, visuals.text_color());
    }
    response
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{contexte, image, image_de, textes};
    use super::*;

    /// S-C3 : une coche (un trait de trois points) devant la seule option
    /// courante, aucun cercle de radio ni fond de sélection, et les libellés
    /// des options alignés. Témoin : sans option courante, aucune coche.
    #[test]
    fn un_choix_de_menu_coche_seulement_l_option_courante() {
        let context = contexte();
        let coches = |courante: Option<usize>| {
            let formes = image(&context, Vec::new(), |ui| {
                for (index, texte) in ["Un", "Deux"].into_iter().enumerate() {
                    menu_choice(ui, courante == Some(index), texte);
                }
            });
            let traits = formes
                .iter()
                .filter(|forme| {
                    matches!(&forme.shape, egui::Shape::Path(chemin) if chemin.points.len() == 3)
                })
                .count();
            let ronds = formes
                .iter()
                .filter(|forme| matches!(forme.shape, egui::Shape::Circle(_)))
                .count();
            (traits, ronds, textes(&formes))
        };
        let (traits, ronds, peints) = coches(Some(1));
        assert_eq!(traits, 1, "une coche, devant l'option courante");
        assert_eq!(ronds, 0, "aucun radio");
        let un = peints.iter().find(|t| t.texte == "Un").unwrap().centre;
        let deux = peints.iter().find(|t| t.texte == "Deux").unwrap().centre;
        assert!(deux.y > un.y);
        assert_eq!(
            coches(None).0,
            0,
            "témoin : sans option courante, pas de coche"
        );
    }

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
