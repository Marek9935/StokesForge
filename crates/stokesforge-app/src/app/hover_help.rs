//! Aide au survol d'un champ (#197) : une petite icône grisée posée **tout à
//! droite** du champ qu'elle explique, qui montre son rappel au survol.
//!
//! Elle remplace le mémo repliable « Formula syntax » qui s'ouvrait sous les
//! champs : on ne voyait pas à quel champ il se rapportait, et il prenait de
//! la hauteur dans des fenêtres déjà denses. L'aide est une brique commune —
//! un champ qui a besoin d'un rappel appelle [`hover_help`] avec le contenu de
//! la bulle, plutôt que de recopier icône, teinte et survol.
//!
//! Une bulle de survol (`Response::on_hover_ui`), pas une fenêtre surgissante :
//! la fenêtre des propriétés est un dialogue modal où egui ne retient qu'une
//! fenêtre surgissante à la fois (#9), alors qu'une info-bulle vit dans sa
//! propre couche.

use eframe::egui;

use crate::icons::{icon_image, Icon, IconRole};
use crate::theme;

/// Pose l'icône d'aide au bout de la rangée en cours ; `contents` peint la
/// bulle qui s'affiche au survol. Rend la réponse de l'icône.
pub(crate) fn hover_help(
    ui: &mut egui::Ui,
    contents: impl FnOnce(&mut egui::Ui),
) -> egui::Response {
    ui.add(
        icon_image(Icon::Help, IconRole::Neutral)
            .tint(theme::help_icon_tint())
            .sense(egui::Sense::hover()),
    )
    .on_hover_ui(contents)
}

/// Le rappel d'un champ en une phrase (#225, M1) : l'icône « ? » dont la bulle
/// n'est que `text`.
pub(crate) fn help_text(ui: &mut egui::Ui, text: impl Into<String>) -> egui::Response {
    let text = text.into();
    hover_help(ui, |ui| {
        ui.label(text);
    })
}

/// Peint un champ (`add`) et pose l'icône « ? » de son rappel juste après, sur
/// la même rangée (#225, M2 : jamais de bulle nue sur un champ). Rend ce que
/// rend `add`, pour que `.changed()` reste lisible à l'appel.
pub(crate) fn help_beside<R>(
    ui: &mut egui::Ui,
    text: impl Into<String>,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    ui.horizontal(|ui| {
        let out = add(ui);
        help_text(ui, text);
        out
    })
    .inner
}
