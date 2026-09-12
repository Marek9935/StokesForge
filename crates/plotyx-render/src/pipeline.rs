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
//! gratuite : la structure, plutôt que la discipline.
//!
//! # Périmètre R4 : le graphe le plus pauvre possible
//!
//! Une `LineSeries`, axes linéaires, pas de légende, pas de marqueur, pas
//! d'échelle logarithmique, pas de panneaux multiples. Ce qui manque est le
//! programme de R5 et est signalé par un `// R5 :` à l'endroit exact où il
//! s'insérera, pas par une liste en tête de fichier qui se périmerait.
//!
//! # Où la mesure de texte entre dans la géométrie
//!
//! Ce n'est pas anecdotique, et c'est ce qui rend le test d'accord (R4.5)
//! autre chose qu'une formalité : `DrawingArea::titled` — appelé dès que le
//! graphe a un titre — demande la hauteur du titre à
//! `estimate_text_size`, puis fait **descendre le haut de la zone de tracé**
//! de cette hauteur. Une mesure fausse de quelques pixels déplace donc la
//! zone entière, axes compris, sans qu'aucune erreur ne soit levée. C'est
//! exactement le défaut silencieux décrit en `architecture-choisie.md` §4.4,
//! et le test d'accord le prend par là.

use plotters::coord::Shift;
use plotters::prelude::*;
use plotyx_core::resolve::{ResolvedPanel, ResolvedPlot, ResolvedSeries};

use crate::color::rgb_or;

/// Taille (au sens de Plotters) du titre du graphe.
pub const TITLE_SIZE: f64 = 24.0;
/// Taille (au sens de Plotters) des chiffres de graduation.
pub const LABEL_SIZE: f64 = 14.0;
/// Taille (au sens de Plotters) des titres d'axe.
pub const AXIS_DESC_SIZE: f64 = 16.0;
/// Marge extérieure du graphe, en unités de backend.
pub const MARGIN: u32 = 12;
/// Hauteur réservée aux graduations et au titre de l'axe du bas.
pub const X_LABEL_AREA: u32 = 52;
/// Largeur réservée aux graduations et au titre de l'axe de gauche.
pub const Y_LABEL_AREA: u32 = 64;
/// Famille de police demandée à Plotters. Elle est traduite en
/// [`epaint::FontFamily::Proportional`] par [`crate::text::font_id`] ; ce que
/// Plotters en fait par lui-même n'a aucune importance, ses fonts étant
/// désactivées.
pub const FONT_FAMILY: &str = "sans-serif";

/// Couleur de repli d'un trait de série dont la chaîne du spec est illisible.
const SERIES_FALLBACK: (u8, u8, u8) = (0x4c, 0x78, 0xa8);
/// Couleur du texte et des axes.
const INK: (u8, u8, u8) = (0x20, 0x20, 0x20);
/// Fond de repli quand `PlotSpec::background` est vide ou illisible.
const PAPER: (u8, u8, u8) = (0xff, 0xff, 0xff);

/// Ce qu'un panneau a fini par tracer — de quoi écrire un test sans relire
/// des pixels.
#[derive(Debug, Clone, PartialEq)]
pub struct DrawnPlot {
    /// Bornes effectivement retenues en X, après cadrage explicite ou
    /// déduction depuis les données.
    pub x_range: (f64, f64),
    /// Idem en Y.
    pub y_range: (f64, f64),
    /// Nombre de séries réellement tracées (les références brisées et les
    /// séries invisibles n'y sont pas).
    pub drawn_series: usize,
    /// Nombre de séries laissées de côté parce que leur référence est
    /// brisée. Jamais une erreur : `architecture-choisie.md` §11.1 en fait un
    /// état de première classe, à afficher et non à taire.
    pub broken_series: usize,
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
/// [`DrawingArea`] — un futur découpage en panneaux, par exemple — n'ait pas
/// à en refaire une.
pub fn draw_on<DB: DrawingBackend>(
    root: &DrawingArea<DB, Shift>,
    plot: &ResolvedPlot<'_>,
) -> Result<DrawnPlot, DrawingAreaErrorKind<DB::ErrorType>> {
    let spec = plot.spec;
    root.fill(&rgb(rgb_or(&spec.background, PAPER)))?;

    // `titled` mesure le titre et rabaisse d'autant le haut de la zone : le
    // point où `estimate_text_size` entre dans la géométrie (voir la doc de
    // tête).
    let area = if spec.title.is_empty() {
        root.clone()
    } else {
        root.titled(&spec.title, text_style(TITLE_SIZE))?
    };

    // R5 : `spec.rows()` × `spec.columns()` panneaux, via `area.split_evenly`.
    // R4 s'en tient au premier — le graphe minimal en a exactement un.
    let Some(panel) = plot.panels.first() else {
        return Ok(DrawnPlot {
            x_range: (0.0, 1.0),
            y_range: (0.0, 1.0),
            drawn_series: 0,
            broken_series: 0,
        });
    };

    let x_range = axis_range(panel, Axis::X);
    let y_range = axis_range(panel, Axis::Y);

    let mut chart = ChartBuilder::on(&area)
        .margin(MARGIN)
        .x_label_area_size(X_LABEL_AREA)
        .y_label_area_size(Y_LABEL_AREA)
        .build_cartesian_2d(x_range.0..x_range.1, y_range.0..y_range.1)?;

    // R5 : `AxisScale::Log` (`build_cartesian_2d` sur un `LogRange`), axe de
    // droite (`build_cartesian_2d` + `set_secondary_coord`), axe du haut.
    let mut mesh = chart.configure_mesh();
    mesh.label_style(text_style(LABEL_SIZE))
        .axis_desc_style(text_style(AXIS_DESC_SIZE))
        .axis_style(rgb(INK))
        .x_desc(panel.spec.bottom.display_label())
        .y_desc(panel.spec.left.display_label());
    if !panel.spec.bottom.grid && !panel.spec.left.grid {
        mesh.disable_mesh();
    }
    mesh.draw()?;

    let mut drawn = 0usize;
    let mut broken = 0usize;
    for series in &panel.series {
        if series.broken() {
            // R5 : le rendu doit *montrer* une référence brisée, pas
            // seulement s'abstenir (§11.1). Compter est le minimum honnête
            // tant qu'il n'y a pas de place où l'écrire.
            broken += 1;
            continue;
        }
        if !series.spec.visible {
            continue;
        }
        let Some(points) = points(series) else {
            continue;
        };
        if points.is_empty() {
            continue;
        }
        // R5 : `PlotType` — points, barres, histogramme, barres d'erreur. R4
        // trace tout en ligne, y compris ce qui demandait autre chose : le
        // graphe minimal de la phase est « une LineSeries », et un rendu
        // muet vaut mieux qu'un rendu qui ment sur un type non encore porté.
        let style = ShapeStyle {
            color: rgb(rgb_or(&series.spec.line.color, SERIES_FALLBACK)).into(),
            filled: false,
            stroke_width: stroke_width(series.spec.line.width),
        };
        chart.draw_series(LineSeries::new(points, style))?;
        drawn += 1;
    }

    // R5 : `LegendSpec` — `chart.configure_series_labels()`.
    root.present()?;

    Ok(DrawnPlot {
        x_range,
        y_range,
        drawn_series: drawn,
        broken_series: broken,
    })
}

/// Les points d'une série, X pris dans sa colonne d'abscisses ou, à défaut,
/// dans l'indice de l'échantillon.
///
/// L'indice comme abscisse de repli est le comportement de la version
/// Python (`resolve.py` : une série sans X se trace contre son rang) et le
/// seul qui garde traçable une colonne seule — le cas le plus courant.
fn points(series: &ResolvedSeries<'_>) -> Option<Vec<(f64, f64)>> {
    let y = series.y.as_ref()?;
    let ys = y.as_slice();
    let xs = series.x.as_ref().map(|x| x.as_slice());
    let pairs: Vec<(f64, f64)> = match xs {
        Some(xs) => xs
            .iter()
            .zip(ys.iter())
            .map(|(x, y)| (*x, *y))
            .filter(|(x, y)| x.is_finite() && y.is_finite())
            .collect(),
        None => ys
            .iter()
            .enumerate()
            .map(|(index, y)| (index as f64, *y))
            .filter(|(_, y)| y.is_finite())
            .collect(),
    };
    Some(pairs)
}

/// Épaisseur de trait : `LineSpec::width` est un `f64` libre côté spec (rien
/// ne le borne), `ShapeStyle::stroke_width` est un `u32`.
fn stroke_width(width: f64) -> u32 {
    if width.is_finite() && width >= 1.0 {
        // Plafonné : une épaisseur venue d'un fichier peut valoir n'importe
        // quoi, et `polygonize` de Plotters alloue proportionnellement.
        (width.round() as u32).min(64)
    } else {
        1
    }
}

/// Lequel des deux axes on borne.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Axis {
    X,
    Y,
}

/// Bornes d'un axe : le cadrage explicite du spec s'il est complet, sinon
/// l'étendue des données, sinon `0..1`.
fn axis_range(panel: &ResolvedPanel<'_>, axis: Axis) -> (f64, f64) {
    let spec = match axis {
        Axis::X => &panel.spec.bottom,
        // R5 : `SeriesAxis::Right` — une série rattachée à droite doit se
        // borner sur `panel.spec.right`, pas sur celui-ci.
        Axis::Y => &panel.spec.left,
    };

    let explicit = if spec.has_explicit_range() {
        match (spec.range_min, spec.range_max) {
            (Some(min), Some(max)) if min.is_finite() && max.is_finite() && min != max => {
                Some((min.min(max), min.max(max)))
            }
            _ => None,
        }
    } else {
        None
    };

    let (min, max) = explicit.unwrap_or_else(|| data_range(panel, axis));
    if spec.inverted {
        (max, min)
    } else {
        (min, max)
    }
}

/// Étendue des données d'un panneau sur un axe, marge de 5 % comprise.
fn data_range(panel: &ResolvedPanel<'_>, axis: Axis) -> (f64, f64) {
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for series in &panel.series {
        if series.broken() || !series.spec.visible {
            continue;
        }
        let Some(pairs) = points(series) else {
            continue;
        };
        for (x, y) in pairs {
            let value = match axis {
                Axis::X => x,
                Axis::Y => y,
            };
            min = min.min(value);
            max = max.max(value);
        }
    }

    if !min.is_finite() || !max.is_finite() {
        return (0.0, 1.0);
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

/// Une couleur de Plotters depuis un triplet.
fn rgb((r, g, b): (u8, u8, u8)) -> RGBColor {
    RGBColor(r, g, b)
}

/// Le style de texte du projet, à la taille donnée.
fn text_style(size: f64) -> TextStyle<'static> {
    (FONT_FAMILY, size).into_font().color(&rgb(INK))
}
