//! L'aspect commun d'un champ à liste (#234, S-N1/S-N9) : un cadre unique, une
//! flèche **pleine à l'intérieur**, à droite — celle de la `ComboBox` d'egui.
//!
//! Une seule différence entre les deux familles, voulue : le **fond**.
//! [`editable_fill`] (fond d'un champ de saisie du thème, blanc en clair) pour
//! ce qui se tape au clavier — le champ nombre à liste —, [`list_only_fill`]
//! (fond des boutons, gris) pour un choix fermé. Les deux viennent des
//! `Visuals` posés par `theme::install`, jamais d'une couleur en dur
//! (`aucune_couleur_en_dur_hors_du_theme`).
//!
//! La `ComboBox` garde son propre dessin (egui) ; ce module en reprend la
//! flèche et les fonds pour que `number_field` n'en diverge plus.

use eframe::egui;

/// Place réservée à droite du texte pour la flèche, en points.
pub const ARROW_ROOM: f32 = 18.0;

/// Fond d'un champ éditable au clavier.
pub fn editable_fill(visuals: &egui::Visuals) -> egui::Color32 {
    visuals.extreme_bg_color
}

/// Fond d'une liste qui seule choisit (celui d'un bouton ou d'une `ComboBox`).
pub fn list_only_fill(visuals: &egui::Visuals) -> egui::Color32 {
    visuals.widgets.inactive.weak_bg_fill
}

/// Le rectangle de la flèche, à l'intérieur et à droite de `field`.
pub fn arrow_rect(field: egui::Rect) -> egui::Rect {
    egui::Rect::from_min_max(
        egui::pos2(field.right() - ARROW_ROOM, field.top()),
        field.right_bottom(),
    )
}

/// Peint la flèche pleine (triangle vers le bas) dans `rect`, teinte du survol
/// comprise.
pub fn paint_arrow(ui: &egui::Ui, rect: egui::Rect, response: &egui::Response) {
    let color = ui.style().interact(response).fg_stroke.color;
    let triangle = egui::Rect::from_center_size(
        rect.center(),
        egui::vec2(rect.width() * 0.45, rect.height() * 0.3),
    );
    ui.painter().add(egui::Shape::convex_polygon(
        vec![
            triangle.left_top(),
            triangle.right_top(),
            triangle.center_bottom(),
        ],
        color,
        egui::Stroke::NONE,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_deux_fonds_different_et_viennent_du_theme() {
        for visuals in [egui::Visuals::light(), egui::Visuals::dark()] {
            assert_ne!(editable_fill(&visuals), list_only_fill(&visuals));
            assert_eq!(editable_fill(&visuals), visuals.extreme_bg_color);
            assert_eq!(
                list_only_fill(&visuals),
                visuals.widgets.inactive.weak_bg_fill
            );
        }
    }

    #[test]
    fn la_fleche_est_dans_le_cadre_du_champ() {
        let champ = egui::Rect::from_min_size(egui::pos2(10.0, 5.0), egui::vec2(80.0, 20.0));
        let fleche = arrow_rect(champ);
        assert!(champ.contains_rect(fleche));
        assert_eq!(fleche.right(), champ.right());
    }
}
