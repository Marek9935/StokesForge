//! Des données résolues à une géométrie traçable — tâche R5.1.
//!
//! # Une étape séparée, et pourquoi
//!
//! R4 lisait les points au moment de tracer et, séparément, au moment de
//! cadrer les axes : deux parcours, deux occasions de ne pas dire la même
//! chose. Avec des barres et des histogrammes, ce n'est plus tenable — la
//! géométrie d'un histogramme ne se déduit pas de ses données sans les
//! classer, et un cadrage qui l'ignorerait couperait la dernière classe.
//!
//! D'où cette étape : chaque série est **préparée une fois**, en géométrie,
//! et le cadrage comme le tracé lisent la même chose. C'est aussi ce qui
//! rend la couverture des [`PlotType`] vérifiable : le type de tracé ne
//! s'interprète qu'ici.
//!
//! # Espace transformé
//!
//! Tout ce que ce module produit est déjà dans l'espace où la zone de tracé
//! travaille — log₁₀ compris (voir [`crate::scale`]). Le reste du pipeline
//! n'a plus à connaître l'échelle des axes, sauf pour étiqueter.

use std::num::NonZeroU32;

use stokesforge_core::resolve::{bar_width, histogram, ResolvedSeries};
use stokesforge_plot::{AxisScale, PlotType, SeriesAxis, SeriesSpec};

use crate::scale::forward;

/// Un point dans l'espace de tracé.
pub type Point = (f64, f64);

/// Une moustache de barre d'erreur : deux extrémités, déjà transformées.
pub type Whisker = (Point, Point);

/// Ce qu'une série est devenue, une fois traduite en géométrie.
#[derive(Debug, Clone, PartialEq)]
pub enum Geometry {
    /// Rien à tracer : type de tracé inconnu de ce lecteur, colonne vide, ou
    /// toutes les valeurs hors de l'échelle choisie.
    Nothing,
    /// Une suite de points, à relier et/ou à marquer selon le type de tracé.
    Curve {
        /// Les points, dans l'ordre de la colonne.
        points: Vec<Point>,
        /// Les moustaches de barres d'erreur, vides si la série n'en a pas.
        whiskers: Vec<Whisker>,
    },
    /// Des barres verticales : `(bord gauche, bord droit, sommet)`.
    Bars(Vec<(f64, f64, f64)>),
}

impl Geometry {
    /// Vrai si rien ne sera dessiné.
    pub fn is_empty(&self) -> bool {
        match self {
            Geometry::Nothing => true,
            Geometry::Curve { points, .. } => points.is_empty(),
            Geometry::Bars(bars) => bars.is_empty(),
        }
    }

    /// Nombre de points portés — ce que le LTTB peut réduire.
    pub fn point_count(&self) -> usize {
        match self {
            Geometry::Nothing => 0,
            Geometry::Curve { points, .. } => points.len(),
            Geometry::Bars(bars) => bars.len(),
        }
    }

    /// Vrai si la géométrie s'appuie sur une ligne de base (les barres), qui
    /// doit alors entrer dans le cadrage sous peine de barres flottantes.
    pub fn needs_baseline(&self) -> bool {
        matches!(self, Geometry::Bars(_))
    }
}

/// Une série prête à tracer : sa géométrie, et ce qu'il faut pour la
/// dessiner et l'inscrire en légende.
#[derive(Debug)]
pub struct Prepared<'s> {
    /// Le spec d'origine — le style s'y lit au moment de peindre.
    pub spec: &'s SeriesSpec,
    /// Nom affiché, tel que la résolution l'a établi.
    pub label: String,
    /// Axe des ordonnées de rattachement.
    pub axis: SeriesAxis,
    /// La géométrie, dans l'espace de tracé.
    pub geometry: Geometry,
    /// Vrai si la référence de la série est brisée : rien à tracer, mais la
    /// série doit **se voir** (`architecture-choisie.md` §11.1).
    pub broken: bool,
    /// Vrai si le type de tracé vient d'une version plus récente de StokesForge
    /// (C5, niveau 2) : rien n'est dessiné, et rien n'est perdu non plus.
    pub unknown_type: bool,
    /// Nombre de points retirés par la décimation, renseigné après coup par
    /// le pipeline.
    pub decimated: usize,
    /// Une colonne de la série vient d'un calcul pas à jour et le graphe en
    /// avertit (#198) : trait grisé et marqueur « ! ».
    pub stale: bool,
}

impl Prepared<'_> {
    /// Vrai si quelque chose sera effectivement peint dans la zone de tracé.
    pub fn draws(&self) -> bool {
        !self.broken && !self.geometry.is_empty()
    }

    /// Vrai si la série a sa place en légende.
    ///
    /// Une référence brisée y a droit — c'est même le seul endroit où elle
    /// peut se montrer, puisqu'elle n'a rien à tracer.
    pub fn in_legend(&self) -> bool {
        self.spec.show_in_legend && (self.draws() || self.broken)
    }

    /// Le texte de l'entrée de légende.
    ///
    /// Une référence brisée le dit. La résolution met déjà
    /// `« (référence rompue) »` quand la série n'a pas de nom explicite ;
    /// quand elle en a un, c'est le nom qui sort, et il faut l'annoter —
    /// sans quoi une série nommée disparaîtrait sans bruit de la figure tout
    /// en gardant une entrée de légende d'apparence normale.
    pub fn legend_text(&self) -> String {
        if !self.broken || self.label.contains("référence rompue") {
            return self.label.clone();
        }
        format!("{} — référence rompue", self.label)
    }

    /// Vrai si le texte de légende a été **saisi** (`SeriesSpec::name`) : du
    /// markup Typst voulu. Faux s'il est repris de la colonne, donc à
    /// afficher tel quel (issue #33, `formula::literal_style`).
    pub fn legend_is_typed(&self) -> bool {
        !self.spec.name.is_empty()
    }
}

/// Prépare une série résolue, sur les échelles d'axe qui la concernent.
///
/// `y_scale` est celle de **son** axe de rattachement : une série de droite
/// se prépare sur l'échelle de droite.
pub fn prepare<'s>(
    resolved: &ResolvedSeries<'s>,
    x_scale: AxisScale,
    y_scale: AxisScale,
) -> Prepared<'s> {
    let spec = resolved.spec;
    let mut prepared = Prepared {
        spec,
        label: resolved.label.clone(),
        axis: spec.axis,
        geometry: Geometry::Nothing,
        broken: resolved.broken(),
        unknown_type: matches!(spec.plot_type, PlotType::Unknown(_)),
        decimated: 0,
        stale: resolved.stale,
    };
    if prepared.broken || prepared.unknown_type {
        return prepared;
    }

    prepared.geometry = match spec.plot_type {
        PlotType::Histogram => histogram_geometry(resolved, x_scale, y_scale),
        PlotType::Bar => bars_geometry(resolved, x_scale, y_scale),
        // Les quatre autres tracent une suite de points ; ce qui les
        // distingue — trait, marqueurs, moustaches — se lit au moment de
        // peindre, pas ici, parce que la géométrie est la même.
        _ => curve_geometry(resolved, x_scale, y_scale),
    };
    prepared
}

/// Points et moustaches, en un seul parcours.
///
/// Un seul parcours et pas deux : les colonnes d'erreur sont indexées
/// **par point**, et filtrer les points non représentables d'un côté sans
/// filtrer les erreurs de l'autre décalerait les moustaches d'un cran à
/// partir du premier `NaN` — un défaut qui ne lève rien et se voit à peine.
fn curve_geometry(
    resolved: &ResolvedSeries<'_>,
    x_scale: AxisScale,
    y_scale: AxisScale,
) -> Geometry {
    let Some(ys) = resolved.y.as_ref().map(|y| y.as_slice()) else {
        return Geometry::Nothing;
    };
    let xs = resolved.x.as_ref().map(|x| x.as_slice());
    let wants_errors = matches!(resolved.spec.plot_type, PlotType::ErrorBar);
    let x_errors = wants_errors
        .then(|| resolved.x_error.as_ref().map(|e| e.as_slice()))
        .flatten();
    let y_errors = wants_errors
        .then(|| resolved.y_error.as_ref().map(|e| e.as_slice()))
        .flatten();

    let mut points = Vec::with_capacity(ys.len());
    let mut whiskers = Vec::new();

    for (index, &raw_y) in ys.iter().enumerate() {
        let raw_x = match xs {
            Some(xs) => match xs.get(index) {
                Some(x) => *x,
                None => break,
            },
            // Abscisse implicite : l'indice de ligne, comme la résolution le
            // fait déjà quand aucune colonne X n'est désignée.
            None => index as f64,
        };
        let (Some(x), Some(y)) = (forward(x_scale, raw_x), forward(y_scale, raw_y)) else {
            continue;
        };
        points.push((x, y));

        if let Some(half) = x_errors.and_then(|e| e.get(index)).copied() {
            if let Some(whisker) = whisker_along_x(raw_x, half, y, x_scale) {
                whiskers.push(whisker);
            }
        }
        if let Some(half) = y_errors.and_then(|e| e.get(index)).copied() {
            if let Some(whisker) = whisker_along_y(raw_y, half, x, y_scale) {
                whiskers.push(whisker);
            }
        }
    }

    Geometry::Curve { points, whiskers }
}

/// Moustache horizontale autour de `centre`, à ordonnée déjà transformée.
///
/// Rend `None` quand la demi-largeur n'est pas une demi-largeur (non finie,
/// négative) ou quand aucune des deux extrémités n'a d'image sur l'échelle.
/// Une seule extrémité représentable donne une demi-moustache plutôt que
/// rien : sur un axe logarithmique, c'est le cas courant près de l'origine.
fn whisker_along_x(center: f64, half: f64, y: f64, scale: AxisScale) -> Option<Whisker> {
    if !half.is_finite() || half <= 0.0 {
        return None;
    }
    let here = forward(scale, center)?;
    let low = forward(scale, center - half).unwrap_or(here);
    let high = forward(scale, center + half).unwrap_or(here);
    (low != high).then_some(((low, y), (high, y)))
}

/// Moustache verticale autour de `centre`, à abscisse déjà transformée.
fn whisker_along_y(center: f64, half: f64, x: f64, scale: AxisScale) -> Option<Whisker> {
    if !half.is_finite() || half <= 0.0 {
        return None;
    }
    let here = forward(scale, center)?;
    let low = forward(scale, center - half).unwrap_or(here);
    let high = forward(scale, center + half).unwrap_or(here);
    (low != high).then_some(((x, low), (x, high)))
}

/// Barres centrées sur les abscisses, de largeur déduite ou imposée.
///
/// La largeur vient de [`stokesforge_core::resolve::bar_width`] — du socle, et
/// non d'ici : la déduction ne doit pas donner deux largeurs différentes
/// selon le moteur qui peint, et c'est la même raison qui a fait naître
/// `PlotSpec`.
fn bars_geometry(
    resolved: &ResolvedSeries<'_>,
    x_scale: AxisScale,
    y_scale: AxisScale,
) -> Geometry {
    let Some(ys) = resolved.y.as_ref().map(|y| y.as_slice()) else {
        return Geometry::Nothing;
    };
    let xs = resolved.x.as_ref().map(|x| x.as_slice());
    let implicit: Vec<f64>;
    let xs = match xs {
        Some(xs) => xs,
        None => {
            implicit = (0..ys.len()).map(|i| i as f64).collect();
            &implicit
        }
    };
    let width = bar_width(Some(xs), resolved.spec.bar_width);
    let half = width / 2.0;

    let mut bars = Vec::with_capacity(ys.len());
    for (&raw_x, &raw_y) in xs.iter().zip(ys.iter()) {
        let (Some(left), Some(right), Some(top)) = (
            forward(x_scale, raw_x - half),
            forward(x_scale, raw_x + half),
            forward(y_scale, raw_y),
        ) else {
            continue;
        };
        bars.push((left.min(right), left.max(right), top));
    }
    Geometry::Bars(bars)
}

/// Classes et effectifs, calculés par le socle.
///
/// [`stokesforge_core::resolve::histogram`] est le port de la fonction Python et
/// vit là-bas exactement pour cette raison : deux renderers ne doivent pas
/// classer différemment. Un histogramme ne consomme **que** sa colonne Y —
/// `PlotType::needs_x` le dit déjà côté spec, et la résolution ne lui donne
/// pas d'abscisse.
fn histogram_geometry(
    resolved: &ResolvedSeries<'_>,
    x_scale: AxisScale,
    y_scale: AxisScale,
) -> Geometry {
    let Some(values) = resolved.y.as_ref().map(|y| y.as_slice()) else {
        return Geometry::Nothing;
    };
    let bins = NonZeroU32::new(resolved.spec.histogram_bins.get()).unwrap_or(NonZeroU32::MIN);
    let (edges, counts) = histogram(values, bins);
    if counts.is_empty() || edges.len() < 2 {
        return Geometry::Nothing;
    }

    let mut bars = Vec::with_capacity(counts.len());
    for (index, &count) in counts.iter().enumerate() {
        let (Some(&raw_left), Some(&raw_right)) = (edges.get(index), edges.get(index + 1)) else {
            break;
        };
        let (Some(left), Some(right), Some(top)) = (
            forward(x_scale, raw_left),
            forward(x_scale, raw_right),
            forward(y_scale, count),
        ) else {
            continue;
        };
        bars.push((left.min(right), left.max(right), top));
    }
    Geometry::Bars(bars)
}

/// Étendue d'une géométrie sur l'axe des abscisses.
pub fn x_extent(geometry: &Geometry) -> Option<(f64, f64)> {
    match geometry {
        Geometry::Nothing => None,
        Geometry::Curve { points, .. } => extent(points.iter().map(|(x, _)| *x)),
        Geometry::Bars(bars) => extent(bars.iter().flat_map(|(l, r, _)| [*l, *r])),
    }
}

/// Étendue d'une géométrie sur l'axe des ordonnées, moustaches comprises.
///
/// Les moustaches entrent dans le cadrage : une barre d'erreur qui sort de
/// la zone de tracé décrit une incertitude qu'on ne voit plus.
pub fn y_extent(geometry: &Geometry) -> Option<(f64, f64)> {
    match geometry {
        Geometry::Nothing => None,
        Geometry::Curve { points, whiskers } => extent(
            points
                .iter()
                .map(|(_, y)| *y)
                .chain(whiskers.iter().flat_map(|(a, b)| [a.1, b.1])),
        ),
        Geometry::Bars(bars) => extent(bars.iter().map(|(_, _, top)| *top)),
    }
}

fn extent(values: impl Iterator<Item = f64>) -> Option<(f64, f64)> {
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for value in values.filter(|v| v.is_finite()) {
        min = min.min(value);
        max = max.max(value);
    }
    (min.is_finite() && max.is_finite()).then_some((min, max))
}
