//! Thème visuel propre à StokesForge.
//!
//! L'application ne suit **pas** le thème clair/sombre du système : elle
//! impose sa propre palette, dérivée de [`egui::Visuals::light`] (jamais du
//! sombre — cf. `docs/application.md` § « Thème »). Deux raisons à ce choix :
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

/// Espace laissé au-dessus d'un titre de section de formulaire.
const SECTION_SPACING: f32 = 6.0;

/// **Le** titre de section d'un formulaire (propriétés du graphe, assistant
/// d'import…) : un espace, puis le libellé en [`bold`]. Le titre suffit à
/// séparer les groupes : jamais de `ui.separator()` juste avant
/// (`aucun_separateur_avant_un_titre_de_section`), et jamais un libellé
/// ordinaire qui ferait office de titre.
pub fn section_title(ui: &mut egui::Ui, text: impl Into<String>) -> egui::Response {
    ui.add_space(SECTION_SPACING);
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

/// Rouge foncé et discret du contour d'un dialogue modal qui vient de
/// refuser un clic ([`crate::app`], `modal_dialog`).
pub fn blocked_flash_color() -> Color32 {
    Color32::from_rgb(150, 28, 28)
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
