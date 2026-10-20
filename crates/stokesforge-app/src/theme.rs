//! Thème visuel propre à StokesForge.
//!
//! L'application ne suit **pas** le thème clair/sombre du système : elle
//! impose sa propre palette, dérivée de [`egui::Visuals::light`] (jamais du
//! sombre — cf. `docs/application/theme-i18n-systeme.md` § « Thème »). Deux raisons à ce choix :
//!
//! - une appli scientifique se juge sur ses graphes, pas sur son chrome —
//!   `stokesforge-render`/`stokesforge-render-egui` dessinent les tracés sur un fond
//!   clair implicite (axes, grilles, texte noir) ; laisser l'utilisateur
//!   basculer en sombre romprait ce contraste sans qu'aucun code de rendu
//!   n'en soit informé ;
//! - un thème qui change avec le système est un thème qu'on ne peut pas
//!   garantir en capture d'écran ni en support (« chez moi c'est gris, chez
//!   vous c'est bleu nuit »). Fixer la palette une fois pour toutes rend
//!   l'appli identique sur toutes les machines.
//!
//! [`install`] pose cette palette une seule fois, à la construction de la
//! fenêtre (`main.rs`, avant [`crate::app::StokesForgeApp::new`]) — jamais depuis
//! [`crate::app::StokesForgeApp::ui`](eframe::App::ui), qui tournerait sinon à chaque image pour
//! rien.
//!
//! # Barres de défilement : un seul standard
//!
//! Toutes les barres de défilement de l'application ont le même aspect
//! (largeur, couleur, contraste), fixé par [`scroll_style`] et posé une fois
//! pour toutes par [`install`] sur le style global d'egui — donc hérité par
//! chaque `egui::ScrollArea`, `TableBuilder` compris, dans toutes les
//! fenêtres (flottantes et détachées). Pour changer l'aspect de toutes les
//! barres d'un coup, on modifie les constantes `SCROLL_BAR_*` ci-dessous,
//! et nulle part ailleurs.
//!
//! Règle pour le code futur : **ne jamais construire ni modifier un
//! `ScrollStyle` hors de ce module** (`ScrollStyle::solid()`,
//! `spacing_mut().scroll = …`). Une zone qui a besoin de connaître la place
//! prise par une barre (bande réservée, barre déportée) la lit par
//! [`scroll_bar_allocated_width`]. Le test
//! `aucun_scroll_style_hors_du_theme` le verrouille.

use eframe::egui::{self, Color32, Context, CornerRadius, Theme};

/// Teinte d'accent de la marque StokesForge : un vert forêt plutôt que le bleu
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

/// Graisse du gras de l'interface (axe `wght` de Noto Sans Variable, qui va de
/// 100 à 900) : le « semi-gras » 600 de la maquette des onglets (#169).
const BOLD_WEIGHT: f32 = 600.0;

/// **Le** gras de l'interface : l'onglet actif, la ligne sélectionnée d'une
/// liste, un titre de section de formulaire. `egui::RichText::strong` n'est
/// pas un gras — il ne change que la couleur, en l'occurrence l'[`ACCENT`] —
/// et n'est donc jamais employé (`aucun_strong_d_egui_hors_du_theme`). La
/// graisse est une coordonnée de variation de la police embarquée par
/// `stokesforge_render::font_definitions` : avec une police sans axe `wght`
/// (une paire Fira ou Libertinus, une police système) le texte reste en
/// graisse normale, sans erreur.
pub fn bold(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text).variation("wght", BOLD_WEIGHT)
}

/// Les **deux** espacements de l'interface (S-L4, #227) : aucun
/// `ui.add_space(<nombre>)` ne s'écrit hors de ce module
/// (`aucun_add_space_litteral_hors_du_theme`, `tests/gardes.rs`). Le petit
/// écart sépare deux éléments d'un même bloc (une note de son champ, deux
/// lignes d'un message) ; l'écart de section ouvre un bloc nouveau
/// ([`section_title`], bandes d'onglets).
pub const SPACE_ITEM: f32 = 4.0;

/// L'écart de section : voir [`SPACE_ITEM`].
pub const SPACE_SECTION: f32 = 6.0;

/// Largeur d'un champ **à quelques chiffres** (S-L2, #237) : un nombre entier
/// dont la borne est connue et tient en trois chiffres (canal de couleur,
/// pour cent, degré…), sans liste de valeurs (~38 px).
pub const FIELD_WIDTH_DIGITS: f32 = 38.0;

/// Largeur d'un champ réel signé à trois chiffres et une décimale, sans liste
/// (rotation, « -180.5 », #246).
pub const FIELD_WIDTH_SIGNED_DIGITS: f32 = 54.0;

/// Largeur d'un champ **court** (S-L2, #227) : un nombre, une borne, un
/// caractère (~80 px). C'est la largeur de la brique « nombre ».
pub const FIELD_WIDTH_SHORT: f32 = 80.0;

/// Largeur d'un champ **moyen** (S-L2) : un nom, une liste de valeurs courte
/// (~160 px).
pub const FIELD_WIDTH_MEDIUM: f32 = 160.0;

/// Les quatre largeurs nommées d'un champ de formulaire (S-L2) ; la pleine prend
/// toute la place qui reste dans la colonne des champs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldWidth {
    /// [`FIELD_WIDTH_DIGITS`].
    Digits,
    /// [`FIELD_WIDTH_SHORT`].
    Short,
    /// [`FIELD_WIDTH_MEDIUM`].
    Medium,
    /// Toute la largeur disponible.
    Full,
}

impl FieldWidth {
    /// La largeur en points dans `ui` (la pleine lit la place qui reste).
    pub fn points(self, ui: &egui::Ui) -> f32 {
        match self {
            Self::Digits => FIELD_WIDTH_DIGITS,
            Self::Short => FIELD_WIDTH_SHORT,
            Self::Medium => FIELD_WIDTH_MEDIUM,
            Self::Full => ui.available_width(),
        }
    }
}

/// **Le** titre de section d'un formulaire (propriétés du graphe, assistant
/// d'import…) : un espace, puis le libellé en [`bold`]. Le titre suffit à
/// séparer les groupes : jamais de `ui.separator()` juste avant
/// (`aucun_separateur_avant_un_titre_de_section`), jamais un trait seul entre
/// deux blocs (S-L3), et jamais un libellé ordinaire qui ferait office de titre.
pub fn section_title(ui: &mut egui::Ui, text: impl Into<String>) -> egui::Response {
    ui.add_space(SPACE_SECTION);
    ui.label(bold(text))
}

/// Les variations qu'écrit [`bold`], pour mesurer ou reconnaître un gras
/// (largeur réservée à un onglet, tests).
pub fn bold_variations() -> egui::epaint::text::VariationCoords {
    egui::epaint::text::VariationCoords::new([("wght", BOLD_WEIGHT)])
}

/// L'[`ACCENT`] lui-même : texte et trait de l'onglet actif, filet de la
/// ligne sélectionnée d'une liste (#169).
pub fn accent() -> Color32 {
    ACCENT
}

/// Filet qui sépare une bande d'onglets de son contenu (#169).
pub fn tab_rule_color() -> Color32 {
    Color32::from_gray(0xD6)
}

/// Teinte de l'icône d'aide au survol d'un champ (#197) : l'icône neutre,
/// estompée pour rester discrète à côté du champ qu'elle explique.
pub fn help_icon_tint() -> Color32 {
    Color32::from_white_alpha(150)
}

/// Trait d'un onglet survolé, à la place de celui de l'onglet actif (#169).
pub fn tab_hover_mark_color() -> Color32 {
    Color32::from_gray(0xC4)
}

/// Fond de la ligne sélectionnée d'une liste (Project Manager, #169) : plus
/// pâle que `selection.bg_fill`, le filet d'[`ACCENT`] à gauche porte la
/// sélection.
pub fn list_row_selected_fill() -> Color32 {
    ACCENT.gamma_multiply(0.14)
}

/// Fond du volet Results : un cran plus clair que le fond de panneau.
pub fn results_pane_fill(ui: &egui::Ui) -> Color32 {
    ui.visuals()
        .panel_fill
        .gamma_multiply(1.0)
        .lerp_to_gamma(Color32::WHITE, 0.45)
}

/// Fond de la ligne survolée d'une liste (#169).
pub fn list_row_hover_fill() -> Color32 {
    Color32::from_gray(0xEC)
}

/// Contour bleu d'un fichier du système glissé au-dessus de la fenêtre
/// (`app/file_drop.rs`). Distinct, à ce jour, du vert des dépôts internes
/// (colonnes vers un graphe, [`header_letter_row_selected_fill`]) : l'écart
/// est une question ouverte de #170 (point 2), pas un choix arrêté.
pub fn file_drop_outline_color() -> Color32 {
    Color32::from_rgb(0x2a, 0x7f, 0xff)
}

/// Aperçu de l'import : ligne d'en-tête reconnue (nom, sous-en-tête, unité),
/// dans l'esprit du code couleur de l'assistant ASCII d'OriginLab.
pub fn import_header_line_color() -> Color32 {
    Color32::from_rgb(80, 140, 220)
}

/// Aperçu de l'import : sous-en-tête de commentaire ou de x'.
pub fn import_comment_line_color() -> Color32 {
    Color32::from_rgb(200, 80, 200)
}

/// Teinte d'une colonne calculée qui n'est plus à jour (lettre d'en-tête et
/// cellules), #181.
pub fn stale_color(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(230, 170, 70)
    } else {
        Color32::from_rgb(170, 100, 0)
    }
}

/// Courbe de résultat de l'aperçu d'analyse (ajustement, signal corrigé,
/// somme des pics), #181.
pub fn preview_result_color(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(255, 120, 90)
    } else {
        Color32::from_rgb(200, 50, 30)
    }
}

/// Courbe d'aide de l'aperçu d'analyse (ligne de base, fond), #181.
pub fn preview_helper_color(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(120, 200, 255)
    } else {
        Color32::from_rgb(20, 100, 190)
    }
}

/// Repère posé sur l'aperçu d'analyse (point d'ancrage, pic de départ), #181.
pub fn preview_marker_color() -> Color32 {
    Color32::from_rgb(230, 150, 20)
}

/// Fond translucide d'une zone ou d'une plage de l'aperçu d'analyse, #181.
pub fn preview_zone_fill() -> Color32 {
    Color32::from_rgba_unmultiplied(230, 150, 20, 48)
}

/// Couleur du pic `index` (à partir de 1) dans l'aperçu d'analyse, #181.
pub fn preview_peak_color(index: usize) -> Color32 {
    const LIGHT: [(u8, u8, u8); 6] = [
        (31, 119, 180),
        (44, 160, 44),
        (148, 103, 189),
        (140, 86, 75),
        (227, 119, 194),
        (23, 190, 207),
    ];
    let (r, g, b) = LIGHT[index.saturating_sub(1) % LIGHT.len()];
    Color32::from_rgb(r, g, b)
}

/// Cadre d'un champ dont la saisie est **refusée** (#219) : texte illisible,
/// contrainte entre champs, erreur Typst. Le rouge est réservé à ce cas, à une
/// exception **explicite** près : [`r_squared_color`] (`docs/standard-interface.md`).
pub fn field_refused_color() -> Color32 {
    Color32::from_rgb(200, 30, 30)
}

/// Teinte d'un R² faible (#288), selon son palier : orange sous 0,95, rouge
/// dès 0,8 (la boîte de dialogue, sous 0,6, garde ce rouge). Seul rouge permis
/// hors du refus de champ : jeton **dédié**, qui ne sert à rien d'autre. La
/// couleur n'est jamais le seul signal (gras et bulle, `exact_result_label`).
/// Sans palier d'alerte : la couleur du texte ordinaire.
pub fn r_squared_color(
    level: stokesforge_app_state::r_squared::RSquaredLevel,
    dark: bool,
) -> Color32 {
    if level.is_red() {
        if dark {
            Color32::from_rgb(255, 110, 110)
        } else {
            Color32::from_rgb(190, 20, 40)
        }
    } else if level.is_warning() {
        if dark {
            Color32::from_rgb(255, 165, 50)
        } else {
            Color32::from_rgb(195, 95, 0)
        }
    } else if dark {
        Color32::from_gray(220)
    } else {
        Color32::from_gray(30)
    }
}

/// Cadre d'un champ dont la valeur a été **ramenée** à une borne (#219) : teinte
/// d'avertissement, distincte du rouge du refus.
pub fn field_clamped_color() -> Color32 {
    Color32::from_rgb(220, 120, 0)
}

/// Rouge foncé et discret du contour d'un dialogue modal qui vient de
/// refuser un clic ([`crate::app`], `modal_dialog`).
pub fn blocked_flash_color() -> Color32 {
    Color32::from_rgb(150, 28, 28)
}

/// Le bouton d'un geste destructeur (Delete, Discard Changes ; S-R5, #229) :
/// style normal au repos, **rouge** au survol et au focus clavier (egui peint
/// un widget focalisé avec le style `active`). À poser dans un `scope` qui ne
/// contient que ce bouton.
pub fn destructive_button_visuals(visuals: &mut egui::Visuals) {
    let red = visuals.error_fg_color;
    for state in [&mut visuals.widgets.hovered, &mut visuals.widgets.active] {
        state.weak_bg_fill = red;
        state.bg_fill = red;
        state.bg_stroke.color = red;
        state.fg_stroke.color = Color32::WHITE;
    }
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

/// Épaisseur de toutes les barres de défilement, en points. Le défaut
/// d'egui (6 pt en barre pleine, 2 pt en barre flottante au repos) se
/// visait mal et se voyait peu.
const SCROLL_BAR_WIDTH: f32 = 10.0;

/// Espace laissé entre le contenu et la barre, en points.
const SCROLL_BAR_MARGIN: f32 = 2.0;

/// Opacité de la poignée au repos : la poignée prend la couleur du texte
/// des widgets (gris foncé, `widgets.inactive.fg_stroke`), atténuée — bien
/// plus contrastée que le gris très clair (`widgets.inactive.bg_fill`) des
/// barres pleines d'egui, sans être aussi lourde qu'un texte.
const SCROLL_BAR_HANDLE_OPACITY: f32 = 0.45;

/// Opacité de la poignée survolée ou saisie (elle prend alors la couleur de
/// survol, puis l'[`ACCENT`] pendant le glisser).
const SCROLL_BAR_HANDLE_INTERACT_OPACITY: f32 = 0.8;

/// Opacité du fond de la piste (`extreme_bg_color`).
const SCROLL_BAR_TRACK_OPACITY: f32 = 1.0;

/// Le style de **toutes** les barres de défilement de StokesForge — voir la
/// section « Barres de défilement » en tête de module.
///
/// Construit en mode « flottant » d'egui, mais figé : largeur constante
/// (pas d'élargissement au survol), place réservée égale à la barre plus sa
/// marge (elle ne recouvre donc jamais le contenu), et opacités non nulles
/// au repos (elle reste visible sans survol). C'est le seul mode d'egui qui
/// laisse régler l'opacité de la poignée ; en mode « plein », elle est
/// toujours opaque et de la couleur de fond des boutons, qu'on ne peut pas
/// foncer sans foncer aussi tous les boutons.
pub fn scroll_style() -> egui::style::ScrollStyle {
    egui::style::ScrollStyle {
        floating: true,
        bar_width: SCROLL_BAR_WIDTH,
        floating_width: SCROLL_BAR_WIDTH,
        floating_allocated_width: SCROLL_BAR_WIDTH + SCROLL_BAR_MARGIN,
        bar_inner_margin: SCROLL_BAR_MARGIN,
        bar_outer_margin: 0.0,
        handle_min_length: 16.0,
        foreground_color: true,
        dormant_background_opacity: SCROLL_BAR_TRACK_OPACITY,
        active_background_opacity: SCROLL_BAR_TRACK_OPACITY,
        interact_background_opacity: SCROLL_BAR_TRACK_OPACITY,
        dormant_handle_opacity: SCROLL_BAR_HANDLE_OPACITY,
        active_handle_opacity: SCROLL_BAR_HANDLE_OPACITY,
        interact_handle_opacity: SCROLL_BAR_HANDLE_INTERACT_OPACITY,
        ..egui::style::ScrollStyle::floating()
    }
}

/// Place qu'occupe une barre de défilement du standard [`scroll_style`],
/// perpendiculairement à son sens (largeur d'une barre verticale, hauteur
/// d'une barre horizontale), marge comprise.
pub fn scroll_bar_allocated_width() -> f32 {
    scroll_style().allocated_width()
}

/// Pose le thème StokesForge sur `ctx` : verrouille la préférence sur
/// [`Theme::Light`] (empêche egui de re-suivre le système à la moindre
/// notification `winit`), applique la palette maison par-dessus
/// [`egui::Visuals::light`], puis le standard des barres de défilement
/// ([`scroll_style`]).
pub fn install(ctx: &Context) {
    ctx.set_theme(Theme::Light);
    ctx.set_visuals(visuals());
    ctx.all_styles_mut(|style| style.spacing.scroll = scroll_style());
    // Chargeurs d'images (SVG des icônes, `icons.rs`).
    egui_extras::install_image_loaders(ctx);
}

/// Construit la palette StokesForge : un `Visuals::light()` standard, avec juste
/// les teintes qui portent l'identité de l'appli (sélection, liens, contour
/// des widgets actifs) redéfinies — le reste (gris de fond, contrastes de
/// texte, tailles) vient tel quel d'egui, pour ne pas réinventer ce que la
/// bibliothèque a déjà calibré pour la lisibilité.
fn visuals() -> egui::Visuals {
    let mut visuals = egui::Visuals::light();

    visuals.hyperlink_color = ACCENT;
    visuals.selection.bg_fill = ACCENT.gamma_multiply(0.35);
    visuals.selection.stroke.color = ACCENT;

    // Contour des boutons/onglets actifs (dont la fenêtre flottante
    // focalisée) : reprend l'accent plutôt que le bleu par défaut, pour que
    // la disposition ressemble au reste de l'appli.
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
        assert!(
            !visuals().dark_mode,
            "le thème StokesForge doit rester clair"
        );
    }

    /// Le rouge de R² (#288) est un jeton à part : distinct de celui du refus de
    /// champ, et chaque palier a sa teinte, en clair comme en sombre.
    #[test]
    fn la_teinte_de_r_carre_a_un_jeton_propre_par_palier() {
        use stokesforge_app_state::r_squared::RSquaredLevel::{Dialog, Good, Orange, Red};
        for dark in [false, true] {
            let orange = r_squared_color(Orange, dark);
            let red = r_squared_color(Red, dark);
            assert_ne!(orange, red);
            assert_eq!(
                red,
                r_squared_color(Dialog, dark),
                "la boîte garde le rouge"
            );
            assert_ne!(red, field_refused_color());
            assert_ne!(orange, r_squared_color(Good, dark));
        }
        assert_ne!(r_squared_color(Red, true), r_squared_color(Red, false));
    }

    /// Tous les paliers, toutes les teintes : `Good` garde la couleur du texte
    /// ordinaire (ni orange ni rouge), et aucune teinte d'alerte ne vaut le
    /// rouge de refus de champ.
    #[test]
    fn la_teinte_de_r_carre_distingue_tous_les_paliers() {
        use stokesforge_app_state::r_squared::RSquaredLevel::{Dialog, Good, Orange, Red};
        for dark in [false, true] {
            let teintes = [
                r_squared_color(Good, dark),
                r_squared_color(Orange, dark),
                r_squared_color(Red, dark),
            ];
            for (i, a) in teintes.iter().enumerate() {
                for b in &teintes[i + 1..] {
                    assert_ne!(a, b, "dark = {dark}");
                }
                assert_ne!(*a, field_refused_color(), "dark = {dark}");
                assert_ne!(*a, field_clamped_color(), "dark = {dark}");
            }
            assert_eq!(r_squared_color(Dialog, dark), teintes[2]);
        }
        assert_ne!(
            r_squared_color(Orange, true),
            r_squared_color(Orange, false)
        );
        assert_ne!(r_squared_color(Good, true), r_squared_color(Good, false));
    }

    /// Contraste WCAG minimal 3:1 (texte en gras) des teintes d'alerte sur les
    /// fonds clair (blanc) et sombre (gris 27, fond sombre d'egui).
    #[test]
    fn la_teinte_de_r_carre_contraste_sur_fond_clair_et_sombre() {
        use stokesforge_app_state::r_squared::RSquaredLevel::{Good, Orange, Red};
        use stokesforge_render::color::contrast_ratio;
        let rgb = |c: Color32| (c.r(), c.g(), c.b());
        for (dark, fond) in [(false, (255, 255, 255)), (true, (27, 27, 27))] {
            for niveau in [Good, Orange, Red] {
                let ratio = contrast_ratio(rgb(r_squared_color(niveau, dark)), fond);
                assert!(
                    ratio >= 3.0,
                    "{niveau:?} dark = {dark} : contraste {ratio:.2} < 3"
                );
            }
        }
    }

    #[test]
    fn install_pose_le_standard_des_barres_de_defilement() {
        let ctx = Context::default();
        install(&ctx);
        assert_eq!(ctx.style_of(Theme::Light).spacing.scroll, scroll_style());
        assert_eq!(
            scroll_bar_allocated_width(),
            SCROLL_BAR_WIDTH + SCROLL_BAR_MARGIN
        );
    }

    /// Garde-fou du standard : aucun fichier de `stokesforge-app` hors de ce
    /// module ne construit ni ne modifie un `ScrollStyle`. Une barre au style
    /// local échapperait au réglage unique des constantes `SCROLL_BAR_*`.
    #[test]
    fn aucun_scroll_style_hors_du_theme() {
        let fautifs = styles_de_defilement_hors_du_theme(&crate::sources::sources());
        assert!(
            fautifs.is_empty(),
            "style de barre de défilement construit hors de theme.rs — passer par \
             theme::scroll_style / theme::scroll_bar_allocated_width : {fautifs:?}"
        );
    }

    /// Les lignes qui touchent un `ScrollStyle` hors de `theme.rs`,
    /// `src/chemin:ligne` (commentaires exclus, tests compris). Toutes les
    /// sources de `src/`, sous-dossiers compris (#116).
    fn styles_de_defilement_hors_du_theme(sources: &[crate::sources::Fichier]) -> Vec<String> {
        let mut fautifs = Vec::new();
        for fichier in sources {
            if fichier.chemin == "theme.rs" {
                continue;
            }
            for (index, ligne) in fichier.code().lines().enumerate() {
                if ligne.contains("ScrollStyle") || ligne.contains(".scroll =") {
                    fautifs.push(format!("src/{}:{}", fichier.chemin, index + 1));
                }
            }
        }
        fautifs
    }

    /// Garde-fou du gras : `RichText::strong` / `Ui::strong` d'egui ne
    /// graissent rien (une autre couleur seulement) ; tout gras passe par
    /// [`bold`]. Toutes les sources de `src/`, commentaires exclus.
    #[test]
    fn aucun_strong_d_egui_hors_du_theme() {
        let fautifs = strong_d_egui(&crate::sources::sources());
        assert!(
            fautifs.is_empty(),
            "`strong()` d'egui ne met pas en gras — passer par theme::bold : {fautifs:?}"
        );
    }

    /// Les lignes qui appellent `.strong()` hors de `theme.rs`.
    fn strong_d_egui(sources: &[crate::sources::Fichier]) -> Vec<String> {
        let mut fautifs = Vec::new();
        for fichier in sources {
            if fichier.chemin == "theme.rs" {
                continue;
            }
            for (index, ligne) in fichier.code().lines().enumerate() {
                if ligne.contains(".strong()") || ligne.contains("ui.strong(") {
                    fautifs.push(format!("src/{}:{}", fichier.chemin, index + 1));
                }
            }
        }
        fautifs
    }

    /// Le témoin : un `.strong()` hors de `theme.rs` est vu, [`bold`] non.
    #[test]
    fn la_garde_du_gras_voit_un_strong() {
        let fichier = |code: &str| crate::sources::Fichier {
            chemin: "app/x.rs".to_string(),
            texte: code.to_string(),
            de_test: false,
        };
        assert_eq!(
            strong_d_egui(&[fichier("let t = RichText::new(a).strong();")]),
            ["src/app/x.rs:1"]
        );
        assert_eq!(
            strong_d_egui(&[fichier("ui.strong(a);")]),
            ["src/app/x.rs:1"]
        );
        assert!(strong_d_egui(&[fichier("ui.label(theme::bold(a));")]).is_empty());
    }

    /// [`bold`] pose la graisse sur le texte, et rien d'autre ne la pose.
    #[test]
    fn bold_pose_la_graisse() {
        let job = egui::WidgetText::from(bold("x")).into_layout_job(
            &egui::Style::default(),
            egui::FontSelection::Default,
            egui::Align::LEFT,
        );
        assert_eq!(job.sections[0].format.coords, bold_variations());
        let job = egui::WidgetText::from(egui::RichText::new("x")).into_layout_job(
            &egui::Style::default(),
            egui::FontSelection::Default,
            egui::Align::LEFT,
        );
        assert_ne!(job.sections[0].format.coords, bold_variations());
    }

    /// Avec les polices embarquées, [`bold`] graisse vraiment les glyphes
    /// (texte plus large qu'en graisse normale) — pas seulement une
    /// coordonnée posée dans le vide : c'est ce qui prouve que la police du
    /// projet a un axe `wght` que epaint sait suivre.
    #[test]
    fn bold_elargit_vraiment_le_texte_avec_les_polices_embarquees() {
        let ctx = Context::default();
        ctx.set_fonts(stokesforge_render_egui::font_definitions());
        install(&ctx);
        let mut largeurs = (0.0, 0.0);
        let mut sortie = ctx.run_ui(egui::RawInput::default(), |ui| {
            largeurs = (
                ui.label(egui::RichText::new("Zone de tracé")).rect.width(),
                ui.label(bold("Zone de tracé")).rect.width(),
            );
        });
        sortie.textures_delta.clear();
        assert!(
            largeurs.1 > largeurs.0 + 1.0,
            "gras {} contre normal {}",
            largeurs.1,
            largeurs.0
        );
    }

    /// Le témoin : un style de défilement posé dans un module de `src/app/` est
    /// vu ; un commentaire et `theme.rs` lui-même passent.
    #[test]
    fn la_garde_du_defilement_voit_un_style_hors_d_app_rs() {
        let fichier = |chemin: &str, texte: &str| crate::sources::Fichier {
            chemin: chemin.to_string(),
            texte: texte.to_string(),
            de_test: false,
        };
        let sources = [
            fichier(
                "app/grille.rs",
                "// ScrollStyle\nfn f(s: &mut Style) {\n    s.spacing.scroll = ScrollStyle::thin();\n}",
            ),
            fichier("app/notes.rs", "fn f(s: &mut Spacing) { s.scroll = x; }"),
            fichier("theme.rs", "fn scroll_style() -> ScrollStyle { todo!() }"),
        ];
        assert_eq!(
            styles_de_defilement_hors_du_theme(&sources),
            ["src/app/grille.rs:3", "src/app/notes.rs:1"]
        );
    }
}
