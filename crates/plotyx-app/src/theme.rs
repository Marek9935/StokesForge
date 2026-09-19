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

/// Teinte d'accent de la marque PlotYx : un vert forêt plutôt que le bleu
/// par défaut d'egui, utilisé pour la sélection, les liens et les éléments
/// actifs — assez sombre pour rester lisible en texte sur fond clair.
const ACCENT: Color32 = Color32::from_rgb(0x1E, 0x7A, 0x3C);

/// Texte de la gouttière de la grille (lettres de colonne, « # », numéros de
/// ligne, libellés de champ d'en-tête) — nettement plus sombre qu'[`ACCENT`]
/// : la gouttière porte déjà un fond teinté ([`header_letter_row_fill`] et
/// consorts), un texte à la même saturation que son propre fond s'y lit mal.
/// Volontairement indépendant de `Visuals::strong_text_color()` (qui vaut
/// `widgets.active.fg_stroke.color`, donc littéralement [`ACCENT`]) plutôt
/// que de foncer cette teinte globale : elle sert aussi au texte « actif »
/// d'éléments sans rapport avec la gouttière (boutons, onglets).
const GUTTER_TEXT: Color32 = Color32::from_rgb(0x0F, 0x4A, 0x24);

/// Couleur de texte de la gouttière — voir [`GUTTER_TEXT`].
pub fn gutter_text_color() -> Color32 {
    GUTTER_TEXT
}

/// Fond de repos de la gouttière de la grille (lettres de colonnes, « # »,
/// numéros de ligne, libellés de champ d'en-tête) — la distingue visuellement
/// des données elles-mêmes. Dérivé de [`ACCENT`] comme le reste de la
/// palette de sélection, mais très dilué : une teinte discrète plutôt qu'une
/// vraie case pleine couleur, qui écraserait le reste de la grille.
pub fn header_letter_row_fill() -> Color32 {
    ACCENT.gamma_multiply(0.10)
}

/// Fond de la gouttière quand elle est *concernée* par la sélection en cours
/// sans être elle-même la cible du clic — la colonne ou la ligne d'une ou
/// plusieurs cellules de données sélectionnées, par exemple. Un cran plus
/// soutenu que [`header_letter_row_fill`] (le repos), sans atteindre
/// l'intensité d'une sélection directe de la gouttière elle-même (voir
/// [`header_letter_row_selected_fill`]) — trois paliers, comme un tableur :
/// rien, « ta ligne/colonne est concernée », « c'est *toi* qui es
/// sélectionné(e) ».
pub fn header_letter_row_touched_fill() -> Color32 {
    ACCENT.gamma_multiply(0.24)
}

/// Fond de la gouttière quand elle est elle-même directement sélectionnée
/// (clic sur une lettre de colonne, un numéro de ligne, un libellé de champ
/// d'en-tête, ou sur « # » pour tout le tableau) — le palier le plus soutenu
/// des trois, voir [`header_letter_row_touched_fill`].
pub fn header_letter_row_selected_fill() -> Color32 {
    ACCENT.gamma_multiply(0.42)
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
