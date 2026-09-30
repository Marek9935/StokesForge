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
//! Le `PlotSpec` réel : les six [`plotyx_plot::PlotType`], les deux
//! [`plotyx_plot::AxisScale`], l'axe secondaire, les styles de trait et de
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
//! 6. **la légende** en dernier, par-dessus, et mesurée par le même chemin
//!    que les axes ([`crate::legend`]).
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

use plotters::coord::Shift;
use plotters::prelude::*;
use plotyx_core::resolve::{ResolvedPanel, ResolvedPlot};
use plotyx_plot::{AxisScale, AxisSpec, PlotSpec, SeriesAxis};

use crate::clip::{self, PixelBounds};
use crate::color::{auto_ink, over_white, rgba_or};
use crate::decimate;
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

/// Taille par défaut d'une figure PlotYx, en pixels — `(largeur, hauteur)`.
///
/// Choix délibéré, pas encore un réglage. 4/3, comme visé — mais provisoire
/// (valeurs à revoir). Toute forme de configurabilité future (par graphe, par
/// export) passe par ce point unique plutôt que par une constante recopiée
/// dans chaque backend — `plotyx-render-svg` (`DEFAULT_SIZE`, export) l'utilise
/// comme taille fixe de page ; `plotyx-app` (vue écran, F11) ne s'en sert que
/// pour son ratio 4/3, la figure étant mise à l'échelle « contain » dans
/// l'espace disponible de la fenêtre (`graph_view_contents`).
pub const DEFAULT_FIGURE_SIZE: (u32, u32) = (480, 360);

/// Ce qu'un panneau a fini par tracer — de quoi écrire un test sans relire
/// des pixels.
#[derive(Debug, Clone, PartialEq)]
pub struct DrawnPanel {
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
    /// Nombre de séries réellement tracées.
    pub drawn_series: usize,
    /// Nombre de séries laissées de côté parce que leur référence est
    /// brisée. Jamais une erreur : `architecture-choisie.md` §11.1 en fait un
    /// état de première classe, à afficher et non à taire — elles
    /// apparaissent en légende, en gris et annotées.
    pub broken_series: usize,
    /// Nombre de séries dont le type de tracé vient d'une version plus
    /// récente de PlotYx (C5, niveau 2) : rien n'est dessiné, rien n'est
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
    /// À droite (ordonnée secondaire), si une série s'y rattache.
    pub right: AxisArea,
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
    let background = rgba_or(&spec.background, opaque(PAPER));
    // Encre automatique : celle de tout texte et de toute échelle dont la
    // couleur est laissée vide — blanche sur un fond sombre, sombre sinon.
    // Un fond (semi-)transparent est jugé composé sur du blanc.
    let ink = auto_ink(over_white(background));
    // Un fond d'opacité nulle n'est pas peint du tout : la figure exportée
    // (SVG, PNG, PDF) n'a pas de fond, pour s'intégrer à une autre
    // présentation.
    if background.3 > 0 {
        root.fill(&rgba(background))?;
    }

    // `titled` mesure le titre et rabaisse d'autant le haut de la zone : le
    // point où `estimate_text_size` entre dans la géométrie (voir la doc de
    // tête).
    let area = if spec.title.is_empty() {
        root.clone()
    } else {
        root.titled(
            &spec.title,
            text_style(TITLE_SIZE, rgba_or(&spec.title_color, opaque(ink))),
        )?
    };
    let title_height = (area.get_base_pixel().1 - root.get_base_pixel().1).max(0) as u32;

    let mut drawn = DrawnPlot {
        panels: Vec::with_capacity(plot.panels.len()),
        omitted_panels: 0,
        drawn_series: 0,
        broken_series: 0,
        unknown_series: 0,
        decimated_points: 0,
        title_height,
    };

    let layout = grid_of(spec, plot.panels.len());
    let cells = area.split_evenly((layout.rows, layout.columns));

    for (index, panel) in plot.panels.iter().enumerate() {
        let Some(cell) = layout.cells.get(index).and_then(|cell| cells.get(*cell)) else {
            drawn.omitted_panels += 1;
            continue;
        };
        let (w, h) = cell.dim_in_pixel();
        if w < MIN_CELL.0 || h < MIN_CELL.1 {
            drawn.omitted_panels += 1;
            continue;
        }
        let panel_drawn = draw_panel(cell, panel, background)?;
        drawn.drawn_series += panel_drawn.drawn_series;
        drawn.broken_series += panel_drawn.broken_series;
        drawn.unknown_series += panel_drawn.unknown_series;
        drawn.decimated_points += panel_drawn.decimated_points;
        drawn.panels.push(panel_drawn);
    }

    root.present()?;
    Ok(drawn)
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
/// fichier (`plotyx-plot` le documente, et `PlotSpec::rows` s'en protège
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

/// Trace un panneau dans sa cellule.
fn draw_panel<DB: DrawingBackend>(
    cell: &DrawingArea<DB, Shift>,
    panel: &ResolvedPanel<'_>,
    background: (u8, u8, u8, u8),
) -> Result<DrawnPanel, DrawingAreaErrorKind<DB::ErrorType>> {
    let spec = panel.spec;
    // Encre automatique du panneau, voir `draw_on`.
    let ink = auto_ink(over_white(background));
    let x_scale = spec.bottom.scale;
    let y_scale = spec.left.scale;
    let y2_scale = spec.right.scale;

    // 1 — préparation. Une série invisible ne figure nulle part : ni tracée,
    // ni cadrée, ni en légende.
    let mut prepared: Vec<Prepared<'_>> = panel
        .series
        .iter()
        .filter(|series| series.spec.visible)
        .map(|series| {
            let scale = if series.spec.axis == SeriesAxis::Right {
                y2_scale
            } else {
                y_scale
            };
            series::prepare(series, x_scale, scale)
        })
        .collect();

    let right_active = spec.right.visible
        && prepared
            .iter()
            .any(|series| series.axis == SeriesAxis::Right);

    // 2 — cadrage, dans l'espace transformé.
    let x_range = axis_range(
        &spec.bottom,
        x_scale,
        extent_of(&prepared, series::x_extent, None),
        false,
    );
    let y_range = axis_range(
        &spec.left,
        y_scale,
        extent_of(&prepared, series::y_extent, Some(SeriesAxis::Left)),
        needs_baseline(&prepared, SeriesAxis::Left),
    );
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
    let area = if spec.title.is_empty() {
        cell.clone()
    } else {
        cell.titled(&spec.title, text_style(PANEL_TITLE_SIZE, opaque(ink)))?
    };
    let (width, height) = area.dim_in_pixel();
    // Les aires de graduation sont plafonnées à un tiers de la cellule :
    // dans une grille, 64 px de libellés sur un panneau de 150 px ne
    // laisseraient plus rien à tracer.
    let x_label_area = label_area(spec.bottom.visible, X_LABEL_AREA, height);
    let y_label_area = label_area(spec.left.visible, Y_LABEL_AREA, width);
    let right_label_area = label_area(right_active, Y_LABEL_AREA, width);

    let mut builder = ChartBuilder::on(&area);
    builder
        .margin(MARGIN)
        .x_label_area_size(x_label_area)
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

    // 4 — les axes. Chaque axe a ses couleurs (titre, échelle), et le
    // `configure_mesh` de Plotters n'a qu'un style de titre et qu'un style
    // de ligne d'axe pour les deux : l'axe du bas et celui de gauche sont
    // donc tracés en deux passes, chacune coupant l'autre axe. La géométrie
    // n'en dépend pas — les aires de graduation sont fixées par le
    // `ChartBuilder` plus haut.
    let format_x = |value: &f64| scale::format_tick(x_scale, *value);
    let format_y = |value: &f64| scale::format_tick(y_scale, *value);
    {
        let colors = AxisColors::of(&spec.bottom, ink);
        let mut mesh = chart.configure_mesh();
        colors.apply(&mut mesh);
        mesh.x_desc(spec.bottom.display_label())
            .disable_y_mesh()
            .disable_y_axis();
        if !spec.bottom.grid {
            mesh.disable_x_mesh();
        }
        if !spec.bottom.visible {
            mesh.disable_x_axis();
        }
        if scale::needs_formatter(x_scale) {
            mesh.x_label_formatter(&format_x)
                .x_labels(scale::decade_label_count(x_range));
        }
        mesh.draw()?;
    }
    {
        let colors = AxisColors::of(&spec.left, ink);
        let mut mesh = chart.configure_mesh();
        colors.apply(&mut mesh);
        mesh.y_desc(spec.left.display_label())
            .disable_x_mesh()
            .disable_x_axis();
        if !spec.left.grid {
            mesh.disable_y_mesh();
        }
        if !spec.left.visible {
            mesh.disable_y_axis();
        }
        if scale::needs_formatter(y_scale) {
            mesh.y_label_formatter(&format_y)
                .y_labels(scale::decade_label_count(y_range));
        }
        mesh.draw()?;
    }

    let format_y2 = |value: &f64| scale::format_tick(y2_scale, *value);
    if let Some(range) = y2_range {
        let colors = AxisColors::of(&spec.right, ink);
        let mut mesh = chart.configure_secondary_axes();
        mesh.label_style(text_style(LABEL_SIZE, colors.ticks))
            .axis_desc_style(text_style(AXIS_DESC_SIZE, colors.title))
            .axis_style(rgba(colors.ticks))
            .y_desc(spec.right.display_label());
        if scale::needs_formatter(y2_scale) {
            mesh.y_label_formatter(&format_y2)
                .y_labels(scale::decade_label_count(range));
        }
        mesh.draw()?;
    }

    // 5 — les séries, en pixels.
    let plotting = chart.plotting_area();
    let origin = plotting.get_base_pixel();
    let size = plotting.dim_in_pixel();
    let screen = plotting.use_screen_coord();
    let threshold = decimate::threshold_for_width(size.0);

    let mut drawn = DrawnPanel {
        row: spec.row,
        column: spec.column,
        x_range: data_range(x_scale, x_range),
        y_range: data_range(y_scale, y_range),
        y2_range: y2_range.map(|range| data_range(y2_scale, range)),
        pixel_origin: origin,
        pixel_size: size,
        x_scale,
        y_scale,
        drawn_series: 0,
        broken_series: 0,
        unknown_series: 0,
        legend_entries: 0,
        decimated_points: 0,
        axis_areas: AxisAreas {
            bottom: AxisArea {
                size: x_label_area,
                titled: !spec.bottom.display_label().is_empty(),
            },
            left: AxisArea {
                size: y_label_area,
                titled: !spec.left.display_label().is_empty(),
            },
            right: AxisArea {
                size: right_label_area,
                titled: !spec.right.display_label().is_empty(),
            },
        },
    };

    // Le rectangle de tracé, en pixels : c'est contre lui que les segments
    // d'une courbe sont rognés avant `paint::polyline` (voir `crate::clip`),
    // et non contre le cadrage en données — un axe logarithmique n'y serait
    // pas linéaire (doc de tête de `clip`).
    let bounds = PixelBounds::of_panel(origin, size);

    for series in &mut prepared {
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
        let removed = paint_series(
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
            if right { secondary } else { y_range },
            bounds,
            ink,
        )?;
        series.decimated = removed;
        drawn.decimated_points += removed;
        drawn.drawn_series += 1;
    }

    // 6 — la légende, par-dessus, et mesurée comme les axes.
    let entries: Vec<&Prepared<'_>> = prepared.iter().filter(|s| s.in_legend()).collect();
    let inscribed = legend::draw(
        &screen,
        origin,
        size,
        &spec.legend,
        &entries,
        &text_style(LABEL_SIZE, opaque(ink)),
        legend::LegendColors { background, ink },
    )?;
    drawn.legend_entries = inscribed.entries;

    // R7 : le réticule, le rectangle de zoom élastique et la désignation de
    // point se peignent **par-dessus** ce qui précède, et surtout par-dessus
    // le cache de `Shape` — donc pas ici, sous peine de rejouer tout le
    // pipeline à chaque mouvement de souris (`architecture-choisie.md`
    // §4.2, tâche 7.3).
    Ok(drawn)
}

/// Peint une série préparée, et rend le nombre de points que la décimation a
/// retirés.
fn paint_series<DB: DrawingBackend>(
    screen: &DrawingArea<DB, Shift>,
    map: impl Fn((f64, f64)) -> plotters_backend::BackendCoord,
    series: &Prepared<'_>,
    threshold: usize,
    y_range: (f64, f64),
    bounds: Option<PixelBounds>,
    ink: (u8, u8, u8),
) -> Result<usize, DrawingAreaErrorKind<DB::ErrorType>> {
    let spec = series.spec;
    match &series.geometry {
        Geometry::Nothing => Ok(0),
        Geometry::Bars(bars) => {
            // La ligne de base d'une barre est zéro quand l'axe sait le
            // représenter, et le bas du cadrage sinon — sur une échelle
            // logarithmique, zéro n'a pas de place.
            let baseline = scale_baseline(y_range);
            let rects: Vec<_> = bars
                .iter()
                .map(|(left, right, top)| (map((*left, *top)), map((*right, baseline))))
                .collect();
            paint::bars(screen, &rects, &spec.line, ink)?;
            Ok(0)
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

            let mapped: Vec<_> = drawn_points.iter().map(|point| map(*point)).collect();
            if spec.plot_type.draws_line() {
                // Rognage géométrique contre le rectangle de tracé, **avant**
                // `paint::polyline` : un segment qui traverse le panneau est
                // coupé à l'intersection réelle plutôt que ramené sur le bord
                // par composante (le défaut documenté en tête de
                // `crate::paint`, corrigé par `crate::clip`). Sans cadrage
                // qui déborde des données, chaque segment reste entièrement
                // dans `bounds` et ce rognage ne change rien : `clip_segment`
                // rend alors le segment tel quel.
                match bounds {
                    Some(bounds) => {
                        for run in clip::clip_polyline(bounds, &mapped) {
                            paint::polyline(screen, &run, &spec.line, false)?;
                        }
                    }
                    None => paint::polyline(screen, &mapped, &spec.line, false)?,
                }
            }
            if !whiskers.is_empty() {
                let segments: Vec<_> = whiskers
                    .iter()
                    .map(|(from, to)| (map(*from), map(*to)))
                    .collect();
                paint::whiskers(screen, &segments, &spec.line)?;
            }
            if spec.plot_type.draws_symbols() {
                paint::symbols(screen, &mapped, &spec.symbol, false)?;
            }
            Ok(removed)
        }
    }
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
fn axis_range(
    spec: &AxisSpec,
    scale: AxisScale,
    data: Option<(f64, f64)>,
    with_baseline: bool,
) -> (f64, f64) {
    let explicit = spec
        .has_explicit_range()
        .then(|| match (spec.range_min, spec.range_max) {
            (Some(min), Some(max)) if min.is_finite() && max.is_finite() && min != max => {
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
    if spec.inverted {
        (max, min)
    } else {
        (min, max)
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

/// Le style de texte du projet, à la taille et dans la couleur données
/// (opacité comprise).
fn text_style(size: f64, color: (u8, u8, u8, u8)) -> TextStyle<'static> {
    (FONT_FAMILY, size).into_font().color(&rgba(color))
}

/// Opacité des lignes principales de la grille, dans la couleur de
/// l'échelle — celle que Plotters donne par défaut sur du noir.
const GRID_BOLD_OPACITY: f64 = 0.2;
/// Opacité des lignes secondaires de la grille.
const GRID_LIGHT_OPACITY: f64 = 0.1;

/// Couleurs effectives d'un axe : celles du spec, ou l'encre automatique
/// quand elles sont vides.
struct AxisColors {
    title: (u8, u8, u8, u8),
    ticks: (u8, u8, u8, u8),
}

impl AxisColors {
    fn of(axis: &AxisSpec, ink: (u8, u8, u8)) -> Self {
        Self {
            title: rgba_or(&axis.title_color, opaque(ink)),
            ticks: rgba_or(&axis.tick_color, opaque(ink)),
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
            .axis_desc_style(text_style(AXIS_DESC_SIZE, self.title))
            .axis_style(ticks)
            .bold_line_style(ticks.mix(GRID_BOLD_OPACITY))
            .light_line_style(ticks.mix(GRID_LIGHT_OPACITY));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plotyx_plot::{PanelSpec, Panels};

    fn spec_with(panels: Vec<PanelSpec>) -> PlotSpec {
        PlotSpec::empty("Grille").with_panels(Panels::new(panels))
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
