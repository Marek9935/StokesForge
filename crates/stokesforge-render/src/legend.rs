//! La légende — tâche R5.3.
//!
//! # Pourquoi elle est écrite ici et non déléguée à Plotters
//!
//! `ChartContext::configure_series_labels()` existe et fait presque
//! l'affaire. Presque : pour dimensionner son cadre, il appelle
//! `MultiLineText::estimate_dimension`, qui passe par
//! `FontData::estimate_layout` — **pas** par `estimate_text_size` du
//! backend. Nos fonts Plotters étant coupées
//! (`default-features = false`, `architecture-choisie.md` §4.4), cette
//! estimation est l'heuristique monospace par octets UTF-8 : « λ » compte
//! pour deux, « Échantillon » pour douze caractères et treize octets.
//!
//! Le cadre serait donc mesuré faux pendant que le texte dedans serait
//! dessiné juste — un décalage qui ne lève aucune erreur, exactement le
//! défaut silencieux que ce chantier est construit pour empêcher. Et il
//! serait faux **de la même façon des deux côtés**, donc le test d'accord ne
//! l'attraperait pas : c'est le cas qui justifie de ne pas s'en remettre au
//! seul test.
//!
//! La légende est donc mise en page ici, sur
//! [`plotters::drawing::DrawingArea::estimate_text_size`], qui est le
//! chemin du backend — donc [`crate::text::estimate`], donc la mesure unique
//! du projet.
//!
//! # Ce qu'une entrée montre
//!
//! Sa vignette est celle de la série : le trait avec son style, le marqueur
//! avec sa forme, ou un aplat pour une série de barres. Une série à
//! **référence brisée** y figure aussi, en gris et annotée — c'est le seul
//! endroit de la figure où elle peut se montrer, puisqu'elle n'a rien à
//! tracer (`architecture-choisie.md` §11.1 : « à afficher comme telle et
//! jamais comme une série vide »).

use plotters::coord::Shift;
use plotters::element::{Rectangle, Text};
use plotters::prelude::*;
use plotters::style::TextStyle;
use plotters_backend::{BackendCoord, DrawingBackend};
use stokesforge_plot::{LegendPosition, LegendSpec, PlotType};

use crate::paint::{self, BROKEN, SERIES_FALLBACK};
use crate::series::{Geometry, Prepared};

/// Marge entre le cadre de la légende et le bord de la zone de tracé.
const MARGIN: i32 = 8;
/// Marge intérieure du cadre.
const PADDING: i32 = 6;
/// Largeur de la vignette (le bout de trait, ou l'aplat).
pub const KEY_WIDTH: i32 = 26;
/// Écart entre la vignette et le texte.
const KEY_GAP: i32 = 6;
/// Écart vertical entre deux entrées.
const ROW_GAP: i32 = 4;
/// Nombre d'entrées au-delà duquel la légende est tronquée.
///
/// Un graphe peut porter des centaines de séries (le cas de charge du
/// document en compte 500) ; une légende de 500 lignes ne rentre dans aucune
/// zone de tracé et coûterait 500 mesures de texte par image. Au-delà, les
/// premières sont listées et la dernière ligne dit combien manquent.
const MAX_ENTRIES: usize = 24;

/// Ce que la légende a effectivement inscrit.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DrawnLegend {
    /// Nombre d'entrées dessinées, la ligne de débordement non comprise.
    pub entries: usize,
    /// Nombre d'entrées laissées de côté faute de place.
    pub omitted: usize,
    /// Le cadre tracé, en pixels de backend : coin haut-gauche et taille
    /// (issue #53 : désigner la légende sous le pointeur).
    pub frame: Option<((i32, i32), (u32, u32))>,
    /// Pour chaque entrée dessinée, dans l'ordre : milieu vertical et
    /// hauteur de sa ligne, en pixels de backend.
    pub rows: Vec<(f64, f64)>,
}

/// Les deux styles de texte de la légende : celui d'un nom saisi (markup
/// Typst) et celui d'un texte repris d'une colonne ou généré ici, affiché tel
/// quel (issue #33). Même corps, même couleur.
#[derive(Clone)]
pub struct LegendText<'a> {
    /// Nom de série saisi (`formula::rich_style`).
    pub typed: TextStyle<'a>,
    /// Nom repris de la colonne, ligne « … et N autres »
    /// (`formula::literal_style`).
    pub literal: TextStyle<'a>,
}

/// Couleurs de la légende : le fond de son cadre (celui du graphe) et
/// l'encre de son contour et de ses vignettes (l'encre automatique du fond,
/// `crate::color::auto_ink`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LegendColors {
    /// Fond du cadre, opacité comprise (transparent si le graphe n'a pas de
    /// fond).
    pub background: (u8, u8, u8, u8),
    /// Contour du cadre et des vignettes de barres.
    pub ink: (u8, u8, u8),
}

/// Trace la légende d'un panneau dans sa zone de tracé.
///
/// `area` doit être la zone de tracé en coordonnées **écran**
/// (`plotting_area().use_screen_coord()`), et `origin`/`size` sa position et
/// sa taille : la légende se place par rapport à la zone de tracé, pas par
/// rapport au panneau, pour ne pas chevaucher les graduations.
///
/// `center` (issue #53) : le centre où poser le cadre, en pixels de backend,
/// quand la légende a été déplacée ; il prime sur le coin d'ancrage.
#[allow(clippy::too_many_arguments)]
pub fn draw<DB: DrawingBackend>(
    area: &DrawingArea<DB, Shift>,
    origin: BackendCoord,
    size: (u32, u32),
    spec: &LegendSpec,
    entries: &[&Prepared<'_>],
    styles: &LegendText<'_>,
    colors: LegendColors,
    center: Option<BackendCoord>,
) -> Result<DrawnLegend, DrawingAreaErrorKind<DB::ErrorType>> {
    let LegendColors { background, ink } = colors;
    if !spec.visible || entries.is_empty() {
        return Ok(DrawnLegend::default());
    }

    let omitted = entries.len().saturating_sub(MAX_ENTRIES);
    let shown = &entries[..entries.len().min(MAX_ENTRIES)];

    // Les textes sont mesurés une fois, et c'est la même mesure qui décide
    // du cadre et qui a servi à placer les axes.
    let mut texts: Vec<(String, &TextStyle<'_>)> = shown
        .iter()
        .map(|entry| {
            let style = if entry.legend_is_typed() {
                &styles.typed
            } else {
                &styles.literal
            };
            (entry.legend_text(), style)
        })
        .collect();
    if omitted > 0 {
        texts.push((format!("… et {omitted} autres"), &styles.literal));
    }

    let mut label_width = 0i32;
    let mut row_height = 1i32;
    for (text, style) in &texts {
        let (w, h) = area.estimate_text_size(text, style)?;
        label_width = label_width.max(w as i32);
        row_height = row_height.max(h as i32);
    }

    let rows = texts.len() as i32;
    let box_width = PADDING * 2 + KEY_WIDTH + KEY_GAP + label_width;
    let box_height = PADDING * 2 + rows * row_height + (rows - 1).max(0) * ROW_GAP;

    let (area_w, area_h) = (size.0 as i32, size.1 as i32);
    let left = match spec.position {
        LegendPosition::TopLeft | LegendPosition::BottomLeft => MARGIN,
        LegendPosition::TopRight | LegendPosition::BottomRight => area_w - box_width - MARGIN,
    };
    let top = match spec.position {
        LegendPosition::TopLeft | LegendPosition::TopRight => MARGIN,
        LegendPosition::BottomLeft | LegendPosition::BottomRight => area_h - box_height - MARGIN,
    };
    // Une légende plus large que sa zone de tracé reste ancrée en haut à
    // gauche plutôt que de sortir par le côté opposé : le rognage de
    // Plotters s'occupe du reste.
    let (x0, y0) = match center {
        Some((cx, cy)) => (cx - box_width / 2, cy - box_height / 2),
        None => (origin.0 + left.max(0), origin.1 + top.max(0)),
    };

    let frame = [(x0, y0), (x0 + box_width, y0 + box_height)];
    area.draw(&Rectangle::new(
        frame,
        ShapeStyle {
            color: paint::rgba(background),
            filled: true,
            stroke_width: 1,
        },
    ))?;
    area.draw(&Rectangle::new(
        frame,
        ShapeStyle {
            color: paint::rgb(ink).into(),
            filled: false,
            stroke_width: 1,
        },
    ))?;

    for (index, (text, style)) in texts.iter().enumerate() {
        let row_top = y0 + PADDING + index as i32 * (row_height + ROW_GAP);
        let middle = row_top + row_height / 2;
        let key_left = x0 + PADDING;
        if let Some(entry) = shown.get(index) {
            key(area, entry, (key_left, middle), ink)?;
        }
        area.draw(&Text::new(
            text.as_str(),
            (key_left + KEY_WIDTH + KEY_GAP, row_top),
            *style,
        ))?;
    }

    let row_boxes = (0..shown.len())
        .map(|index| {
            let row_top = y0 + PADDING + index as i32 * (row_height + ROW_GAP);
            (row_top as f64 + row_height as f64 / 2.0, row_height as f64)
        })
        .collect();
    Ok(DrawnLegend {
        entries: shown.len(),
        omitted,
        frame: Some(((x0, y0), (box_width as u32, box_height as u32))),
        rows: row_boxes,
    })
}

/// La vignette d'une entrée : ce que la série a l'air d'être.
fn key<DB: DrawingBackend>(
    area: &DrawingArea<DB, Shift>,
    entry: &Prepared<'_>,
    (left, middle): BackendCoord,
    ink: (u8, u8, u8),
) -> Result<(), DrawingAreaErrorKind<DB::ErrorType>> {
    series_key(
        area,
        entry.spec,
        entry.broken,
        matches!(entry.geometry, Geometry::Bars(_)),
        (left, middle),
        ink,
    )
}

/// Vignette de style sans préparation des valeurs (#82) : ouvrir le panneau
/// ne doit ni parcourir une colonne ni calculer son histogramme à chaque image.
pub fn series_key<DB: DrawingBackend>(
    area: &DrawingArea<DB, Shift>,
    spec: &stokesforge_plot::SeriesSpec,
    broken: bool,
    bars: bool,
    (left, middle): BackendCoord,
    ink: (u8, u8, u8),
) -> Result<(), DrawingAreaErrorKind<DB::ErrorType>> {
    if bars {
        let color = if broken {
            BROKEN
        } else {
            crate::color::rgb_or(&spec.line.color, SERIES_FALLBACK)
        };
        let swatch = [(left, middle - 5), (left + KEY_WIDTH, middle + 5)];
        area.draw(&Rectangle::new(
            swatch,
            ShapeStyle {
                color: paint::rgb(color).into(),
                filled: true,
                stroke_width: 1,
            },
        ))?;
        return area.draw(&Rectangle::new(
            swatch,
            ShapeStyle {
                color: paint::rgb(ink).into(),
                filled: false,
                stroke_width: 1,
            },
        ));
    }

    let draws_line = spec.plot_type.draws_line() || broken;
    if draws_line {
        paint::polyline(
            area,
            &[(left, middle), (left + KEY_WIDTH, middle)],
            &spec.line,
            broken,
        )?;
    }
    if spec.plot_type.draws_symbols() && !matches!(spec.plot_type, PlotType::Unknown(_)) {
        paint::symbol_at(area, (left + KEY_WIDTH / 2, middle), &spec.symbol, broken)?;
    }
    Ok(())
}
