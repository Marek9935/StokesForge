//! Thème visuel propre à PlotYx.
//!
//! L'application ne suit **pas** le thème clair/sombre du système : elle
//! impose sa propre palette, dérivée de [`egui::Visuals::light`] (jamais du
//! sombre — cf. `docs/application.md` § « Thème »). Deux raisons à ce choix :
//!
//! - une appli scientifique se juge sur ses graphes, pas sur son chrome —
//!   `plotyx-render`/`plotyx-render-egui` dessinent les tracés sur un fond
//!   clair implicite (axes, grilles, texte noir) ; laisser l'utilisateur
//!   basculer en sombre romprait ce contraste sans qu'aucun code de rendu
//!   n'en soit informé ;
//! - un thème qui change avec le système est un thème qu'on ne peut pas
//!   garantir en capture d'écran ni en support (« chez moi c'est gris, chez
//!   vous c'est bleu nuit »). Fixer la palette une fois pour toutes rend
//!   l'appli identique sur toutes les machines.
//!
//! [`install`] pose cette palette une seule fois, à la construction de la
//! fenêtre (`main.rs`, avant [`crate::app::PlotyxApp::new`]) — jamais depuis
//! [`crate::app::PlotyxApp::ui`], qui tournerait sinon à chaque image pour
//! rien.

use eframe::egui::{self, Color32, Context, CornerRadius, Theme};

/// Teinte d'accent de la marque PlotYx : un bleu-pétrole plutôt que le bleu
/// par défaut d'egui, utilisé pour la sélection, les liens et les éléments
/// actifs — assez sombre pour rester lisible en texte sur fond clair.
const ACCENT: Color32 = Color32::from_rgb(0x1B, 0x6C, 0x7A);

/// Fond de la rangée des lettres de colonnes (A/B/C…, avec leur type) de
/// l'en-tête empilée de la grille — la distingue visuellement des quatre
/// rangées de champs qui la suivent (nom long/unité/commentaire/formule).
/// Dérivé de [`ACCENT`] comme le reste de la palette de sélection, plutôt
/// qu'une teinte ad hoc — un bleu-gris clair proche de la référence visuelle
/// Qt de PlotYx.
pub fn header_letter_row_fill() -> Color32 {
    ACCENT.gamma_multiply(0.18)
}

/// Pose le thème PlotYx sur `ctx` : verrouille la préférence sur
/// [`Theme::Light`] (empêche egui de re-suivre le système à la moindre
/// notification `winit`), puis applique la palette maison par-dessus
/// [`egui::Visuals::light`].
pub fn install(ctx: &Context) {
    ctx.set_theme(Theme::Light);
    ctx.set_visuals(visuals());
}

/// Construit la palette PlotYx : un `Visuals::light()` standard, avec juste
/// les teintes qui portent l'identité de l'appli (sélection, liens, contour
/// des widgets actifs) redéfinies — le reste (gris de fond, contrastes de
/// texte, tailles) vient tel quel d'egui, pour ne pas réinventer ce que la
/// bibliothèque a déjà calibré pour la lisibilité.
fn visuals() -> egui::Visuals {
    let mut visuals = egui::Visuals::light();

    visuals.hyperlink_color = ACCENT;
    visuals.selection.bg_fill = ACCENT.gamma_multiply(0.35);
    visuals.selection.stroke.color = ACCENT;

    // Contour des boutons/onglets actifs (dont l'onglet focalisé de
    // `egui_dock`) : reprend l'accent plutôt que le bleu par défaut, pour que
    // la disposition dockable ressemble au reste de l'appli.
    visuals.widgets.active.bg_stroke.color = ACCENT;
    visuals.widgets.active.fg_stroke.color = ACCENT;
    visuals.widgets.hovered.bg_stroke.color = ACCENT;

    // Coins légèrement moins arrondis que le défaut egui (2px) : plus proche
    // de l'esthétique dense et tabulaire d'un logiciel d'analyse de données
    // que d'une appli mobile.
    let corner_radius = CornerRadius::same(3);
    visuals.window_corner_radius = corner_radius;
    visuals.menu_corner_radius = corner_radius;
    visuals.widgets.noninteractive.corner_radius = corner_radius;
    visuals.widgets.inactive.corner_radius = corner_radius;
    visuals.widgets.hovered.corner_radius = corner_radius;
    visuals.widgets.active.corner_radius = corner_radius;
    visuals.widgets.open.corner_radius = corner_radius;

    visuals
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Garde-fou : si un jour quelqu'un fait dériver [`visuals`] de
    /// `Visuals::dark()` par erreur (copier-coller, refactor...), ce test
    /// échoue plutôt que de laisser le thème partir en sombre en silence.
    #[test]
    fn base_theme_is_light() {
        assert!(!visuals().dark_mode, "le thème PlotYx doit rester clair");
    }
}
