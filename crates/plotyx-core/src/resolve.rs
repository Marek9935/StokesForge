//! Résolution d'un [`PlotSpec`] contre l'arbre courant : traduit des
//! références (`NodeId`) en données prêtes à tracer.
//!
//! Port de `legacy-python/src/plotyx/plot/resolve.py`. C'est le seul module
//! du crate à connaître `plotyx-plot` — exactement comme `resolve.py` était
//! le seul module de `plotyx.plot` à connaître `plotyx.core` côté Python. La
//! frontière est la même, juste inversée : là-bas la couche neutre importait
//! le modèle pour résoudre ; ici c'est le modèle qui importe la couche
//! neutre pour porter le spec (`NodeKind::Graph`), et la résolution reste
//! l'endroit qui les met en présence.
//!
//! # Références rompues : un état de première classe, pas une exception
//!
//! Citation de la documentation de tête de `resolve.py`, portée telle quelle
//! (c'est directement la décision §11.1 d'`architecture-choisie.md`) :
//!
//! > Le parti pris de Python est d'en faire un **état de première classe**
//! > plutôt qu'une exception : `ResolvedSeries` existe toujours, avec
//! > `missing` non vide et `ok` faux.
//!
//! Rien ne plante ; la série n'est pas retirée du spec ; elle reste visible,
//! marquée comme référence rompue ([`ResolvedSeries::broken`]). Renommer ou
//! déplacer la colonne référencée ne casse rien (la référence est un
//! `NodeId`) ; la supprimer produit cet état, détectable, jamais une série
//! silencieusement vide.
//!
//! # Zéro copie
//!
//! Les colonnes du modèle sont déjà des `Arc<Vec<f64>>`
//! ([`Column::float_values`]). La résolution clone l'`Arc` (un incrément de
//! compteur de référence), jamais le buffer : [`ResolvedValues`] est une vue
//! sur la même mémoire que la colonne, éventuellement tronquée au préfixe
//! commun d'une série dont les colonnes ont des longueurs différentes.
//!
//! **Une exception, et une seule** : une colonne à types mixtes
//! ([`plotyx_core::ColumnType::Mixed`](crate::ColumnType::Mixed)) n'a pas de
//! buffer `f64` contigu — ses cellules sont un `Vec<MixedCell>` — et il n'y a
//! donc rien à partager. [`Column::numeric_view`] en matérialise un, où les
//! cellules texte et vides valent `NaN`, ce que le renderer écarte déjà :
//! une colonne mixte trace exactement ses cellules numériques. Le coût est
//! `O(n)` par résolution et **par colonne mixte référencée par une série**,
//! jamais par colonne du projet, et il ne touche aucune colonne homogène.
//!
//! # Ce qui n'est pas porté, et pourquoi
//!
//! `prepare()` et `ComputeCancelled` de `resolve.py` sont **spécifiques à
//! l'asynchrone Python** : `prepare()` enveloppe `resolve_spec` d'une
//! progression et d'une annulation coopérative pensées pour un calcul en fil
//! de fond (`plotyx.ui.plot_worker`, `QThread`). Ce mécanisme n'existe pas
//! côté Rust — le recalcul automatique n'est ni construit ni planifié
//! (`architecture-choisie.md` §11.4) — et le porter inventerait une
//! infrastructure d'annulation dont rien ne dépend encore. [`resolve_spec`]
//! couvre la partie qui reste : la résolution elle-même, qui ne parcourt
//! jamais les données (coût indépendant du nombre de points, comme en
//! Python). Le pré-calcul d'histogramme que `prepare()` intercalait est
//! disponible séparément ([`histogram`]) : un futur renderer l'appelle
//! lui-même sur les séries `Histogram` qu'il rencontre.

use std::num::NonZeroU32;
use std::sync::Arc;

use plotyx_plot::{PanelSpec, PlotSpec, SeriesSpec};

use crate::column::Column;
use crate::id::NodeId;
use crate::node::Node;
use crate::tree::Tree;

/// Rôle d'une référence de série qui n'a pas pu être résolue.
///
/// Remplace les chaînes `"x"`/`"y"`/`"xErr"`/`"yErr"` de `resolve.py` par un
/// jeu fermé — même raisonnement que partout ailleurs dans le port Rust
/// (`plotyx-plot`) : une énumération nommée plutôt qu'un vocabulaire de
/// chaînes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MissingRole {
    /// Colonne des abscisses.
    X,
    /// Colonne des ordonnées.
    Y,
    /// Colonne des barres d'erreur en X.
    XError,
    /// Colonne des barres d'erreur en Y.
    YError,
}

/// Vue zéro-copie, éventuellement tronquée, sur les valeurs `f64` résolues
/// d'une colonne.
///
/// Porte le même `Arc` que la [`Column`] d'origine : le cloner n'incrémente
/// qu'un compteur de référence, jamais les octets. La longueur portée ici
/// peut être inférieure à celle du buffer — c'est le préfixe commun d'une
/// série dont les colonnes n'ont pas la même longueur (colonnes de longueur
/// libre, décision du socle), au lieu d'un refus de tracer.
#[derive(Debug, Clone)]
pub struct ResolvedValues {
    data: Arc<Vec<f64>>,
    len: usize,
}

impl ResolvedValues {
    fn new(data: Arc<Vec<f64>>, len: usize) -> Self {
        debug_assert!(
            len <= data.len(),
            "une vue résolue ne peut pas dépasser son buffer"
        );
        Self { data, len }
    }

    /// Les valeurs, jusqu'à la troncature éventuelle.
    pub fn as_slice(&self) -> &[f64] {
        &self.data[..self.len]
    }

    /// Nombre de valeurs visibles (après troncature éventuelle).
    pub fn len(&self) -> usize {
        self.len
    }

    /// Vrai si aucune valeur n'est visible.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

/// Une série résolue et ses tableaux, ou la description de ce qui manque.
///
/// Emprunte `spec` à l'appelant plutôt que de le cloner : `SeriesSpec` ne
/// porte pas de gros buffer (ce sont des identités et du style), mais rien
/// n'oblige à en dupliquer même la partie légère quand un emprunt suffit —
/// contrairement à `resolve.py`, où le dataclass Python est de toute façon
/// un objet partagé par référence.
#[derive(Debug)]
pub struct ResolvedSeries<'s> {
    /// La série d'origine.
    pub spec: &'s SeriesSpec,
    /// Nom affiché : celui du spec s'il est fixé, sinon celui de la colonne
    /// Y, sinon `"(référence rompue)"`.
    pub label: String,
    /// Abscisses. `None` si la série n'en a pas (histogramme) ou si la
    /// référence est rompue.
    pub x: Option<ResolvedValues>,
    /// Ordonnées. `None` seulement si la référence est rompue.
    pub y: Option<ResolvedValues>,
    /// Barres d'erreur en X, si demandées et résolues.
    pub x_error: Option<ResolvedValues>,
    /// Barres d'erreur en Y, si demandées et résolues.
    pub y_error: Option<ResolvedValues>,
    /// Rôles dont la colonne référencée est introuvable. Non vide ⇔ la
    /// série est une référence brisée ([`ResolvedSeries::broken`]).
    pub missing: Vec<MissingRole>,
    /// Vrai si les colonnes résolues n'avaient pas toutes la même longueur :
    /// le tracé porte alors sur leur préfixe commun plutôt que d'être refusé.
    pub truncated: bool,
}

impl<'s> ResolvedSeries<'s> {
    /// Vrai si la série est traçable en l'état.
    pub fn ok(&self) -> bool {
        self.missing.is_empty() && self.y.as_ref().is_some_and(|y| !y.is_empty())
    }

    /// Vrai si au moins une colonne référencée est introuvable.
    pub fn broken(&self) -> bool {
        !self.missing.is_empty()
    }

    /// Nombre de points de la série (0 si la référence est rompue).
    pub fn point_count(&self) -> usize {
        self.y.as_ref().map_or(0, ResolvedValues::len)
    }
}

/// Un panneau résolu : le panneau d'origine, et ses séries résolues dans
/// l'ordre de superposition.
#[derive(Debug)]
pub struct ResolvedPanel<'s> {
    /// Le panneau d'origine.
    pub spec: &'s PanelSpec,
    /// Ses séries, résolues, dans le même ordre.
    pub series: Vec<ResolvedSeries<'s>>,
}

/// Un graphe résolu : tous ses panneaux, résolus.
#[derive(Debug)]
pub struct ResolvedPlot<'s> {
    /// Le spec d'origine.
    pub spec: &'s PlotSpec,
    /// Ses panneaux, résolus, dans le même ordre.
    pub panels: Vec<ResolvedPanel<'s>>,
}

impl<'s> ResolvedPlot<'s> {
    /// Les séries dont la référence est rompue, tous panneaux confondus.
    pub fn broken_series(&self) -> impl Iterator<Item = &ResolvedSeries<'s>> {
        self.panels
            .iter()
            .flat_map(|panel| panel.series.iter())
            .filter(|series| series.broken())
    }

    /// Nombre total de points tracés, tous panneaux et séries confondus.
    pub fn point_count(&self) -> usize {
        self.panels
            .iter()
            .flat_map(|panel| panel.series.iter())
            .map(ResolvedSeries::point_count)
            .sum()
    }
}

/// Colonne inscrite sous `id`, ou `None` si la référence est rompue.
///
/// `None` couvre deux cas volontairement indistincts pour l'appelant : nœud
/// absent de l'arbre courant (supprimé, en attente d'un `undo`), ou nœud
/// présent mais qui n'est pas une colonne. Dans les deux, il n'y a rien à
/// tracer — port de `resolve_column` (`resolve.py` ligne 68), qui traite ces
/// deux cas de la même façon côté Python (`project.registry.get` rendant soit
/// rien, soit un objet d'un autre type).
pub fn resolve_column(tree: &Tree, id: NodeId) -> Option<&Column> {
    tree.node(id).and_then(Node::as_column)
}

/// Nom affiché d'une série : `SeriesSpec::name` s'il est renseigné, sinon le
/// nom long de la colonne Y, sinon son nom de nœud, sinon « (référence
/// rompue) » si la colonne n'existe plus.
///
/// **Source unique** : la légende du graphe (via [`resolve_series`]) et la
/// liste des séries de la fenêtre des propriétés (`plotyx-app`) l'appellent
/// toutes deux, pour qu'un renommage ou un changement de règle atteigne les
/// deux (issue #25). Elle vit ici, dans `plotyx-core`, parce que c'est
/// l'endroit qui sait lire une colonne ; l'interface n'a pas à la réécrire.
pub fn series_label(series: &SeriesSpec, tree: &Tree) -> String {
    if !series.name.is_empty() {
        return series.name.clone();
    }
    let Some(y_node) = tree.node(series.y).filter(|node| node.kind().is_column()) else {
        return "(référence rompue)".to_string();
    };
    let long_name = y_node
        .as_column()
        .map(|column| column.meta().long_name.as_str())
        .unwrap_or_default();
    if long_name.is_empty() {
        y_node.name().to_string()
    } else {
        long_name.to_string()
    }
}

/// Vue `f64` d'une colonne référencée par `id`, ou `None` si la référence est
/// rompue (port de `_numeric`, `resolve.py` ligne 77). Il n'y a plus de type
/// de colonne non traçable : une colonne `Mixed` — y compris une colonne
/// entièrement textuelle — trace ses cellules numériques et vaut `NaN`
/// ailleurs (voir [`Column::numeric_view`]).
///
/// Zéro copie pour une colonne numérique ou date-heure ; une copie pour une
/// colonne **mixte**, qui n'a pas de buffer `f64` contigu à partager — voir
/// [`Column::numeric_view`], qui porte le détail et le coût de cette seule
/// exception à la propriété « zéro copie » de ce module.
fn resolve_numeric(tree: &Tree, id: NodeId) -> Option<Arc<Vec<f64>>> {
    resolve_column(tree, id).and_then(Column::numeric_view)
}

/// Résout une série. Ne lève jamais : une référence rompue est un état
/// (port de `resolve_series`, `resolve.py` ligne 165).
pub fn resolve_series<'s>(tree: &Tree, series: &'s SeriesSpec) -> ResolvedSeries<'s> {
    let label = series_label(series, tree);
    let mut missing = Vec::new();

    let y = resolve_numeric(tree, series.y);
    if y.is_none() {
        missing.push(MissingRole::Y);
    }

    let mut x = None;
    if series.plot_type.needs_x() {
        if let Some(x_id) = series.x {
            x = resolve_numeric(tree, x_id);
            if x.is_none() {
                missing.push(MissingRole::X);
            }
        }
    }

    let mut x_error = None;
    if let Some(id) = series.x_error {
        x_error = resolve_numeric(tree, id);
        if x_error.is_none() {
            missing.push(MissingRole::XError);
        }
    }

    let mut y_error = None;
    if let Some(id) = series.y_error {
        y_error = resolve_numeric(tree, id);
        if y_error.is_none() {
            missing.push(MissingRole::YError);
        }
    }

    if !missing.is_empty() {
        return ResolvedSeries {
            spec: series,
            label,
            x: None,
            y: None,
            x_error: None,
            y_error: None,
            missing,
            truncated: false,
        };
    }

    // Colonnes de longueur libre (décision du socle) : le tracé porte sur le
    // préfixe commun plutôt que d'être refusé.
    //
    // A10 (`guide-de-correction-robustesse.md` §F1) : `n` — le nombre de
    // points de la série — ne vient que des colonnes de **données** (`x`,
    // `y`), qui vont par paires et se tronquent ensemble. Les colonnes
    // d'erreur sont des **annotations par point** : elles ne participent pas
    // à ce calcul, sous peine qu'une colonne d'erreur trop courte (ou vide)
    // tronque le signal lui-même — c'est exactement l'anomalie.
    let lengths: Vec<usize> = [&x, &y]
        .into_iter()
        .flatten()
        .map(|values| values.len())
        .collect();
    let n = lengths.iter().copied().min().unwrap_or(0);
    let error_lengths: Vec<usize> = [&x_error, &y_error]
        .into_iter()
        .flatten()
        .map(|values| values.len())
        .collect();
    let truncated =
        lengths.iter().any(|&len| len != n) || error_lengths.iter().any(|&len| len != n);

    if x.is_none() && series.plot_type.needs_x() {
        // Abscisse implicite : l'indice de ligne, façon Origin quand aucune
        // colonne X n'est désignée. Construit à la volée, jamais stocké.
        x = Some(Arc::new((0..n).map(|i| i as f64).collect()));
    }

    ResolvedSeries {
        spec: series,
        label,
        x: x.map(|values| ResolvedValues::new(values, n)),
        y: y.map(|values| ResolvedValues::new(values, n)),
        x_error: x_error.map(|values| conform_error_values(values, n)),
        y_error: y_error.map(|values| conform_error_values(values, n)),
        missing,
        truncated,
    }
}

/// Conforme une colonne d'erreur (barre d'erreur en X ou en Y) à la longueur
/// `n` des données qu'elle annote (A10, `guide-de-correction-robustesse.md`
/// §F1) : tronquée si elle est plus longue, **complétée par `NaN`** — la
/// valeur manquante du modèle — si elle est plus courte, jamais l'inverse
/// (raccourcir `x`/`y` pour s'aligner sur elle).
///
/// Zéro copie dans le cas courant (assez de valeurs, ou trop) : ce n'est
/// qu'une vue tronquée sur le même `Arc`, comme [`ResolvedValues::new`]
/// partout ailleurs. Une colonne d'erreur trop courte est le cas rare ; elle
/// seule paie une copie (le buffer d'origine, complété de `NaN`), plutôt que
/// de faire payer `x`/`y` d'une troncature qui perdrait des mesures.
fn conform_error_values(values: Arc<Vec<f64>>, n: usize) -> ResolvedValues {
    if values.len() >= n {
        return ResolvedValues::new(values, n);
    }
    let mut padded = (*values).clone();
    padded.resize(n, f64::NAN);
    ResolvedValues::new(Arc::new(padded), n)
}

/// Résout toutes les séries d'un panneau, dans l'ordre.
pub fn resolve_panel<'s>(tree: &Tree, panel: &'s PanelSpec) -> ResolvedPanel<'s> {
    ResolvedPanel {
        spec: panel,
        series: panel
            .series
            .iter()
            .map(|series| resolve_series(tree, series))
            .collect(),
    }
}

/// Résout toutes les séries d'un graphe. Ne lève jamais.
///
/// Coût indépendant du nombre de points : une résolution de `NodeId` et un
/// clonage d'`Arc` par colonne référencée, jamais une copie des données
/// (port de `resolve_spec`, `resolve.py` ligne 227).
pub fn resolve_spec<'s>(tree: &Tree, spec: &'s PlotSpec) -> ResolvedPlot<'s> {
    ResolvedPlot {
        spec,
        panels: spec
            .panels
            .iter()
            .map(|panel| resolve_panel(tree, panel))
            .collect(),
    }
}

/// Largeur des barres : celle demandée si elle est positive, sinon 80 % du
/// pas médian des abscisses.
///
/// Ici, dans le socle, et pas dans un renderer : la déduction ne doit pas
/// donner deux largeurs différentes selon le moteur qui peint — c'est la
/// même raison qui a fait naître `PlotSpec` (port de `bar_width`,
/// `resolve.py` ligne 287).
///
/// `requested` est déjà un `Option<f64>` plutôt que le `float` Python
/// sentinelle-à-zéro : c'est la décision de R2 (`SeriesSpec::bar_width`,
/// `None` = déduite), donc pas de test `> 0` à faire ici pour le distinguer
/// d'une valeur explicite de zéro.
pub fn bar_width(x: Option<&[f64]>, requested: Option<f64>) -> f64 {
    // A6 — une largeur demandée absurde (négative, nulle, NaN ou infinie)
    // n'est pas une largeur : on la traite comme absente et on retombe sur
    // la déduction, plutôt que de la renvoyer telle quelle au renderer.
    if let Some(requested) = requested.filter(|w| w.is_finite() && *w > 0.0) {
        return requested;
    }
    let Some(x) = x else {
        return 1.0;
    };
    if x.len() < 2 {
        return 1.0;
    }
    let mut sorted = x.to_vec();
    sorted.sort_by(f64::total_cmp);
    let mut steps: Vec<f64> = sorted
        .windows(2)
        .map(|pair| pair[1] - pair[0])
        .filter(|step| step.is_finite() && *step > 0.0)
        .collect();
    if steps.is_empty() {
        return 1.0;
    }
    steps.sort_by(f64::total_cmp);
    median_of_sorted(&steps) * 0.8
}

/// Médiane d'une tranche déjà triée (moyenne des deux valeurs centrales si
/// la longueur est paire, comme `numpy.median`).
fn median_of_sorted(sorted: &[f64]) -> f64 {
    let n = sorted.len();
    if n % 2 == 1 {
        sorted[n / 2]
    } else {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    }
}

/// Bornes et effectifs d'un histogramme, `NaN` exclus (« finite » du code
/// Python).
///
/// Dans le socle, sans dépendance de calcul lourde (pas de `numpy` — ni
/// équivalent Rust type `ndarray` : `plotyx-core` reste sans dépendance de
/// calcul, cahier des charges de la phase R3), pour que le calcul soit
/// testable seul et partagé par tout futur renderer (port de `histogram`,
/// `resolve.py` ligne 304). `bins` est déjà un `NonZeroU32` côté
/// `SeriesSpec::histogram_bins` — pas de `max(1, …)` à refaire ici, R2 l'a
/// déjà rendu impossible d'exprimer zéro classe.
pub fn histogram(values: &[f64], bins: NonZeroU32) -> (Vec<f64>, Vec<f64>) {
    let finite: Vec<f64> = values.iter().copied().filter(|v| v.is_finite()).collect();
    if finite.is_empty() {
        return (vec![0.0], Vec::new());
    }
    let bins = bins.get() as usize;

    let mut min = finite[0];
    let mut max = finite[0];
    for &v in &finite[1..] {
        min = min.min(v);
        max = max.max(v);
    }
    // `numpy.histogram` élargit un intervalle dégénéré de 0,5 de chaque
    // côté plutôt que de diviser par une largeur nulle.
    let (first_edge, last_edge) = if min == max {
        (min - 0.5, max + 0.5)
    } else {
        (min, max)
    };

    // A5 (`guide-de-correction-robustesse.md`, « Une échéance, pas une
    // cause ») — `last_edge - first_edge` peut déborder `f64` (deux valeurs
    // proches de `±f64::MAX`), ce qui rendait `width = +inf`, puis des
    // bornes `NaN` (`inf * 0.0`) et un comptage qui rangeait tout en classe
    // 0. On évite tout intermédiaire non fini :
    //
    // - les bornes se calculent par combinaison convexe
    //   `first*(1-t) + last*t`, qui reste toujours dans `[first, last]`
    //   (donc finie) quels que soient `first`/`last`, sans jamais passer par
    //   une différence qui pourrait déborder ;
    // - la position d'une valeur dans l'histogramme se calcule via
    //   `half_width = last/2 - first/2` — la moitié de la largeur, mais
    //   calculée en divisant d'abord chaque borne par deux, sans jamais
    //   déborder — puis `t = (v/2 - first/2) / half_width`, qui rend le
    //   même résultat que `(v - first) / width` sans jamais matérialiser une
    //   différence non finie.
    let half_width = last_edge / 2.0 - first_edge / 2.0;

    let edges: Vec<f64> = (0..=bins)
        .map(|i| {
            let t = (i as f64) / (bins as f64);
            first_edge * (1.0 - t) + last_edge * t
        })
        .collect();

    let mut counts = vec![0.0f64; bins];
    for &v in &finite {
        let t = (v / 2.0 - first_edge / 2.0) / half_width;
        let position = t * bins as f64;
        let index = (position.floor() as isize).clamp(0, bins as isize - 1) as usize;
        counts[index] += 1.0;
    }

    (edges, counts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::column::{Column, ColumnMeta, ColumnType, ValueBlock};
    use crate::node::WindowMeta;
    use crate::project::Project;
    use plotyx_plot::{PanelSpec, PlotType, SeriesSpec};

    fn numeric_column(values: &[f64]) -> Column {
        Column::from_block(
            ColumnType::Numeric,
            ValueBlock::Float(values.to_vec()),
            ColumnMeta::default(),
        )
        .expect("colonne numérique valide")
    }

    /// Fixture : un projet avec une feuille contenant les colonnes `x`, `y`
    /// (deux points) et `texte` (colonne mixte entièrement textuelle, sans
    /// contenu numérique), plus un graphe.
    struct Fixture {
        project: Project,
        x: NodeId,
        y: NodeId,
        texte: NodeId,
        graph: NodeId,
    }

    impl Fixture {
        fn new() -> Self {
            let mut project = Project::new("Demo").expect("projet valide");
            let root = project.root();
            let book = project.add_empty_workbook(root, None).expect("classeur");
            let sheet = project.add_worksheet(book, None).expect("feuille");
            let x = project
                .add_column(sheet, None, Some("X"), numeric_column(&[0.0, 1.0, 2.0]))
                .expect("colonne X");
            let y = project
                .add_column(sheet, None, Some("Y"), numeric_column(&[10.0, 20.0, 30.0]))
                .expect("colonne Y");
            let texte = project
                .add_column(
                    sheet,
                    None,
                    Some("T"),
                    Column::from_text(&[Some("a".to_string()), Some("b".to_string())]),
                )
                .expect("colonne texte");
            let graph = project
                .add_graph(root, None, PlotSpec::default())
                .expect("graphe");
            Self {
                project,
                x,
                y,
                texte,
                graph,
            }
        }
    }

    #[test]
    fn a_fully_resolved_series_carries_zero_copy_views() {
        let f = Fixture::new();
        let series = SeriesSpec::new_xy(f.x, f.y);
        let resolved = resolve_series(f.project.tree(), &series);

        assert!(resolved.ok());
        assert!(!resolved.broken());
        assert_eq!(resolved.missing, Vec::new());
        assert_eq!(resolved.point_count(), 3);
        assert_eq!(resolved.x.unwrap().as_slice(), &[0.0, 1.0, 2.0]);
        assert_eq!(resolved.y.unwrap().as_slice(), &[10.0, 20.0, 30.0]);
    }

    /// Tâche 3.6, premier volet : renommer une colonne référencée par un
    /// graphe laisse le graphe résoluble — la référence est un `NodeId`, le
    /// renommage ne le touche pas (décision §11.1 d'`architecture-choisie.md`).
    #[test]
    fn renaming_the_referenced_column_leaves_the_series_resolvable() {
        let f = Fixture::new();
        let mut project = f.project;
        let series = SeriesSpec::new_xy(f.x, f.y);

        let before = resolve_series(project.tree(), &series);
        assert!(before.ok());

        project.rename(f.y, "Tension").expect("renommage valide");

        let after = resolve_series(project.tree(), &series);
        assert!(after.ok(), "le renommage ne doit pas casser la résolution");
        assert!(!after.broken());
        assert_eq!(after.y.unwrap().as_slice(), &[10.0, 20.0, 30.0]);
        // Le nom affiché suit le renommage : la colonne s'appelle maintenant
        // « Tension », et rien dans le spec ne fige l'ancien nom « Y ».
        assert_eq!(
            series_label(&series, project.tree()),
            "Tension",
            "le nom affiché doit suivre le renommage de la colonne"
        );
    }

    /// Tâche 3.6, second volet : supprimer la colonne référencée produit une
    /// référence brisée **explicite** dans le résultat de résolution — jamais
    /// une série silencieusement vide. C'est `ResolvedSeries::ok() == false`
    /// avec `missing` non vide, pas un panique ni une série retirée du spec.
    #[test]
    fn a_broken_reference_is_a_state_not_a_panic() {
        let f = Fixture::new();
        let mut project = f.project;
        project.remove(f.y).expect("suppression de la colonne Y");
        let series = SeriesSpec::new_xy(f.x, f.y);

        let resolved = resolve_series(project.tree(), &series);

        assert!(!resolved.ok());
        assert!(resolved.broken());
        assert_eq!(resolved.missing, vec![MissingRole::Y]);
        assert!(resolved.y.is_none());
        assert!(
            resolved.x.is_none(),
            "toute la série est marquée, pas juste Y"
        );
        assert_eq!(resolved.label, "(référence rompue)");
    }

    /// Il n'y a plus de type de colonne non traçable (`Text` a disparu au
    /// profit de `Mixed`) : une colonne entièrement textuelle résout sa
    /// série normalement, simplement sans le moindre point numérique — ce
    /// n'est pas une référence rompue.
    #[test]
    fn a_fully_textual_column_resolves_with_no_numeric_points() {
        let f = Fixture::new();
        let series = SeriesSpec::new(f.texte);
        let resolved = resolve_series(f.project.tree(), &series);
        assert!(
            resolved.ok(),
            "une colonne mixte n'est jamais une référence rompue"
        );
        assert!(resolved.missing.is_empty());
        let slice = resolved.y.expect("ordonnées résolues");
        assert!(slice.as_slice().iter().all(|value| value.is_nan()));
    }

    /// Une colonne **mixte** est traçable, contrairement à une colonne texte :
    /// la série se résout, et seules les cellules numériques portent une
    /// valeur — les autres valent `NaN`, ce que le renderer écarte déjà.
    #[test]
    fn a_mixed_column_resolves_and_only_its_numbers_carry_a_value() {
        use crate::column::MixedCell;

        let mut project = Project::new("Demo").expect("projet valide");
        let root = project.root();
        let book = project.add_empty_workbook(root, None).expect("classeur");
        let sheet = project.add_worksheet(book, None).expect("feuille");
        let y = project
            .add_column(
                sheet,
                None,
                Some("Y"),
                Column::from_mixed(&[
                    MixedCell::Number(1.2),
                    MixedCell::Text("n/a".to_string()),
                    MixedCell::Empty,
                    MixedCell::Number(1.5),
                ]),
            )
            .expect("colonne mixte");

        let series = SeriesSpec::new(y);
        let resolved = resolve_series(project.tree(), &series);

        assert!(
            resolved.ok(),
            "une colonne mixte n'est pas une référence rompue"
        );
        assert!(!resolved.broken());
        assert_eq!(resolved.point_count(), 4);
        let values = resolved.y.expect("ordonnées résolues");
        let slice = values.as_slice();
        assert_eq!(slice[0], 1.2);
        assert!(slice[1].is_nan(), "la cellule texte ne se trace pas");
        assert!(slice[2].is_nan(), "la cellule vide ne se trace pas");
        assert_eq!(slice[3], 1.5);
    }

    #[test]
    fn no_x_column_means_the_implicit_row_index() {
        let f = Fixture::new();
        let series = SeriesSpec::new(f.y);
        let resolved = resolve_series(f.project.tree(), &series);
        assert!(resolved.ok());
        assert_eq!(resolved.x.unwrap().as_slice(), &[0.0, 1.0, 2.0]);
    }

    #[test]
    fn a_histogram_series_never_asks_for_x() {
        let f = Fixture::new();
        let series = SeriesSpec {
            plot_type: PlotType::Histogram,
            ..SeriesSpec::new(f.y)
        };
        let resolved = resolve_series(f.project.tree(), &series);
        assert!(resolved.ok());
        assert!(resolved.x.is_none());
    }

    #[test]
    fn mismatched_lengths_truncate_to_the_common_prefix() {
        let mut project = Project::new("Demo").expect("projet valide");
        let root = project.root();
        let book = project.add_empty_workbook(root, None).expect("classeur");
        let sheet = project.add_worksheet(book, None).expect("feuille");
        let x = project
            .add_column(
                sheet,
                None,
                Some("X"),
                numeric_column(&[0.0, 1.0, 2.0, 3.0]),
            )
            .expect("colonne X");
        let y = project
            .add_column(sheet, None, Some("Y"), numeric_column(&[10.0, 20.0]))
            .expect("colonne Y, plus courte");

        let series = SeriesSpec::new_xy(x, y);
        let resolved = resolve_series(project.tree(), &series);

        assert!(resolved.ok());
        assert!(resolved.truncated);
        assert_eq!(resolved.x.unwrap().as_slice(), &[0.0, 1.0]);
        assert_eq!(resolved.y.unwrap().as_slice(), &[10.0, 20.0]);
    }

    #[test]
    fn explicit_name_wins_over_the_column_name() {
        let f = Fixture::new();
        let series = SeriesSpec {
            name: "Ma série".to_string(),
            ..SeriesSpec::new(f.y)
        };
        assert_eq!(series_label(&series, f.project.tree()), "Ma série");
    }

    #[test]
    fn empty_name_falls_back_to_the_long_name_then_the_column_name() {
        let f = Fixture::new();
        let series = SeriesSpec::new(f.y);
        assert_eq!(series_label(&series, f.project.tree()), "Y");

        let mut project = f.project;
        project
            .set_column_meta(
                f.y,
                ColumnMeta {
                    long_name: "Tension".to_string(),
                    ..ColumnMeta::default()
                },
            )
            .expect("métadonnées valides");
        assert_eq!(series_label(&series, project.tree()), "Tension");
    }

    #[test]
    fn resolve_spec_walks_every_panel_and_series() {
        let f = Fixture::new();
        let mut project = f.project;
        // Une référence rompue, pour la seconde série : supprimer la colonne
        // qu'elle visait plutôt que passer par une colonne texte, qui résout
        // désormais toujours (voir `a_fully_textual_column_resolves_with_no_numeric_points`).
        project
            .remove(f.texte)
            .expect("suppression de la colonne texte");
        let spec = PlotSpec {
            panels: vec![
                PanelSpec {
                    series: vec![SeriesSpec::new_xy(f.x, f.y)],
                    ..PanelSpec::at(0, 0)
                },
                PanelSpec {
                    series: vec![SeriesSpec::new(f.texte)],
                    ..PanelSpec::at(1, 0)
                },
            ]
            .into(),
            ..PlotSpec::empty("Deux panneaux")
        };

        let resolved = resolve_spec(project.tree(), &spec);

        assert_eq!(resolved.panels.len(), 2);
        assert_eq!(resolved.point_count(), 3);
        assert_eq!(resolved.broken_series().count(), 1);
    }

    #[test]
    fn bar_width_prefers_the_requested_value() {
        assert_eq!(bar_width(Some(&[0.0, 1.0, 2.0]), Some(0.3)), 0.3);
    }

    #[test]
    fn bar_width_without_x_or_a_single_point_defaults_to_one() {
        assert_eq!(bar_width(None, None), 1.0);
        assert_eq!(bar_width(Some(&[1.0]), None), 1.0);
    }

    #[test]
    fn bar_width_deduces_eighty_percent_of_the_median_step() {
        // Pas constant de 2 → largeur déduite 1.6.
        assert_eq!(bar_width(Some(&[0.0, 2.0, 4.0, 6.0]), None), 1.6);
    }

    #[test]
    fn bar_width_ignores_non_finite_and_non_positive_steps() {
        // Un doublon produit un pas nul, exclu ; NaN aussi.
        assert_eq!(bar_width(Some(&[0.0, 0.0, 1.0, f64::NAN]), None), 0.8);
    }

    #[test]
    fn histogram_of_an_all_nan_slice_is_the_empty_case() {
        let (edges, counts) = histogram(&[f64::NAN, f64::NAN], NonZeroU32::new(10).unwrap());
        assert_eq!(edges, vec![0.0]);
        assert!(counts.is_empty());
    }

    #[test]
    fn histogram_excludes_non_finite_values() {
        let values = [0.0, 1.0, 2.0, 3.0, f64::NAN, f64::INFINITY];
        let (edges, counts) = histogram(&values, NonZeroU32::new(4).unwrap());
        assert_eq!(edges.len(), 5);
        assert_eq!(edges.first().copied(), Some(0.0));
        assert_eq!(edges.last().copied(), Some(3.0));
        let total: f64 = counts.iter().sum();
        assert_eq!(total, 4.0, "les deux valeurs non finies sont exclues");
    }

    #[test]
    fn histogram_of_a_single_repeated_value_widens_the_degenerate_range() {
        let (edges, counts) = histogram(&[5.0, 5.0, 5.0], NonZeroU32::new(2).unwrap());
        assert_eq!(edges, vec![4.5, 5.0, 5.5]);
        // Toutes les valeurs tombent dans le bin de droite avec cette
        // implémentation (bord droit inclusif) : le compte total est
        // préservé, ce qui est la propriété qui compte.
        let total: f64 = counts.iter().sum();
        assert_eq!(total, 3.0);
    }

    #[test]
    fn window_meta_default_is_used_for_fixture_windows() {
        // Sanity check du fixture lui-même : le graphe existe et sa fenêtre
        // est par défaut.
        let f = Fixture::new();
        let node = f.project.tree().get(f.graph).expect("graphe présent");
        assert!(matches!(
            node.kind(),
            crate::node::NodeKind::Graph { meta, .. } if *meta == WindowMeta::default()
        ));
    }
}
