//! Le pipeline Plotters, **écrit une fois pour les deux backends**.
//!
//! # Pourquoi ce module n'est pas dans chacun des deux crates de backend
//!
//! Le critère de fin du chantier est « axes au même endroit, vérifié par un
//! test » (`roadmap-rendu.md`). Un test peut constater un désaccord ; il ne
//! peut pas empêcher qu'il naisse. Si le backend egui et le backend SVG
//! appelaient chacun leur `ChartBuilder`, la moindre divergence de marge, de
//! taille de police ou d'ordre d'appel deviendrait un désaccord à trouver
//! après coup. Ici il n'y a **qu'une** suite d'appels Plotters, générique sur
//! `DB: DrawingBackend`, et les deux backends ne diffèrent plus que par la
//! façon de poser des pixels.
//!
//! C'est la transposition de la règle que la phase R5 se donne — « aucune
//! fonctionnalité n'entre dans un seul backend » — au niveau où elle est
//! gratuite : la structure, plutôt que la discipline. **R5 n'a eu à écrire
//! aucune ligne de rendu dans un backend** : tout ce que la phase ajoute —
//! types de tracé, échelles, légende, panneaux, décimation — est ici.
//!
//! # Périmètre, phase R5
//!
//! Le `PlotSpec` réel : les six [`stokesforge_plot::PlotType`], les deux
//! [`stokesforge_plot::AxisScale`], l'axe secondaire, les styles de trait et de
//! marqueur, la légende, la grille de panneaux, et la décimation LTTB. Ce
//! qui manque encore est signalé par un `// R6 :` ou `// R7 :` à l'endroit
//! exact où il s'insérera, pas par une liste en tête de fichier qui se
//! périmerait.
//!
//! # Comment un panneau se trace, dans l'ordre
//!
//! L'ordre n'est pas libre, et chaque étape dépend de ce que la précédente
//! a appris :
//!
//! 1. **préparer** chaque série en géométrie ([`crate::series`]), ce qui
//!    applique l'échelle de son axe et classe les histogrammes. Le cadrage
//!    et le tracé liront la même géométrie, donc ne pourront pas la lire
//!    différemment ;
//! 2. **cadrer** les axes sur cette géométrie ;
//! 3. **construire** la zone de tracé — c'est seulement là que la largeur en
//!    pixels existe ;
//! 4. **décimer** ([`crate::decimate`]), qui a besoin de cette largeur ;
//! 5. **peindre**, en pixels ([`crate::paint`]) ;
//! 6. **la légende**, par-dessus, et mesurée par le même chemin que les
//!    axes ([`crate::legend`]) ;
//! 7. **les textes libres** (issue #35), tout au-dessus : centrés sur leur
//!    point de données, projeté comme une série ; un point hors du cadrage
//!    n'est pas dessiné, un texte peut déborder sur les graduations.
//!
//! # Où la mesure de texte entre dans la géométrie
//!
//! Ce n'est pas anecdotique, et c'est ce qui rend le test d'accord (R4.5,
//! étendu en R5.6) autre chose qu'une formalité : `DrawingArea::titled` —
//! appelé dès que le graphe ou un panneau a un titre — demande la hauteur du
//! titre à `estimate_text_size`, puis fait **descendre le haut de la zone de
//! tracé** de cette hauteur. Une mesure fausse de quelques pixels déplace
//! donc la zone entière, axes compris, sans qu'aucune erreur ne soit levée.
//! C'est exactement le défaut silencieux décrit en
//! `architecture-choisie.md` §4.4, et le test d'accord le prend par là. La
//! légende passe par le même chemin, pour la même raison
//! ([`crate::legend`]).
//!
//! # Deux styles de texte, selon le rôle
//!
//! Les graduations prennent `text_style` (mise en page `epaint`) ; titre du
//! graphe, titres de panneau, titres d'axe et légende prennent
//! [`rich_style`], que les backends compilent par Typst (issue #32, voir
//! [`crate::formula`]). C'est ici, et seulement ici, que le rôle d'un texte
//! est décidé.

use plotters::coord::Shift;
use plotters::prelude::*;
use stokesforge_core::resolve::{ResolvedPanel, ResolvedPlot};
use stokesforge_plot::{
    AxisScale, AxisSide as PlotAxisSide, AxisSpec, PlotSpec, SeriesAxis, TextAnchor, TextTarget,
};

use crate::clip::{self, PixelBounds};
use crate::color::{auto_ink, over_white, parse_hex, rgba_or};
use crate::decimate;
use crate::formula::{literal_style_font, rich_style_font};
use crate::legend;
use crate::paint::{self, opaque, rgba, PAPER};
use crate::scale;
use crate::series::{self, Geometry, Prepared};

/// Taille (au sens de Plotters) du titre du graphe.
pub const TITLE_SIZE: f64 = 24.0;
/// Taille (au sens de Plotters) du titre d'un panneau.
///
/// Plus petite que celle du graphe : dans une grille, le titre du graphe
/// doit rester celui qu'on lit en premier.
pub const PANEL_TITLE_SIZE: f64 = 16.0;
/// Taille (au sens de Plotters) des chiffres de graduation.
pub const LABEL_SIZE: f64 = 14.0;
/// Taille (au sens de Plotters) des titres d'axe.
pub const AXIS_DESC_SIZE: f64 = 16.0;
/// Marge extérieure du graphe, en unités de backend.
pub const MARGIN: u32 = 12;
/// Hauteur réservée aux graduations et au titre de l'axe du bas.
pub const X_LABEL_AREA: u32 = 52;
/// Largeur réservée aux graduations et au titre d'un axe vertical.
pub const Y_LABEL_AREA: u32 = 64;
/// Aire de graduation d'un axe miroir sans libellés (#176) : les tirets seuls.
pub const BARE_LABEL_AREA: u32 = 12;
/// Famille de police demandée à Plotters. Elle est traduite en
/// [`epaint::FontFamily::Proportional`] par [`crate::text::font_id`] ; ce que
/// Plotters en fait par lui-même n'a aucune importance, ses fonts étant
/// désactivées.
pub const FONT_FAMILY: &str = "sans-serif";

/// Nombre de cellules au-delà duquel la grille déclarée est abandonnée au
/// profit d'une grille compacte.
///
/// `PlotSpec::rows()` et `columns()` valent `max(position) + 1` : une
/// position de panneau venue d'un fichier peut valoir n'importe quoi (rien
/// ne la borne à la lecture, et le spec le documente en `saturating_add`),
/// donc une grille déclarée peut valoir des milliards de cellules pour deux
/// panneaux. Découper la zone en autant est impossible ; le repli est décrit
/// dans [`grid_of`].
pub const MAX_CELLS: usize = 256;

/// Dimensions en deçà desquelles une cellule n'est plus une zone de tracé.
///
/// Un panneau plus petit que cela n'a la place ni de ses graduations ni de
/// son cadre ; il est compté comme omis plutôt que dessiné en bouillie.
pub const MIN_CELL: (u32, u32) = (48, 36);

/// Taille par défaut d'une figure StokesForge, en pixels — `(largeur, hauteur)`.
///
/// Choix délibéré, pas encore un réglage. 4/3, comme visé — mais provisoire
/// (valeurs à revoir). Toute forme de configurabilité future (par graphe, par
/// export) passe par ce point unique plutôt que par une constante recopiée
/// dans chaque backend — `stokesforge-render-svg` (`DEFAULT_SIZE`, export) l'utilise
/// comme taille fixe de page ; `stokesforge-app` (vue écran, F11) ne s'en sert que
/// pour son ratio 4/3, la figure étant mise à l'échelle « contain » dans
/// l'espace disponible de la fenêtre (`graph_view_contents`).
pub const DEFAULT_FIGURE_SIZE: (u32, u32) = (480, 360);

/// Ce qu'un panneau a fini par tracer — de quoi écrire un test sans relire
/// des pixels.
#[derive(Debug, Clone, PartialEq)]
pub struct DrawnPanel {
    /// Indice du calque dans [`PlotSpec::panels`] : l'identité d'un calque
    /// (plusieurs calques peuvent se superposer dans la même case).
    pub index: usize,
    /// Le calque est superposé à un autre (#167) : son axe X est en haut si
    /// `x_top`, son axe Y à droite si `y_right`.
    pub overlay: Option<stokesforge_plot::OverlaySpec>,
    /// Position déclarée du panneau dans la grille.
    pub row: usize,
    /// Idem, en colonne.
    pub column: usize,
    /// Bornes retenues en X, **dans l'espace des données** : sur une échelle
    /// logarithmique, ce sont bien des valeurs et non leurs logarithmes.
    pub x_range: (f64, f64),
    /// Idem pour l'ordonnée de gauche.
    pub y_range: (f64, f64),
    /// Bornes de l'ordonnée de droite, si une série s'y rattache.
    pub y2_range: Option<(f64, f64)>,
    /// Coin haut-gauche de la zone de tracé, en pixels de backend.
    ///
    /// Ajouté en R7 (tâche 7.2/7.3) : c'est ce qui, combiné à
    /// [`Self::pixel_size`] et aux bornes ci-dessus, donne la correspondance
    /// pixel ↔ données qu'`interact::PanelTransform` construit — sans
    /// redécouvrir le calcul de grille ni les aires de graduation, puisque
    /// ce sont les pixels **réellement** alloués par Plotters
    /// (`plotting.get_base_pixel()`) qui sont recopiés ici, pas une
    /// approximation reconstruite à côté.
    pub pixel_origin: (i32, i32),
    /// Dimensions de la zone de tracé, en pixels de backend.
    pub pixel_size: (u32, u32),
    /// Échelle de l'axe des abscisses, telle qu'elle a été appliquée.
    pub x_scale: AxisScale,
    /// Échelle de l'ordonnée de gauche.
    pub y_scale: AxisScale,
    /// Échelle de l'ordonnée de droite (sans effet si [`Self::y2_range`]
    /// est `None`).
    pub y2_scale: AxisScale,
    /// Nombre de séries réellement tracées.
    pub drawn_series: usize,
    /// Nombre de séries laissées de côté parce que leur référence est
    /// brisée. Jamais une erreur : `architecture-choisie.md` §11.1 en fait un
    /// état de première classe, à afficher et non à taire — elles
    /// apparaissent en légende, en gris et annotées.
    pub broken_series: usize,
    /// Nombre de séries dont le type de tracé vient d'une version plus
    /// récente de StokesForge (C5, niveau 2) : rien n'est dessiné, rien n'est
    /// perdu.
    pub unknown_series: usize,
    /// Nombre d'entrées inscrites en légende.
    pub legend_entries: usize,
    /// Nombre de points retirés par la décimation LTTB.
    pub decimated_points: usize,
    /// Aires de graduation réellement allouées autour de la zone de tracé —
    /// de quoi retrouver, sans relire des pixels, quel axe (ou quel titre
    /// d'axe) est sous un clic ([`crate::hit`]).
    pub axis_areas: AxisAreas,
    /// Les textes libres réellement tracés (issue #35), dans l'ordre du
    /// spec ; un texte hors du cadrage visible n'y figure pas.
    pub annotations: Vec<DrawnAnnotation>,
    /// Les objets graphiques réellement tracés (issue #76), dans l'ordre du
    /// spec (du dessous au dessus). Contrairement à un texte libre, un objet
    /// qui déborde du cadrage visible y figure : il est tracé rogné, mais
    /// désignable par la partie visible.
    pub shapes: Vec<crate::shapes::DrawnShape>,
    /// Géométrie visible des séries, après décimation et rognage (#83).
    pub series_geometry: Vec<crate::series_selection::DrawnSeries>,
    /// Les titres et la légende de ce panneau (issue #53) : titre du
    /// panneau, titres d'axe, cadre de la légende. Un texte à sa place par
    /// défaut y figure aussi, avec une position **approchée** (Plotters le
    /// pose lui-même) ; c'est assez pour le désigner et pour partir de là
    /// quand on le déplace.
    pub texts: Vec<DrawnText>,
    /// Les lignes de la légende, une par entrée dessinée.
    pub legend_rows: Vec<DrawnLegendRow>,
}

/// Un titre ou une légende tel qu'il a été tracé (issue #53).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrawnText {
    /// Le texte désigné.
    pub target: TextTarget,
    /// Centre, en pixels de backend.
    pub center: (f64, f64),
    /// Taille du texte **non tourné**, en pixels de backend.
    pub size: (u32, u32),
    /// Rotation, en degrés, sens antihoraire (les titres d'axe vertical).
    pub rotation: f64,
    /// Vrai si le texte est posé sur son ancre ([`TextAnchor`]), faux s'il
    /// est à sa place par défaut — alors `center` est approché.
    pub anchored: bool,
}

impl DrawnText {
    /// Vrai si le pixel `(x, y)` tombe dans le rectangle tourné du texte.
    pub fn contains(&self, x: f64, y: f64) -> bool {
        rotated_contains(self.center, self.size, self.rotation, x, y)
    }
}

/// Une ligne de la légende : l'entrée qu'elle montre, et où.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrawnLegendRow {
    /// Rang de la série dans `PanelSpec::series`.
    pub series: usize,
    /// Milieu vertical de la ligne, en pixels de backend.
    pub center_y: f64,
    /// Hauteur de la ligne.
    pub height: f64,
}

/// Un texte libre tel qu'il a été tracé — de quoi le désigner sous le
/// pointeur ([`crate::hit`]), rotation comprise.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrawnAnnotation {
    /// Indice dans `PanelSpec::annotations`.
    pub index: usize,
    /// Centre du texte, en pixels de backend.
    pub center: (f64, f64),
    /// Taille du texte **non tourné**, en pixels de backend.
    pub size: (u32, u32),
    /// Rotation, en degrés, sens antihoraire.
    pub rotation: f64,
}

impl DrawnAnnotation {
    /// Vrai si le pixel `(x, y)` tombe dans le rectangle tourné du texte.
    pub fn contains(&self, x: f64, y: f64) -> bool {
        rotated_contains(self.center, self.size, self.rotation, x, y)
    }
}

/// Le pixel `(x, y)` est-il dans le rectangle `size` centré en `center` et
/// tourné de `rotation` degrés (antihoraire) ?
fn rotated_contains(center: (f64, f64), size: (u32, u32), rotation: f64, x: f64, y: f64) -> bool {
    let (dx, dy) = (x - center.0, y - center.1);
    // Le texte est tourné dans le sens antihoraire à l'écran (axe y vers le
    // bas) ; on ramène le point dans le repère du texte droit par la
    // rotation inverse.
    let (sin, cos) = rotation.to_radians().sin_cos();
    let (u, v) = (dx * cos - dy * sin, dx * sin + dy * cos);
    u.abs() <= size.0 as f64 / 2.0 && v.abs() <= size.1 as f64 / 2.0
}

/// Une aire de graduation : son épaisseur en pixels (0 si l'axe est masqué
/// ou absent), et si un titre d'axe y est écrit — Plotters le pose contre le
/// bord **extérieur** de l'aire (en bas pour l'axe du bas, à gauche pour
/// celui de gauche, à droite pour l'axe secondaire).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AxisArea {
    /// Épaisseur, en pixels de backend, perpendiculairement à l'axe.
    pub size: u32,
    /// Vrai si l'axe porte un titre (libellé ou unité non vides).
    pub titled: bool,
}

/// Les aires de graduation d'un panneau.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AxisAreas {
    /// Sous la zone de tracé (abscisses).
    pub bottom: AxisArea,
    /// À gauche (ordonnée principale).
    pub left: AxisArea,
    /// À droite (ordonnée secondaire), si une série s'y rattache, ou l'axe Y
    /// d'un calque superposé (#167).
    pub right: AxisArea,
    /// Au-dessus de la zone de tracé : l'axe X d'un calque superposé (#167).
    pub top: AxisArea,
}

/// Ce qu'un graphe a fini par tracer.
#[derive(Debug, Clone, PartialEq)]
pub struct DrawnPlot {
    /// Les panneaux effectivement dessinés, dans l'ordre du spec.
    pub panels: Vec<DrawnPanel>,
    /// Nombre de panneaux laissés de côté faute de place ([`MIN_CELL`]).
    pub omitted_panels: usize,
    /// Nombre total de séries tracées, tous panneaux confondus.
    pub drawn_series: usize,
    /// Nombre total de références brisées.
    pub broken_series: usize,
    /// Nombre total de séries à type de tracé inconnu.
    pub unknown_series: usize,
    /// Nombre total de points retirés par la décimation.
    pub decimated_points: usize,
    /// Hauteur, en pixels de backend, de la bande du titre du graphe en haut
    /// de la figure (0 sans titre).
    pub title_height: u32,
    /// Le titre du graphe tel qu'il a été tracé (issue #53) ; `None` sans
    /// titre.
    pub title: Option<DrawnText>,
}

impl DrawnPlot {
    /// Le cadrage en X du premier panneau — la commodité du cas courant, qui
    /// n'en a qu'un.
    pub fn x_range(&self) -> (f64, f64) {
        self.panels.first().map_or((0.0, 1.0), |p| p.x_range)
    }

    /// Le cadrage en Y du premier panneau.
    pub fn y_range(&self) -> (f64, f64) {
        self.panels.first().map_or((0.0, 1.0), |p| p.y_range)
    }

    /// Le panneau d'indice donné.
    pub fn panel(&self, index: usize) -> Option<&DrawnPanel> {
        self.panels.get(index)
    }
}

/// Le fond de la figure, blanc s'il n'est pas lisible.
fn figure_background(spec: &PlotSpec) -> (u8, u8, u8, u8) {
    rgba_or(&spec.background, opaque(PAPER))
}

/// Encre automatique de la figure : celle de tout texte et de toute échelle
/// dont la couleur est laissée vide, et des poignées d'une série
/// sélectionnée (#98) — blanche sur un fond sombre, sombre sinon. Un fond
/// (semi-)transparent est jugé composé sur du blanc, comme l'écran le peint.
pub fn figure_ink(spec: &PlotSpec) -> (u8, u8, u8) {
    auto_ink(over_white(figure_background(spec)))
}

/// Trace le graphe résolu sur le backend donné.
///
/// Consomme le backend — c'est la convention de Plotters
/// ([`IntoDrawingArea`]) et elle tombe juste : un backend ne sert qu'à une
/// figure.
pub fn draw<DB: DrawingBackend>(
    backend: DB,
    plot: &ResolvedPlot<'_>,
) -> Result<DrawnPlot, DrawingAreaErrorKind<DB::ErrorType>> {
    let root = backend.into_drawing_area();
    draw_on(&root, plot)
}

/// Trace le graphe résolu sur une zone de dessin déjà construite.
///
/// Séparée de [`draw`] pour que l'appelant qui possède déjà une
/// [`DrawingArea`] n'ait pas à en refaire une.
pub fn draw_on<DB: DrawingBackend>(
    root: &DrawingArea<DB, Shift>,
    plot: &ResolvedPlot<'_>,
) -> Result<DrawnPlot, DrawingAreaErrorKind<DB::ErrorType>> {
    let spec = plot.spec;
    let background = figure_background(spec);
    let ink = figure_ink(spec);
    // Un fond d'opacité nulle n'est pas peint du tout : la figure exportée
    // (SVG, PNG, PDF) n'a pas de fond, pour s'intégrer à une autre
    // présentation.
    if background.3 > 0 {
        root.fill(&rgba(background))?;
    }

    // `titled` mesure le titre et rabaisse d'autant le haut de la zone : le
    // point où `estimate_text_size` entre dans la géométrie (voir la doc de
    // tête).
    //
    // Un titre déplacé (issue #53) garde sa bande, **vide** : la mise en page
    // ne dépend pas de l'endroit où le titre est posé — sinon la zone de
    // tracé changerait de taille au lâcher, et le titre, ancré aux données,
    // sauterait. Il est tracé sur son ancre une fois les panneaux faits,
    // par-dessus.
    let title_color = rgba_or(&spec.title_color, opaque(ink));
    let title_style = font_style(&spec.title_font, TITLE_SIZE, title_color);
    let title_anchor =
        usable_anchor(spec.title_anchor).filter(|_| spec.title_visible && !spec.title.is_empty());
    let area = if !spec.title_visible || spec.title.is_empty() {
        root.clone()
    } else if title_anchor.is_some() {
        reserve_title_band(root, &spec.title, &title_style)?
    } else {
        root.titled(&spec.title, title_style.clone())?
    };
    let title_height = (area.get_base_pixel().1 - root.get_base_pixel().1).max(0) as u32;
    let (root_width, _) = root.dim_in_pixel();
    let mut drawn_title = if !spec.title_visible || spec.title.is_empty() || title_anchor.is_some()
    {
        None
    } else {
        let size = root.estimate_text_size(&spec.title, &title_style)?;
        Some(DrawnText {
            target: TextTarget::Title,
            center: (root_width as f64 / 2.0, title_height as f64 / 2.0),
            size,
            rotation: 0.0,
            anchored: false,
        })
    };

    let mut drawn = DrawnPlot {
        panels: Vec::with_capacity(plot.panels.len()),
        omitted_panels: 0,
        drawn_series: 0,
        broken_series: 0,
        unknown_series: 0,
        decimated_points: 0,
        title_height,
        title: None,
    };
    let mut first_projection: Option<Projection> = None;

    // Toute la figure, en coordonnées d'écran : les textes déplacés (titres,
    // légende, textes libres) s'y tracent sans être ramenés dans la cellule
    // ou la zone de tracé (`BackendCoordOnly` rogne chaque point sur le
    // rectangle de sa zone, #163).
    let canvas = root.use_screen_coord();
    let layout = grid_of(spec, plot.panels.len());
    let cells = area.split_evenly((layout.rows, layout.columns));

    // Les aires de graduation de chaque case : l'union de ce que demandent
    // ses calques, pour que les calques superposés (#167) partagent une zone
    // de tracé identique. `bases` garde, par case, le dernier calque ordinaire
    // tracé : celui qu'un calque superposé recouvre.
    let mut plans: std::collections::HashMap<usize, CellPlan> = std::collections::HashMap::new();
    for (index, panel) in plot.panels.iter().enumerate() {
        if let Some(cell) = layout.cells.get(index) {
            plans.entry(*cell).or_default().add(panel);
        }
    }
    let mut bases = std::collections::HashMap::new();
    // La légende d'un calque ordinaire liste aussi les courbes des calques
    // superposés qui le recouvrent (#172) : `overlays_of` donne, par indice de
    // calque ordinaire, les calques superposés qui le suivent dans sa case.
    let mut overlays_of: std::collections::HashMap<usize, Vec<usize>> =
        std::collections::HashMap::new();
    let mut last_base: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    for (index, panel) in plot.panels.iter().enumerate() {
        let Some(cell) = layout.cells.get(index).copied() else {
            continue;
        };
        if panel.spec.overlay.is_none() {
            last_base.insert(cell, index);
        } else if let Some(base) = last_base.get(&cell) {
            overlays_of.entry(*base).or_default().push(index);
        }
    }

    for (index, panel) in plot.panels.iter().enumerate() {
        let Some(cell_index) = layout.cells.get(index).copied() else {
            drawn.omitted_panels += 1;
            continue;
        };
        let Some(cell) = cells.get(cell_index) else {
            drawn.omitted_panels += 1;
            continue;
        };
        let (w, h) = cell.dim_in_pixel();
        if w < MIN_CELL.0 || h < MIN_CELL.1 {
            drawn.omitted_panels += 1;
            continue;
        }
        let plan = plans.get(&cell_index).copied().unwrap_or_default();
        let overlay_panels: Vec<&ResolvedPanel<'_>> = overlays_of
            .get(&index)
            .into_iter()
            .flatten()
            .filter_map(|&i| plot.panels.get(i))
            .collect();
        let (panel_drawn, projection, frame) = draw_panel(
            cell,
            &canvas,
            panel,
            index,
            background,
            plan,
            bases.get(&cell_index),
            &overlay_panels,
            plot.spec.decimal_comma,
        )?;
        if panel.spec.overlay.is_none() {
            bases.insert(cell_index, frame);
        }
        if first_projection.is_none() {
            first_projection = Some(projection);
        }
        drawn.drawn_series += panel_drawn.drawn_series;
        drawn.broken_series += panel_drawn.broken_series;
        drawn.unknown_series += panel_drawn.unknown_series;
        drawn.decimated_points += panel_drawn.decimated_points;
        drawn.panels.push(panel_drawn);
    }

    if let Some(anchor) = title_anchor {
        // Le titre du graphe n'appartient à aucun panneau : son ancre est
        // lue dans le repère du premier panneau tracé. Sans projection
        // possible (panneau omis, axe logarithmique ≤ 0), il retombe en haut
        // et au centre de la figure plutôt que de disparaître.
        let pixel = first_projection
            .as_ref()
            .and_then(|projection| projection.pixel(anchor))
            .unwrap_or(((root_width / 2) as i32, (MARGIN + 12) as i32));
        let size = draw_centered_text(root, &spec.title, &title_style, 0.0, pixel)?;
        drawn_title = Some(DrawnText {
            target: TextTarget::Title,
            center: (pixel.0 as f64, pixel.1 as f64),
            size,
            rotation: 0.0,
            anchored: true,
        });
    }
    drawn.title = drawn_title;

    root.present()?;
    Ok(drawn)
}

/// La zone sous la bande qu'un titre occuperait, **sans** écrire le titre :
/// la même hauteur que `DrawingArea::titled` de Plotters (hauteur du texte
/// plus deux marges de 5 px au plus), mesurée par le même chemin.
fn reserve_title_band<DB: DrawingBackend>(
    area: &DrawingArea<DB, Shift>,
    text: &str,
    style: &TextStyle<'_>,
) -> Result<DrawingArea<DB, Shift>, DrawingAreaErrorKind<DB::ErrorType>> {
    let (_, text_height) = area.estimate_text_size(text, style)?;
    let band = text_height + (text_height / 2).min(5) * 2;
    let (width, height) = area.dim_in_pixel();
    Ok(area
        .clone()
        .shrink((0, band as i32), (width, height.saturating_sub(band))))
}

/// L'ancre, si elle est exploitable ([`TextAnchor::is_finite`]) : une ancre
/// abîmée laisse le texte à sa place par défaut.
fn usable_anchor(anchor: Option<TextAnchor>) -> Option<TextAnchor> {
    anchor.filter(TextAnchor::is_finite)
}

/// La zone de tracé d'un panneau et ses cadrages : de quoi poser une
/// [`TextAnchor`] en pixels — la même correspondance que
/// `crate::interact::PanelTransform`, qui sert au glisser.
struct Projection {
    origin: (i32, i32),
    size: (u32, u32),
    ranges: Ranges,
    right_active: bool,
}

impl Projection {
    /// Le pixel de l'ancre — **sans** exiger qu'elle tombe dans le cadrage :
    /// un titre vit hors de la zone de tracé. `None` si l'échelle n'a pas
    /// d'image de cette valeur (≤ 0 sur un axe logarithmique) ou si le
    /// cadrage est dégénéré.
    fn pixel(&self, anchor: TextAnchor) -> Option<(i32, i32)> {
        let right = anchor.axis == SeriesAxis::Right && self.right_active;
        let (y_scale, y_range) = if right {
            self.ranges.y2?
        } else {
            self.ranges.y
        };
        let fraction = |scale: AxisScale, (low, high): (f64, f64), value: f64| {
            let t = scale::forward(scale, value)?;
            (high != low).then(|| (t - low) / (high - low))
        };
        let fx = fraction(self.ranges.x.0, self.ranges.x.1, anchor.x)?;
        let fy = fraction(y_scale, y_range, anchor.y)?;
        Some((
            self.origin.0 + (fx * self.size.0 as f64).round() as i32,
            self.origin.1 + ((1.0 - fy) * self.size.1 as f64).round() as i32,
        ))
    }
}

/// Trace `text` (Typst) centré sur `pixel`, tourné de `rotation` degrés
/// (antihoraire), et rend la taille du texte **droit**, mesurée par le même
/// chemin que tout le reste.
fn draw_centered_text<DB: DrawingBackend>(
    area: &DrawingArea<DB, Shift>,
    text: &str,
    style: &TextStyle<'static>,
    rotation: f64,
    pixel: plotters_backend::BackendCoord,
) -> Result<(u32, u32), DrawingAreaErrorKind<DB::ErrorType>> {
    use plotters::style::text_anchor::{HPos, Pos, VPos};
    let centered = Pos::new(HPos::Center, VPos::Center);
    let straight = style.pos(centered);
    // Le style d'un texte riche saisi porte la police de sa zone dans sa
    // famille (`formula::rich_style_font`) : le style tourné la reprend.
    let font = match style.font.get_family() {
        FontFamily::Name(name) => name.split_once('|').map(|(_, font)| font),
        _ => None,
    };
    let family = crate::formula::rotated_family_font(rotation, font);
    let turned = TextStyle {
        font: (family.as_str(), style.font.get_size()).into_font(),
        color: style.color,
        pos: centered,
    };
    let measured = area.estimate_text_size(text, &straight)?;
    area.draw(&Text::new(text, pixel, &turned))?;
    Ok(measured)
}

/// La grille de panneaux : combien de lignes, combien de colonnes, et dans
/// quelle cellule va chaque panneau.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    /// Lignes de la grille.
    pub rows: usize,
    /// Colonnes de la grille.
    pub columns: usize,
    /// Indice de cellule de chaque panneau, dans l'ordre du spec.
    pub cells: Vec<usize>,
}

/// Répartit les panneaux dans une grille — tâche R5.4.
///
/// Le cas nominal est celui que la roadmap décrit : `spec.rows()` ×
/// `spec.columns()`, chaque panneau dans la cellule que son `row`/`column`
/// désigne. Deux panneaux à la même position se superposent, le dernier
/// gagne — ce n'est pas un état que le rendu a à arbitrer.
///
/// **Le repli existe parce que les positions ne sont pas bornées.** `row` et
/// `column` sont des `usize` libres que rien ne valide à la lecture d'un
/// fichier (`stokesforge-plot` le documente, et `PlotSpec::rows` s'en protège
/// déjà par un `saturating_add`). Deux panneaux en `0` et en `usize::MAX`
/// déclarent une grille de `usize::MAX` lignes : indécoupable. Au-delà de
/// [`MAX_CELLS`], les panneaux sont donc rangés **dans l'ordre de
/// déclaration** dans la grille la plus carrée qui les contienne. La figure
/// n'est alors pas celle que le spec décrit, mais elle est lisible et
/// déterministe, et les deux backends la produisent à l'identique — ce qui
/// est la seule propriété que ce module a le droit de promettre.
pub fn grid_of(spec: &PlotSpec, panels: usize) -> Layout {
    let rows = spec.rows();
    let columns = spec.columns();
    let declared = rows
        .checked_mul(columns)
        .filter(|cells| *cells <= MAX_CELLS);
    if declared.is_some() {
        return Layout {
            rows,
            columns,
            cells: spec
                .panels
                .iter()
                .map(|panel| panel.row * columns + panel.column)
                .collect(),
        };
    }

    let count = panels.max(1);
    let columns = (count as f64).sqrt().ceil() as usize;
    let columns = columns.max(1);
    Layout {
        rows: count.div_ceil(columns),
        columns,
        cells: (0..panels).collect(),
    }
}

/// Les aires de graduation réservées dans une case de la grille : l'union de
/// ce que demandent tous ses calques, pour que les calques superposés (#167)
/// aient exactement la même zone de tracé.
#[derive(Debug, Clone, Copy, Default)]
struct CellPlan {
    bottom: bool,
    top: bool,
    left: bool,
    right: bool,
    /// Un calque superposé de la case a son propre axe en haut / à droite :
    /// il prime sur un axe miroir (#176).
    top_overlay: bool,
    right_overlay: bool,
    /// Un calque ordinaire de la case demande un axe miroir en haut / à
    /// droite, et si l'un d'eux garde ses libellés.
    top_mirror: bool,
    right_mirror: bool,
    top_labels: bool,
    right_labels: bool,
    /// Une série est rattachée à l'axe de droite : c'est un vrai axe.
    right_data: bool,
}

impl CellPlan {
    fn add(&mut self, panel: &ResolvedPanel<'_>) {
        let spec = panel.spec;
        match spec.overlay {
            Some(overlay) => {
                self.top_overlay |= overlay.x_top && spec.bottom.visible;
                self.right_overlay |= overlay.y_right && spec.left.visible;
                self.top |= overlay.x_top && spec.bottom.visible;
                self.right |= overlay.y_right && spec.left.visible;
            }
            None => {
                if spec.mirrors_top() {
                    self.top_mirror = true;
                    self.top = true;
                    self.top_labels |= spec.top.tick_labels;
                }
                if spec.mirrors_right() {
                    self.right_mirror = true;
                    self.right = true;
                    self.right_labels |= spec.right.tick_labels;
                }
                self.bottom |= spec.bottom.visible;
                self.left |= spec.left.visible;
                let data = spec.right.visible
                    && panel
                        .series
                        .iter()
                        .any(|s| s.spec.visible && s.spec.axis == SeriesAxis::Right);
                self.right |= data;
                self.right_data |= data;
            }
        }
    }
}

impl CellPlan {
    /// Le miroir du haut est tracé : demandé, et pas d'axe propre d'un calque
    /// superposé à cet endroit.
    fn mirror_top(&self) -> bool {
        self.top_mirror && !self.top_overlay
    }

    /// Le miroir de droite est tracé, même règle.
    fn mirror_right(&self) -> bool {
        self.right_mirror && !self.right_overlay
    }

    /// La taille voulue pour l'aire du haut : un miroir sans libellés n'a
    /// besoin que de ses tirets.
    fn top_wanted(&self) -> u32 {
        if self.top_overlay || self.top_labels {
            X_LABEL_AREA
        } else {
            BARE_LABEL_AREA
        }
    }

    /// Même chose pour l'aire de droite (un vrai axe de droite y garde la
    /// sienne : `right` est alors déjà vrai sans miroir).
    fn right_wanted(&self) -> u32 {
        if self.right_data || self.right_overlay || self.right_labels {
            Y_LABEL_AREA
        } else {
            BARE_LABEL_AREA
        }
    }
}

/// Ce qu'un calque superposé (#167) hérite du calque ordinaire qu'il recouvre.
struct BaseFrame<DB: DrawingBackend> {
    area: DrawingArea<DB, Shift>,
    x_range: (f64, f64),
    y_range: (f64, f64),
    x_scale: AxisScale,
    y_scale: AxisScale,
}

/// Trace un panneau dans sa cellule.
#[allow(clippy::too_many_arguments)]
fn draw_panel<DB: DrawingBackend>(
    cell: &DrawingArea<DB, Shift>,
    canvas: &DrawingArea<DB, Shift>,
    panel: &ResolvedPanel<'_>,
    index: usize,
    background: (u8, u8, u8, u8),
    plan: CellPlan,
    base: Option<&BaseFrame<DB>>,
    legend_overlays: &[&ResolvedPanel<'_>],
    decimal_comma: bool,
) -> Result<(DrawnPanel, Projection, BaseFrame<DB>), DrawingAreaErrorKind<DB::ErrorType>> {
    let spec = panel.spec;
    // Un calque superposé sans calque à recouvrir se trace comme un calque
    // ordinaire.
    let overlay = spec.overlay.filter(|_| base.is_some());
    let base = base.filter(|_| overlay.is_some());
    let x_own = overlay.is_none_or(|o| o.x_top);
    let y_own = overlay.is_none_or(|o| o.y_right);
    // Encre automatique du panneau, voir `draw_on`.
    let ink = auto_ink(over_white(background));
    let x_scale = match base {
        Some(base) if !x_own => base.x_scale,
        _ => spec.bottom.scale,
    };
    let y_scale = match base {
        Some(base) if !y_own => base.y_scale,
        _ => spec.left.scale,
    };
    let y2_scale = spec.right.scale;

    // 1 — préparation. Une série invisible ne figure nulle part : ni tracée,
    // ni cadrée, ni en légende.
    // `source` garde le rang de chaque série dans `PanelSpec::series` : la
    // légende désigne ses lignes par ce rang (issue #53).
    let (source, mut prepared): (Vec<usize>, Vec<Prepared<'_>>) = panel
        .series
        .iter()
        .enumerate()
        .filter(|(_, series)| series.spec.visible)
        .map(|(rank, series)| {
            let scale = if series.spec.axis == SeriesAxis::Right {
                y2_scale
            } else {
                y_scale
            };
            (rank, series::prepare(series, x_scale, scale))
        })
        .unzip();
    // Un calque superposé n'a pas d'ordonnée secondaire : tout se trace sur
    // son axe Y.
    if overlay.is_some() {
        for series in &mut prepared {
            series.axis = SeriesAxis::Left;
        }
    }

    // Axes miroirs (#176), sauf là où un calque superposé a son propre axe.
    let mirror_top = overlay.is_none() && plan.mirror_top() && spec.mirrors_top();
    let mirror_right = overlay.is_none() && plan.mirror_right() && spec.mirrors_right();
    let right_active = overlay.is_none()
        && spec.right.visible
        && prepared
            .iter()
            .any(|series| series.axis == SeriesAxis::Right);

    // 2 — cadrage, dans l'espace transformé.
    let x_range = match base {
        Some(base) if !x_own => base.x_range,
        _ => {
            let own = axis_range(
                &spec.bottom,
                x_scale,
                extent_of(&prepared, series::x_extent, None),
                false,
            );
            join_to_base(
                &spec.bottom,
                x_scale,
                own,
                base.map(|b| (b.x_scale, b.x_range)),
            )
        }
    };
    let y_range = match base {
        Some(base) if !y_own => base.y_range,
        _ => {
            let own = axis_range(
                &spec.left,
                y_scale,
                extent_of(&prepared, series::y_extent, Some(SeriesAxis::Left)),
                needs_baseline(&prepared, SeriesAxis::Left),
            );
            join_to_base(
                &spec.left,
                y_scale,
                own,
                base.map(|b| (b.y_scale, b.y_range)),
            )
        }
    };
    let y2_range = right_active.then(|| {
        axis_range(
            &spec.right,
            y2_scale,
            extent_of(&prepared, series::y_extent, Some(SeriesAxis::Right)),
            needs_baseline(&prepared, SeriesAxis::Right),
        )
    });

    // 3 — la zone de tracé. Le titre du panneau la rabaisse, comme celui du
    // graphe rabaisse la figure entière.
    // Un titre déplacé (issue #53) garde sa bande vide, voir `draw_on`.
    let panel_title_anchor =
        usable_anchor(spec.title_anchor).filter(|_| spec.title_visible && !spec.title.is_empty());
    let title_style = font_style(
        &spec.title_font,
        PANEL_TITLE_SIZE,
        rgba_or(&spec.title_color, opaque(ink)),
    );
    let area = if let Some(base) = base {
        // Dimension liée : la zone de tracé du calque recouvert, titre
        // compris ; un calque superposé n'a pas de titre à lui.
        base.area.clone()
    } else if !spec.title_visible || spec.title.is_empty() {
        cell.clone()
    } else if panel_title_anchor.is_some() {
        reserve_title_band(cell, &spec.title, &title_style)?
    } else {
        cell.titled(&spec.title, title_style.clone())?
    };
    let mut texts: Vec<DrawnText> = Vec::new();
    if base.is_none()
        && spec.title_visible
        && !spec.title.is_empty()
        && panel_title_anchor.is_none()
    {
        let band = (area.get_base_pixel().1 - cell.get_base_pixel().1).max(0);
        let cell_origin = cell.get_base_pixel();
        let (cell_w, _) = cell.dim_in_pixel();
        texts.push(DrawnText {
            target: TextTarget::PanelTitle { panel: index },
            center: (
                cell_origin.0 as f64 + cell_w as f64 / 2.0,
                cell_origin.1 as f64 + band as f64 / 2.0,
            ),
            size: cell.estimate_text_size(&spec.title, &title_style)?,
            rotation: 0.0,
            anchored: false,
        });
    }
    let (width, height) = area.dim_in_pixel();
    // Les aires de graduation sont plafonnées à un tiers de la cellule :
    // dans une grille, 64 px de libellés sur un panneau de 150 px ne
    // laisseraient plus rien à tracer.
    let x_label_area = label_area(plan.bottom, X_LABEL_AREA, height);
    let top_label_area = label_area(plan.top, plan.top_wanted(), height);
    let y_label_area = label_area(plan.left, Y_LABEL_AREA, width);
    let right_label_area = label_area(plan.right, plan.right_wanted(), width);

    let mut builder = ChartBuilder::on(&area);
    builder
        .margin(MARGIN)
        .x_label_area_size(x_label_area)
        .top_x_label_area_size(top_label_area)
        .y_label_area_size(y_label_area)
        .right_y_label_area_size(right_label_area);
    let chart = builder.build_cartesian_2d(x_range.0..x_range.1, y_range.0..y_range.1)?;

    // L'axe secondaire est **toujours** attaché, même inutilisé : il ne
    // prend à l'axe principal que les aires du haut et de la droite, qui
    // n'existent pas quand on ne les a pas demandées. Une seule forme de
    // `chart` pour les deux cas, donc un seul chemin de tracé — le prix
    // d'une branche ici serait deux fois le même code de série.
    let secondary = y2_range.unwrap_or(y_range);
    let mut chart = chart.set_secondary_coord(x_range.0..x_range.1, secondary.0..secondary.1);
    // Les aires de graduation de ce calque seul : un axe n'a d'aire que là
    // où il est affiché (le plan de la case en réserve aussi pour les autres
    // calques).
    let (x_area, top_area, y_area, right_area) = match overlay {
        Some(o) => (
            0,
            if o.x_top && spec.bottom.visible {
                top_label_area
            } else {
                0
            },
            0,
            if o.y_right && spec.left.visible {
                right_label_area
            } else {
                0
            },
        ),
        None => (
            if spec.bottom.visible { x_label_area } else { 0 },
            if mirror_top { top_label_area } else { 0 },
            if spec.left.visible { y_label_area } else { 0 },
            if right_active || mirror_right {
                right_label_area
            } else {
                0
            },
        ),
    };

    // 4 — les axes. Chaque axe a ses couleurs (titre, échelle), et le
    // `configure_mesh` de Plotters n'a qu'un style de titre et qu'un style
    // de ligne d'axe pour les deux : l'axe du bas et celui de gauche sont
    // donc tracés en deux passes, chacune coupant l'autre axe. La géométrie
    // n'en dépend pas — les aires de graduation sont fixées par le
    // `ChartBuilder` plus haut.
    // Un titre d'axe déplacé n'est plus écrit par Plotters (l'aire de
    // graduation reste réservée) : il est posé sur son ancre plus bas.
    let axis_anchor = |axis: &AxisSpec| {
        usable_anchor(axis.title_anchor)
            .filter(|_| axis.title_visible && !axis.display_label().is_empty())
    };
    let bottom_anchor = axis_anchor(&spec.bottom);
    let left_anchor = axis_anchor(&spec.left);
    let right_anchor = axis_anchor(&spec.right);
    let plotters_desc = |axis: &AxisSpec, anchor: Option<TextAnchor>| {
        if !axis.title_visible || anchor.is_some() {
            String::new()
        } else {
            axis.display_label()
        }
    };
    let tick_x = scale::TickFormat::new(x_scale, x_range).with_decimal_comma(decimal_comma);
    let tick_y = scale::TickFormat::new(y_scale, y_range).with_decimal_comma(decimal_comma);
    let format_x = |value: &f64| tick_x.format(*value);
    let format_y = |value: &f64| tick_y.format(*value);
    // Sans libellés (#176) : les tirets restent, les nombres disparaissent.
    let blank = |_: &f64| String::new();
    let shown_x = |shown: bool| -> &dyn Fn(&f64) -> String {
        if shown {
            &format_x
        } else {
            &blank
        }
    };
    let shown_y = |shown: bool| -> &dyn Fn(&f64) -> String {
        if shown {
            &format_y
        } else {
            &blank
        }
    };
    // Un calque superposé n'a ni axe en bas ni axe à gauche : ses axes
    // s'affichent en haut et à droite, par les axes secondaires de Plotters
    // (même repère que le calque lui-même, voir plus bas).
    if overlay.is_none() {
        let colors = AxisColors::of(&spec.bottom, ink);
        let mut mesh = chart.configure_mesh();
        colors.apply(&mut mesh);
        mesh.x_desc(plotters_desc(&spec.bottom, bottom_anchor))
            .disable_y_mesh()
            .disable_y_axis();
        if !spec.bottom.grid {
            mesh.disable_x_mesh();
        }
        if !spec.bottom.visible {
            mesh.disable_x_axis();
        }
        mesh.x_label_formatter(shown_x(spec.bottom.tick_labels))
            .x_labels(scale::label_count(x_scale, x_range));
        mesh.draw()?;
    }
    if overlay.is_none() {
        let colors = AxisColors::of(&spec.left, ink);
        let mut mesh = chart.configure_mesh();
        colors.apply(&mut mesh);
        mesh.y_desc(plotters_desc(&spec.left, left_anchor))
            .disable_x_mesh()
            .disable_x_axis();
        if !spec.left.grid {
            mesh.disable_y_mesh();
        }
        if !spec.left.visible {
            mesh.disable_y_axis();
        }
        mesh.y_label_formatter(shown_y(spec.left.tick_labels))
            .y_labels(scale::label_count(y_scale, y_range));
        mesh.draw()?;
    }

    if let Some(range) = y2_range {
        let tick_y2 = scale::TickFormat::new(y2_scale, range).with_decimal_comma(decimal_comma);
        let format_y2 = |value: &f64| {
            if spec.right.tick_labels {
                tick_y2.format(*value)
            } else {
                String::new()
            }
        };
        let colors = AxisColors::of(&spec.right, ink);
        let mut mesh = chart.configure_secondary_axes();
        mesh.label_style(text_style(LABEL_SIZE, colors.ticks))
            .axis_desc_style(colors.title_style.clone())
            .axis_style(rgba(colors.ticks))
            .y_desc(plotters_desc(&spec.right, right_anchor));
        mesh.y_label_formatter(&format_y2)
            .y_labels(scale::label_count(y2_scale, range));
        if mirror_top {
            mesh.x_label_formatter(shown_x(spec.top.tick_labels))
                .x_labels(scale::label_count(x_scale, x_range));
        }
        mesh.draw()?;
    }

    // Les axes miroirs (#176) : mêmes échelle, bornes et graduations que le
    // bas et la gauche, sans titre. Un axe de droite à données est déjà tracé
    // plus haut ; seul le miroir du haut s'ajoute alors.
    if (mirror_top || mirror_right) && y2_range.is_none() {
        let colors = AxisColors::of(&spec.bottom, ink);
        let mut mesh = chart.configure_secondary_axes();
        mesh.label_style(text_style(LABEL_SIZE, colors.ticks))
            .axis_style(rgba(colors.ticks));
        if mirror_top {
            mesh.x_label_formatter(shown_x(spec.top.tick_labels))
                .x_labels(scale::label_count(x_scale, x_range));
        } else {
            mesh.x_labels(0);
        }
        if mirror_right {
            mesh.y_label_formatter(shown_y(spec.right.tick_labels))
                .y_labels(scale::label_count(y_scale, y_range));
        } else {
            mesh.y_labels(0);
        }
        mesh.draw()?;
    }

    if let Some(o) = overlay.filter(|o| {
        (o.x_top && spec.bottom.visible && top_area > 0)
            || (o.y_right && spec.left.visible && right_area > 0)
    }) {
        let show_x = o.x_top && spec.bottom.visible && top_area > 0;
        let show_y = o.y_right && spec.left.visible && right_area > 0;
        // Un seul style de ligne et de libellés pour les deux axes
        // secondaires : celui de l'axe affiché, de l'axe X si les deux le
        // sont.
        let colors = AxisColors::of(if show_x { &spec.bottom } else { &spec.left }, ink);
        let mut mesh = chart.configure_secondary_axes();
        mesh.label_style(text_style(LABEL_SIZE, colors.ticks))
            .axis_desc_style(colors.title_style.clone())
            .axis_style(rgba(colors.ticks))
            .x_desc(if show_x {
                plotters_desc(&spec.bottom, bottom_anchor)
            } else {
                String::new()
            })
            .y_desc(if show_y {
                plotters_desc(&spec.left, left_anchor)
            } else {
                String::new()
            });
        if !show_x {
            mesh.x_labels(0);
        } else {
            mesh.x_label_formatter(&format_x)
                .x_labels(scale::label_count(x_scale, x_range));
        }
        if !show_y {
            mesh.y_labels(0);
        } else {
            mesh.y_label_formatter(&format_y)
                .y_labels(scale::label_count(y_scale, y_range));
        }
        mesh.draw()?;
    }

    // 5 — les séries, en pixels.
    let plotting = chart.plotting_area();
    let origin = plotting.get_base_pixel();
    let size = plotting.dim_in_pixel();
    let screen = plotting.use_screen_coord();
    let threshold = decimate::threshold_for_width(size.0);

    // Plotters trace un axe vertical un pixel hors de la zone (à gauche, ou à
    // droite) et un axe horizontal un pixel hors d'elle (en bas, ou en haut) :
    // les coins communs de deux axes tracés restent vides. On les comble, pour
    // toute la case (le plan réunit les axes de tous ses calques), depuis le
    // calque ordinaire.
    if overlay.is_none() {
        let (left, right) = (origin.0 - 1, origin.0 + size.0 as i32);
        let (top, bottom) = (origin.1 - 1, origin.1 + size.1 as i32);
        let ink = rgba(AxisColors::of(&spec.bottom, ink).ticks);
        for (vertical, horizontal, x, y) in [
            (plan.left, plan.bottom, left, bottom),
            (plan.right, plan.bottom, right, bottom),
            (plan.left, plan.top, left, top),
            (plan.right, plan.top, right, top),
        ] {
            if vertical && horizontal {
                // En L, recouvrant les deux lignes d'axe : un simple point du coin
                // ne les rejoindrait qu'en diagonale.
                let along_x = if x == left { x + 1 } else { x - 1 };
                let along_y = if y == top { y + 1 } else { y - 1 };
                canvas.draw(&PathElement::new(
                    vec![(along_x, y), (x, y), (x, along_y)],
                    ink,
                ))?;
            }
        }
    }

    let mut drawn = DrawnPanel {
        index,
        overlay,
        row: spec.row,
        column: spec.column,
        x_range: data_range(x_scale, x_range),
        y_range: data_range(y_scale, y_range),
        y2_range: y2_range.map(|range| data_range(y2_scale, range)),
        pixel_origin: origin,
        pixel_size: size,
        x_scale,
        y_scale,
        y2_scale,
        drawn_series: 0,
        broken_series: 0,
        unknown_series: 0,
        legend_entries: 0,
        decimated_points: 0,
        axis_areas: AxisAreas {
            bottom: AxisArea {
                size: x_area,
                titled: overlay.is_none()
                    && spec.bottom.title_visible
                    && !spec.bottom.display_label().is_empty()
                    && bottom_anchor.is_none(),
            },
            left: AxisArea {
                size: y_area,
                titled: overlay.is_none()
                    && spec.left.title_visible
                    && !spec.left.display_label().is_empty()
                    && left_anchor.is_none(),
            },
            right: AxisArea {
                size: right_area,
                titled: match overlay {
                    Some(_) => {
                        spec.left.title_visible
                            && !spec.left.display_label().is_empty()
                            && left_anchor.is_none()
                    }
                    None => {
                        spec.right.title_visible
                            && !spec.right.display_label().is_empty()
                            && right_anchor.is_none()
                    }
                },
            },
            top: AxisArea {
                size: top_area,
                titled: spec.bottom.title_visible
                    && !spec.bottom.display_label().is_empty()
                    && bottom_anchor.is_none(),
            },
        },
        annotations: Vec::new(),
        shapes: Vec::new(),
        series_geometry: Vec::new(),
        texts: Vec::new(),
        legend_rows: Vec::new(),
    };

    // Les titres d'axe à leur place par défaut : position approchée (Plotters
    // les pose contre le bord extérieur de l'aire de graduation), de quoi les
    // désigner et partir de là quand on les déplace (issue #53).
    let (plot_x, plot_y) = (origin.0 as f64, origin.1 as f64);
    let (plot_w, plot_h) = (size.0 as f64, size.1 as f64);
    // `side` est l'axe du spec, `place` le bord où il est affiché (un calque
    // superposé met son axe X en haut et son axe Y à droite, #167).
    let placed: [(
        &AxisSpec,
        Option<TextAnchor>,
        PlotAxisSide,
        PlotAxisSide,
        u32,
    ); 3] = match overlay {
        Some(_) => [
            (
                &spec.bottom,
                bottom_anchor,
                PlotAxisSide::Bottom,
                PlotAxisSide::Top,
                top_area,
            ),
            (
                &spec.left,
                left_anchor,
                PlotAxisSide::Left,
                PlotAxisSide::Right,
                right_area,
            ),
            (
                &spec.right,
                None,
                PlotAxisSide::Right,
                PlotAxisSide::Right,
                0,
            ),
        ],
        None => [
            (
                &spec.bottom,
                bottom_anchor,
                PlotAxisSide::Bottom,
                PlotAxisSide::Bottom,
                x_area,
            ),
            (
                &spec.left,
                left_anchor,
                PlotAxisSide::Left,
                PlotAxisSide::Left,
                y_area,
            ),
            (
                &spec.right,
                right_anchor,
                PlotAxisSide::Right,
                PlotAxisSide::Right,
                if right_active { right_label_area } else { 0 },
            ),
        ],
    };
    for (axis, anchor, side, place, area_size) in placed {
        let label = axis.display_label();
        if !axis.title_visible || label.is_empty() || anchor.is_some() || area_size == 0 {
            continue;
        }
        let colors = AxisColors::of(axis, ink);
        let text_size = area.estimate_text_size(&label, &colors.title_style)?;
        let half = text_size.1 as f64 / 2.0;
        let area_size = area_size as f64;
        let (center, rotation) = match place {
            PlotAxisSide::Left => ((plot_x - area_size + half, plot_y + plot_h / 2.0), 90.0),
            PlotAxisSide::Right => (
                (plot_x + plot_w + area_size - half, plot_y + plot_h / 2.0),
                -90.0,
            ),
            PlotAxisSide::Top => ((plot_x + plot_w / 2.0, plot_y - area_size + half), 0.0),
            PlotAxisSide::Bottom => (
                (plot_x + plot_w / 2.0, plot_y + plot_h + area_size - half),
                0.0,
            ),
        };
        texts.push(DrawnText {
            target: TextTarget::AxisTitle { panel: index, side },
            center,
            size: text_size,
            rotation,
            anchored: false,
        });
    }

    // Le rectangle de tracé, en pixels : c'est contre lui que les segments
    // d'une courbe sont rognés avant `paint::polyline` (voir `crate::clip`),
    // et non contre le cadrage en données — un axe logarithmique n'y serait
    // pas linéaire (doc de tête de `clip`).
    let bounds = PixelBounds::of_panel(origin, size);

    for (rank, series) in source.iter().copied().zip(&mut prepared) {
        if series.broken {
            drawn.broken_series += 1;
            continue;
        }
        if series.unknown_type {
            drawn.unknown_series += 1;
            continue;
        }
        if series.geometry.is_empty() {
            continue;
        }
        let right = series.axis == SeriesAxis::Right && right_active;
        let (removed, mut geometry) = paint_series(
            &screen,
            |point| {
                if right {
                    chart.secondary_plotting_area().map_coordinate(&point)
                } else {
                    chart.plotting_area().map_coordinate(&point)
                }
            },
            series,
            threshold,
            (x_range, if right { secondary } else { y_range }),
            bounds,
            ink,
        )?;
        geometry.index = rank;
        drawn.series_geometry.push(geometry);
        series.decimated = removed;
        drawn.decimated_points += removed;
        drawn.drawn_series += 1;
    }

    // 6 — la légende, par-dessus, et mesurée comme les axes.
    let projection = Projection {
        origin,
        size,
        ranges: Ranges {
            x: (x_scale, x_range),
            y: (y_scale, y_range),
            y2: y2_range.map(|range| (y2_scale, range)),
        },
        right_active,
    };
    let legend_center = usable_anchor(spec.legend.anchor)
        .and_then(|anchor| projection.pixel(anchor))
        .filter(|_| spec.legend.visible);
    let entry_ranks: Vec<usize> = prepared
        .iter()
        .enumerate()
        .filter(|(_, series)| series.in_legend())
        .map(|(index, _)| source[index])
        .collect();
    // Les courbes des calques superposés suivent, calque par calque (#172) ;
    // elles n'ont pas de rang dans ce panneau, donc pas de ligne désignable.
    let overlay_prepared: Vec<Prepared<'_>> = legend_overlays
        .iter()
        .flat_map(|layer| {
            let own = layer.spec.overlay.unwrap_or_default();
            let x = if own.x_top {
                layer.spec.bottom.scale
            } else {
                x_scale
            };
            let y = if own.y_right {
                layer.spec.left.scale
            } else {
                y_scale
            };
            layer
                .series
                .iter()
                .filter(|series| series.spec.visible)
                .map(move |series| series::prepare(series, x, y))
        })
        .collect();
    let entries: Vec<&Prepared<'_>> = prepared
        .iter()
        .chain(&overlay_prepared)
        .filter(|s| s.in_legend())
        .collect();
    let legend_ink = rgba_or(&spec.legend.color, opaque(ink));
    let inscribed = legend::draw(
        canvas,
        origin,
        size,
        &spec.legend,
        &entries,
        &legend::LegendText {
            typed: font_style(&spec.legend.font, LABEL_SIZE, legend_ink),
            literal: literal_style_font(
                spec.legend.font.effective_size(LABEL_SIZE),
                legend_ink,
                spec.legend.font.family(),
            ),
        },
        legend::LegendColors { background, ink },
        legend_center,
    )?;
    drawn.legend_entries = inscribed.entries;
    if let Some(((left, top), (frame_w, frame_h))) = inscribed.frame {
        texts.push(DrawnText {
            target: TextTarget::Legend { panel: index },
            center: (
                left as f64 + frame_w as f64 / 2.0,
                top as f64 + frame_h as f64 / 2.0,
            ),
            size: (frame_w, frame_h),
            rotation: 0.0,
            anchored: legend_center.is_some(),
        });
        drawn.legend_rows = inscribed
            .rows
            .iter()
            .zip(&entry_ranks)
            .map(|(&(center_y, height), &series)| DrawnLegendRow {
                series,
                center_y,
                height,
            })
            .collect();
    }

    // Les titres déplacés, posés sur leur ancre (issue #53) : par-dessus les
    // séries et la légende, comme les textes libres juste après.
    let centered = |anchor: TextAnchor| {
        projection.pixel(anchor).unwrap_or((
            origin.0 + (size.0 / 2) as i32,
            origin.1 + (size.1 / 2) as i32,
        ))
    };
    if let Some(anchor) = panel_title_anchor {
        let pixel = centered(anchor);
        let measured = draw_centered_text(canvas, &spec.title, &title_style, 0.0, pixel)?;
        texts.push(DrawnText {
            target: TextTarget::PanelTitle { panel: index },
            center: (pixel.0 as f64, pixel.1 as f64),
            size: measured,
            rotation: 0.0,
            anchored: true,
        });
    }
    for (axis, anchor, side, rotation) in [
        (&spec.bottom, bottom_anchor, PlotAxisSide::Bottom, 0.0),
        (&spec.left, left_anchor, PlotAxisSide::Left, 90.0),
        (&spec.right, right_anchor, PlotAxisSide::Right, -90.0),
    ] {
        let Some(anchor) = anchor else { continue };
        let pixel = centered(anchor);
        let colors = AxisColors::of(axis, ink);
        let measured = draw_centered_text(
            canvas,
            &axis.display_label(),
            &colors.title_style,
            rotation,
            pixel,
        )?;
        texts.push(DrawnText {
            target: TextTarget::AxisTitle { panel: index, side },
            center: (pixel.0 as f64, pixel.1 as f64),
            size: measured,
            rotation,
            anchored: true,
        });
    }
    drawn.texts = texts;

    let ranges = Ranges {
        x: (x_scale, x_range),
        y: (y_scale, y_range),
        y2: y2_range.map(|range| (y2_scale, range)),
    };

    // 6 bis — les objets graphiques (issue #76), du dessous au dessus, sous
    // les textes libres. Projetés sans test d'appartenance au cadrage : un
    // rectangle qui déborde du zoom est rogné, pas omis.
    for (index, shape) in spec.shapes.iter().enumerate() {
        let right = shape.axis == SeriesAxis::Right && right_active;
        if !shape.visible || !shape.is_finite() {
            continue;
        }
        let pixel_of = |x: f64, y: f64| {
            let point = ranges.forward(x, y, right)?;
            Some(if right {
                chart.secondary_plotting_area().map_coordinate(&point)
            } else {
                chart.plotting_area().map_coordinate(&point)
            })
        };
        let (Some(a), Some(b)) = (pixel_of(shape.x1, shape.y1), pixel_of(shape.x2, shape.y2))
        else {
            continue;
        };
        let (p1, p2) = ((a.0 as f64, a.1 as f64), (b.0 as f64, b.1 as f64));
        crate::shapes::draw_shape(&screen, shape, p1, p2, bounds, ink)?;
        drawn.shapes.push(crate::shapes::DrawnShape {
            index,
            kind: shape.kind,
            p1,
            p2,
            filled: shape.kind.is_closed() && parse_hex(&shape.fill_color).is_some(),
            width: shape.effective_width(),
        });
    }

    // 7 — les textes libres, par-dessus tout (issue #35). Comme un titre
    // ancré, ils ne sont pas bornés au cadrage : un texte posé dans les
    // marges reste tracé (#163).
    for (index, annotation) in spec.annotations.iter().enumerate() {
        if !annotation.visible {
            continue;
        }
        let right = annotation.axis == SeriesAxis::Right && right_active;
        let Some(point) = ranges.forward(annotation.x, annotation.y, right) else {
            continue;
        };
        let pixel = if right {
            chart.secondary_plotting_area().map_coordinate(&point)
        } else {
            chart.plotting_area().map_coordinate(&point)
        };
        if let Some(drawn_annotation) = draw_annotation(canvas, annotation, index, pixel, ink)? {
            drawn.annotations.push(drawn_annotation);
        }
    }

    // R7 : le réticule, le rectangle de zoom élastique et la désignation de
    // point se peignent **par-dessus** ce qui précède, et surtout par-dessus
    // le cache de `Shape` — donc pas ici, sous peine de rejouer tout le
    // pipeline à chaque mouvement de souris (`architecture-choisie.md`
    // §4.2, tâche 7.3).
    let frame = BaseFrame {
        area,
        x_range,
        y_range,
        x_scale,
        y_scale,
    };
    Ok((drawn, projection, frame))
}

/// Les cadrages d'un panneau, dans l'espace transformé, avec leur échelle —
/// de quoi projeter un point de données comme une série.
struct Ranges {
    x: (AxisScale, (f64, f64)),
    y: (AxisScale, (f64, f64)),
    y2: Option<(AxisScale, (f64, f64))>,
}

impl Ranges {
    /// Le point `(x, y)` dans l'espace transformé, **sans** test
    /// d'appartenance au cadrage (objets graphiques #76, textes libres #163) ; `None`
    /// seulement si l'échelle ne le représente pas (≤ 0 sur un axe log).
    fn forward(&self, x: f64, y: f64, right: bool) -> Option<(f64, f64)> {
        let (y_scale, _) = if right { self.y2? } else { self.y };
        Some((scale::forward(self.x.0, x)?, scale::forward(y_scale, y)?))
    }
}

/// Trace un texte libre centré sur `pixel`, et rend ce qui a été tracé.
///
/// Texte riche saisi (Typst), tourné par Typst d'un angle libre
/// ([`crate::formula::rotated_family`]). La taille rapportée est celle du
/// texte **droit**, mesurée par le même chemin : le rectangle désignable
/// est ce rectangle, tourné autour du centre.
fn draw_annotation<DB: DrawingBackend>(
    screen: &DrawingArea<DB, Shift>,
    annotation: &stokesforge_plot::AnnotationSpec,
    index: usize,
    pixel: plotters_backend::BackendCoord,
    ink: (u8, u8, u8),
) -> Result<Option<DrawnAnnotation>, DrawingAreaErrorKind<DB::ErrorType>> {
    if annotation.text.is_empty() {
        return Ok(None);
    }
    let color = rgba_or(&annotation.color, opaque(ink));
    let size = annotation.effective_size();
    let rotation = annotation.effective_rotation();
    let (width, height) = draw_centered_text(
        screen,
        &annotation.text,
        &rich_style_font(size, color, Some(annotation.font.as_str())),
        rotation,
        pixel,
    )?;
    Ok(Some(DrawnAnnotation {
        index,
        center: (pixel.0 as f64, pixel.1 as f64),
        size: (width, height),
        rotation,
    }))
}

/// Peint une série préparée, et rend le nombre de points que la décimation a
/// retirés.
fn paint_series<DB: DrawingBackend>(
    screen: &DrawingArea<DB, Shift>,
    map: impl Fn((f64, f64)) -> plotters_backend::BackendCoord,
    series: &Prepared<'_>,
    threshold: usize,
    ranges: ((f64, f64), (f64, f64)),
    bounds: Option<PixelBounds>,
    ink: (u8, u8, u8),
) -> Result<(usize, crate::series_selection::DrawnSeries), DrawingAreaErrorKind<DB::ErrorType>> {
    let mut geometry = crate::series_selection::DrawnSeries::default();
    let spec = series.spec;
    match &series.geometry {
        Geometry::Nothing => Ok((0, geometry)),
        Geometry::Bars(bars) => {
            // La ligne de base d'une barre est zéro quand l'axe sait le
            // représenter, et le bas du cadrage sinon — sur une échelle
            // logarithmique, zéro n'a pas de place.
            let baseline = scale_baseline(ranges.1);
            let rects: Vec<_> = bars
                .iter()
                .map(|(left, right, top)| (map((*left, *top)), map((*right, baseline))))
                .collect();
            if series.stale {
                paint::bars(screen, &rects, &paint::grayed(&spec.line), ink)?;
            } else {
                paint::bars(screen, &rects, &spec.line, ink)?;
            }
            // `a` est le coin côté valeur, `b` le coin côté ligne de base :
            // l'extrémité libre de la barre (la place de sa poignée de
            // sélection, #98) se lit avant que le rectangle soit normalisé.
            (geometry.bars, geometry.bar_tips) = rects
                .into_iter()
                .filter_map(|(a, b)| {
                    let (mut left, mut right) = (a.0.min(b.0), a.0.max(b.0));
                    let (mut top, mut bottom) = (a.1.min(b.1), a.1.max(b.1));
                    if let Some(bounds) = bounds {
                        left = left.max(bounds.x_min as i32);
                        right = right.min(bounds.x_max as i32);
                        top = top.max(bounds.y_min as i32);
                        bottom = bottom.min(bounds.y_max as i32);
                    }
                    // `clamp` seulement une fois le rectangle jugé non vide :
                    // il panique sur des bornes inversées.
                    (left <= right && top <= bottom).then(|| {
                        let tip = ((left + right) / 2, a.1.clamp(top, bottom));
                        (((left, top), (right, bottom)), tip)
                    })
                })
                .unzip();
            if series.stale {
                if let Some(&tip) = geometry.bar_tips.last() {
                    paint::stale_marker(screen, stale_marker_at(tip, bounds))?;
                }
            }
            Ok((0, geometry))
        }
        Geometry::Curve { points, whiskers } => {
            // Décimer une série à moustaches dissocierait les barres
            // d'erreur de leurs points ; décimer un nuage de points sans
            // trait n'a pas de sens non plus, le LTTB raisonnant le long
            // d'un chemin.
            let decimable =
                spec.plot_type.draws_line() && whiskers.is_empty() && points.len() > threshold;
            let decimated = decimable.then(|| decimate::lttb(points, threshold));
            let drawn_points: &[(f64, f64)] = decimated.as_deref().unwrap_or(points);
            let removed = points.len() - drawn_points.len();

            if spec.plot_type.draws_line() && spec.line.style != stokesforge_plot::LineStyle::None {
                // Les extrémités entières de Plotters fixent le repère (dernier
                // pixel inclus, axe Y inversé). Interpoler dans l'espace déjà
                // transformé évite son arrondi par sommet : au dézoom celui-ci
                // fabriquait des marches sur le spectre Jasco (#150).
                let (xr, yr) = ranges;
                let a = map((xr.0, yr.0));
                let b = map((xr.1, yr.1));
                let precise: Vec<_> = drawn_points
                    .iter()
                    .map(|&(x, y)| {
                        (
                            a.0 as f64 + (x - xr.0) / (xr.1 - xr.0) * (b.0 - a.0) as f64,
                            a.1 as f64 + (y - yr.0) / (yr.1 - yr.0) * (b.1 - a.1) as f64,
                        )
                    })
                    .collect();
                // Rognage géométrique contre le rectangle de tracé, **avant**
                // `paint::polyline_f64` : un segment qui traverse le panneau est
                // coupé à l'intersection réelle plutôt que ramené sur le bord
                // par composante (le défaut documenté en tête de
                // `crate::paint`, corrigé par `crate::clip`). Sans cadrage
                // qui déborde des données, chaque segment reste entièrement
                // dans `bounds` et ce rognage ne change rien : `clip_segment`
                // rend alors le segment tel quel.
                match bounds {
                    Some(bounds) => {
                        for run in clip::clip_polyline_f64(bounds, &precise) {
                            paint::polyline_f64(screen, &run, &spec.line, series.stale)?;
                            geometry.paths.push(quantized_path(&run));
                        }
                    }
                    None => {
                        paint::polyline_f64(screen, &precise, &spec.line, series.stale)?;
                        geometry.paths.push(quantized_path(&precise));
                    }
                }
            }
            if !whiskers.is_empty() {
                let segments: Vec<_> = whiskers
                    .iter()
                    .map(|(from, to)| (map(*from), map(*to)))
                    .collect();
                if series.stale {
                    paint::whiskers(screen, &segments, &paint::grayed(&spec.line))?;
                } else {
                    paint::whiskers(screen, &segments, &spec.line)?;
                }
                for (a, b) in segments {
                    match bounds {
                        Some(bounds) => {
                            if let Some((a, b)) = clip::clip_segment(
                                bounds,
                                (a.0 as f64, a.1 as f64),
                                (b.0 as f64, b.1 as f64),
                            ) {
                                geometry.whiskers.push((
                                    (a.0.round() as i32, a.1.round() as i32),
                                    (b.0.round() as i32, b.1.round() as i32),
                                ));
                            }
                        }
                        None => geometry.whiskers.push((a, b)),
                    }
                }
            }
            if spec.plot_type.draws_symbols()
                && spec.symbol.shape != stokesforge_plot::SymbolShape::None
            {
                let mapped: Vec<_> = drawn_points.iter().map(|point| map(*point)).collect();
                paint::symbols(screen, &mapped, &spec.symbol, series.stale)?;
                geometry.points = mapped
                    .into_iter()
                    .filter(|&(x, y)| {
                        bounds.is_none_or(|b| {
                            x as f64 >= b.x_min
                                && x as f64 <= b.x_max
                                && y as f64 >= b.y_min
                                && y as f64 <= b.y_max
                        })
                    })
                    .collect();
            }
            if series.stale {
                if let Some(&last) = drawn_points.last() {
                    paint::stale_marker(screen, stale_marker_at(map(last), bounds))?;
                }
            }
            Ok((removed, geometry))
        }
    }
}

/// Où poser le marqueur d'une série pas à jour : juste au-dessus de son dernier
/// point (il ne le cache pas), ramené dans le rectangle de tracé.
fn stale_marker_at(point: (i32, i32), bounds: Option<PixelBounds>) -> (i32, i32) {
    const INSET: i32 = 10;
    let (x, y) = (point.0, point.1 - 12);
    match bounds {
        Some(b) => (
            x.clamp(
                b.x_min as i32 + INSET,
                (b.x_max as i32 - INSET).max(b.x_min as i32 + INSET),
            ),
            y.clamp(
                b.y_min as i32 + INSET,
                (b.y_max as i32 - INSET).max(b.y_min as i32 + INSET),
            ),
        ),
        None => (x, y),
    }
}

/// Géométrie de désignation identique aux sommets réellement envoyés au
/// backend (seizièmes de pixel), plutôt qu'un second arrondi entier.
fn quantized_path(points: &[(f64, f64)]) -> Vec<(f64, f64)> {
    points
        .iter()
        .copied()
        .map(|p| crate::subpixel::decode(crate::subpixel::encode(p)))
        .collect()
}

/// La ligne de base des barres, dans l'espace transformé.
fn scale_baseline((low, high): (f64, f64)) -> f64 {
    let (bottom, top) = if low <= high {
        (low, high)
    } else {
        (high, low)
    };
    0.0f64.clamp(bottom, top)
}

/// Vrai si au moins une série de cet axe repose sur une ligne de base.
fn needs_baseline(prepared: &[Prepared<'_>], axis: SeriesAxis) -> bool {
    prepared
        .iter()
        .filter(|series| series.axis == axis)
        .any(|series| series.geometry.needs_baseline())
}

/// Étendue d'un ensemble de séries préparées.
///
/// `axis` restreint aux séries d'un axe donné (pour les ordonnées) ; `None`
/// prend tout (pour les abscisses, communes aux deux axes verticaux).
fn extent_of(
    prepared: &[Prepared<'_>],
    extent: impl Fn(&Geometry) -> Option<(f64, f64)>,
    axis: Option<SeriesAxis>,
) -> Option<(f64, f64)> {
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    let mut seen = false;
    for series in prepared {
        if series.broken {
            continue;
        }
        if axis.is_some_and(|wanted| series.axis != wanted) {
            continue;
        }
        if let Some((low, high)) = extent(&series.geometry) {
            min = min.min(low);
            max = max.max(high);
            seen = true;
        }
    }
    seen.then_some((min, max))
}

/// Bornes d'un axe, dans l'espace transformé.
///
/// Le cadrage explicite du spec prime s'il est complet **et** représentable
/// sur l'échelle choisie ; sinon l'étendue des données ; sinon `0..1`.
/// Un cadrage « de 800 à 400 » (`range_min > range_max`) trace l'axe à
/// l'envers, comme [`AxisSpec::inverted`] : les deux ne s'additionnent pas.
fn axis_range(
    spec: &AxisSpec,
    scale: AxisScale,
    data: Option<(f64, f64)>,
    with_baseline: bool,
) -> (f64, f64) {
    let mut reversed = false;
    let explicit = spec
        .has_explicit_range()
        .then(|| match (spec.range_min, spec.range_max) {
            (Some(min), Some(max)) if min.is_finite() && max.is_finite() && min != max => {
                reversed = min > max;
                let (low, high) = (min.min(max), min.max(max));
                match (scale::forward(scale, low), scale::forward(scale, high)) {
                    (Some(low), Some(high)) if low != high => Some((low, high)),
                    _ => None,
                }
            }
            _ => None,
        })
        .flatten();

    let (min, max) = match explicit {
        Some(range) => range,
        None => padded(scale, data, with_baseline),
    };
    if spec.inverted || reversed {
        (max, min)
    } else {
        (min, max)
    }
}

/// Joint l'axe d'un calque superposé à celui du calque recouvert (#174) : mêmes
/// bornes aux coins, graduations en face. Sans effet hors calque superposé, en
/// échelle logarithmique (les décades s'alignent déjà) ou avec une étendue
/// fixée à la main.
fn join_to_base(
    spec: &AxisSpec,
    scale: AxisScale,
    own: (f64, f64),
    base: Option<(AxisScale, (f64, f64))>,
) -> (f64, f64) {
    match base {
        Some((base_scale, base_range))
            if scale == AxisScale::Linear
                && base_scale == AxisScale::Linear
                && !spec.has_explicit_range() =>
        {
            scale::aligned_range(base_range, own).unwrap_or(own)
        }
        _ => own,
    }
}

/// L'étendue des données, élargie de ce qu'il faut pour être un cadrage.
fn padded(scale: AxisScale, data: Option<(f64, f64)>, with_baseline: bool) -> (f64, f64) {
    // Pas de données : `0..1`, qui vaut « une décade » sur une échelle
    // logarithmique puisque l'intervalle y est déjà celui des exposants.
    let Some((mut min, mut max)) = data else {
        return (0.0, 1.0);
    };
    if with_baseline {
        // Une barre qui ne montre pas d'où elle part ment sur sa hauteur.
        if let Some(baseline) = scale::forward(scale, 0.0) {
            min = min.min(baseline);
            max = max.max(baseline);
        }
    }
    if matches!(scale, AxisScale::Log) {
        // Les décades tiennent lieu de marge : élargir avant de s'y
        // accrocher ne changerait rien la plupart du temps, et ajouterait
        // une décade vide le reste du temps.
        return scale::snap_to_decades((min, max));
    }
    if min == max {
        // Une série constante, ou un seul point : sans cet écartement,
        // `build_cartesian_2d` recevrait un intervalle vide et Plotters
        // diviserait par zéro en projetant.
        let pad = if min == 0.0 { 1.0 } else { min.abs() * 0.05 };
        return (min - pad, max + pad);
    }
    let pad = (max - min) * 0.05;
    (min - pad, max + pad)
}

/// Remet un intervalle transformé dans l'espace des données, pour être
/// rapporté à l'appelant.
fn data_range(scale: AxisScale, (low, high): (f64, f64)) -> (f64, f64) {
    (scale::inverse(scale, low), scale::inverse(scale, high))
}

/// Aire de graduation effective : celle demandée, plafonnée au tiers de la
/// dimension disponible, nulle si l'axe est masqué.
fn label_area(visible: bool, wanted: u32, available: u32) -> u32 {
    if !visible {
        return 0;
    }
    wanted.min(available / 3)
}

/// Le style des **graduations**, à la taille et dans la couleur données
/// (opacité comprise) : mis en page par `epaint`, jamais par Typst. Titres,
/// titres d'axe et légende prennent [`rich_style`] (issue #32).
fn text_style(size: f64, color: (u8, u8, u8, u8)) -> TextStyle<'static> {
    (FONT_FAMILY, size).into_font().color(&rgba(color))
}

/// Le style d'un texte riche saisi (titre, titre d'axe, légende) : la couleur
/// donnée, le corps et la police de sa zone (issues #77, #78), `default_size`
/// et la police du graphe quand ils ne sont pas réglés.
fn font_style(
    font: &stokesforge_plot::TextFont,
    default_size: f64,
    color: (u8, u8, u8, u8),
) -> TextStyle<'static> {
    rich_style_font(font.effective_size(default_size), color, font.family())
}

/// Opacité des lignes principales de la grille, dans la couleur de
/// l'échelle — celle que Plotters donne par défaut sur du noir.
const GRID_BOLD_OPACITY: f64 = 0.2;
/// Opacité des lignes secondaires de la grille.
const GRID_LIGHT_OPACITY: f64 = 0.1;

/// Couleurs effectives d'un axe : celles du spec, ou l'encre automatique
/// quand elles sont vides.
struct AxisColors {
    ticks: (u8, u8, u8, u8),
    /// Le style du titre de l'axe : couleur, police et corps de sa zone.
    title_style: TextStyle<'static>,
}

impl AxisColors {
    fn of(axis: &AxisSpec, ink: (u8, u8, u8)) -> Self {
        let title = rgba_or(&axis.title_color, opaque(ink));
        Self {
            ticks: rgba_or(&axis.tick_color, opaque(ink)),
            title_style: font_style(&axis.title_font, AXIS_DESC_SIZE, title),
        }
    }

    /// Pose ces couleurs sur une passe de `configure_mesh` : libellés et
    /// ligne d'axe dans la couleur de l'échelle, grille dans la même,
    /// atténuée, titre de l'axe dans la sienne.
    fn apply<X, Y, DB>(&self, mesh: &mut plotters::chart::MeshStyle<'_, '_, X, Y, DB>)
    where
        X: plotters::coord::ranged1d::Ranged<ValueType = f64>
            + plotters::coord::ranged1d::ValueFormatter<f64>,
        Y: plotters::coord::ranged1d::Ranged<ValueType = f64>
            + plotters::coord::ranged1d::ValueFormatter<f64>,
        DB: DrawingBackend,
    {
        let ticks = rgba(self.ticks);
        mesh.label_style(text_style(LABEL_SIZE, self.ticks))
            .axis_desc_style(self.title_style.clone())
            .axis_style(ticks)
            .bold_line_style(ticks.mix(GRID_BOLD_OPACITY))
            .light_line_style(ticks.mix(GRID_LIGHT_OPACITY));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stokesforge_plot::{PanelSpec, Panels};

    fn spec_with(panels: Vec<PanelSpec>) -> PlotSpec {
        PlotSpec::empty("Grille").with_panels(Panels::new(panels))
    }

    /// « De 800 à 400 » trace l'axe à l'envers ; « de 400 à 800 » à l'endroit ;
    /// la case « Reverse axis » en plus ne s'additionne pas.
    #[test]
    fn un_cadrage_de_800_a_400_trace_l_axe_a_l_envers() {
        let mut axis = AxisSpec {
            auto_range: false,
            range_min: Some(800.0),
            range_max: Some(400.0),
            ..AxisSpec::default()
        };
        assert_eq!(
            axis_range(&axis, AxisScale::Linear, None, false),
            (800.0, 400.0)
        );
        axis.range_min = Some(400.0);
        axis.range_max = Some(800.0);
        assert_eq!(
            axis_range(&axis, AxisScale::Linear, None, false),
            (400.0, 800.0)
        );
        axis.inverted = true;
        assert_eq!(
            axis_range(&axis, AxisScale::Linear, None, false),
            (800.0, 400.0)
        );
        axis.range_min = Some(800.0);
        axis.range_max = Some(400.0);
        assert_eq!(
            axis_range(&axis, AxisScale::Linear, None, false),
            (800.0, 400.0)
        );
    }

    #[test]
    fn la_grille_declaree_est_celle_du_spec() {
        let spec = spec_with(vec![
            PanelSpec::at(0, 0),
            PanelSpec::at(0, 1),
            PanelSpec::at(1, 0),
        ]);
        let layout = grid_of(&spec, 3);
        assert_eq!((layout.rows, layout.columns), (2, 2));
        // Ordre ligne par ligne, comme `split_evenly`.
        assert_eq!(layout.cells, vec![0, 1, 2]);
    }

    #[test]
    fn un_trou_dans_la_grille_reste_un_trou() {
        let spec = spec_with(vec![PanelSpec::at(0, 0), PanelSpec::at(1, 2)]);
        let layout = grid_of(&spec, 2);
        assert_eq!((layout.rows, layout.columns), (2, 3));
        assert_eq!(layout.cells, vec![0, 5]);
    }

    #[test]
    fn une_position_demesuree_ne_fait_pas_decouper_linfini() {
        // Le cas que `PlotSpec::rows` documente déjà : une position venue
        // d'un fichier n'est bornée par rien.
        let spec = spec_with(vec![PanelSpec::at(0, 0), PanelSpec::at(usize::MAX, 0)]);
        let layout = grid_of(&spec, 2);
        assert!(
            layout.rows * layout.columns <= MAX_CELLS,
            "{layout:?} est indécoupable"
        );
        assert_eq!(layout.cells, vec![0, 1]);
    }

    #[test]
    fn la_ligne_de_base_reste_dans_le_cadrage() {
        assert_eq!(scale_baseline((-5.0, 5.0)), 0.0);
        // Sur une échelle logarithmique, le cadrage ne contient pas zéro :
        // les barres partent du bas de l'axe.
        assert_eq!(scale_baseline((1.0, 3.0)), 1.0);
        // Axe inversé : le bas est la plus grande des deux bornes.
        assert_eq!(scale_baseline((5.0, -5.0)), 0.0);
        assert_eq!(scale_baseline((3.0, 1.0)), 1.0);
    }

    #[test]
    fn une_aire_de_graduation_ne_mange_pas_la_cellule() {
        assert_eq!(label_area(true, 64, 600), 64);
        assert_eq!(label_area(true, 64, 90), 30);
        assert_eq!(label_area(false, 64, 600), 0);
    }
}
