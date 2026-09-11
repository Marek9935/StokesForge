//! Une série : ce qu'on trace, comment on le trace, et sur quel axe.
//!
//! C'est ici que le crate touche au modèle, et d'une seule façon : par des
//! [`NodeId`]. Une série ne porte **aucune donnée** et **aucun nom de
//! colonne** — seulement l'identité des colonnes qu'elle trace.

use std::num::NonZeroU32;

use plotyx_id::NodeId;

use crate::axis::AxisSide;
use crate::style::{LineSpec, SymbolSpec};

/// Type de tracé d'une série. Jeu fermé.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PlotType {
    /// Nuage de points.
    Scatter,
    /// Courbe (défaut).
    #[default]
    Line,
    /// Courbe et marqueurs.
    LineScatter,
    /// Barres.
    Bar,
    /// Histogramme : une seule variable, répartie en classes.
    Histogram,
    /// Courbe et barres d'erreur.
    ErrorBar,
}

impl PlotType {
    /// Vrai si ce type de tracé relie les points par un trait.
    pub fn draws_line(self) -> bool {
        matches!(
            self,
            PlotType::Line | PlotType::LineScatter | PlotType::ErrorBar
        )
    }

    /// Vrai si ce type de tracé pose un marqueur sur chaque point.
    pub fn draws_symbols(self) -> bool {
        matches!(
            self,
            PlotType::Scatter | PlotType::LineScatter | PlotType::ErrorBar
        )
    }

    /// `false` pour un histogramme : il ne consomme qu'une variable.
    pub fn needs_x(self) -> bool {
        !matches!(self, PlotType::Histogram)
    }
}

/// Axe des ordonnées auquel une série est rattachée.
///
/// Deux variantes et pas quatre, là où la version Python réutilisait son
/// `AxisSide` et refusait `BOTTOM`/`TOP` à l'exécution : ici le refus n'a plus
/// lieu d'être, l'appelant n'a pas de valeur à refuser sous la main.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SeriesAxis {
    /// Ordonnée principale, à gauche (défaut).
    #[default]
    Left,
    /// Ordonnée secondaire, à droite.
    Right,
}

impl SeriesAxis {
    /// Le côté du panneau correspondant, pour aller chercher l'[`crate::AxisSpec`].
    pub fn side(self) -> AxisSide {
        match self {
            SeriesAxis::Left => AxisSide::Left,
            SeriesAxis::Right => AxisSide::Right,
        }
    }
}

impl From<SeriesAxis> for AxisSide {
    fn from(value: SeriesAxis) -> Self {
        value.side()
    }
}

/// Nombre de classes d'un histogramme par défaut.
///
/// La forme `match` est le seul moyen d'écrire un [`NonZeroU32`] littéral dans
/// un contexte constant sans passer par un `unwrap`, que le crate s'interdit.
pub const DEFAULT_HISTOGRAM_BINS: NonZeroU32 = match NonZeroU32::new(40) {
    Some(bins) => bins,
    None => NonZeroU32::MIN,
};

/// Une série : des **identités de colonnes**, un type de tracé, un style.
///
/// [`SeriesSpec::y`] est obligatoire et n'est donc pas optionnel ; `x` à
/// `None` signifie « abscisse implicite » (l'indice de la ligne), ce qui est
/// le comportement d'Origin quand aucune colonne X n'est désignée.
///
/// Les quatre références sont des [`NodeId`] et jamais des noms : renommer ou
/// déplacer une colonne laisse le graphe intact, et supprimer une colonne
/// produit une référence brisée **détectable** plutôt qu'une série
/// silencieusement vide. La détection, elle, appartient à la résolution, qui
/// est le seul endroit à voir un arbre.
#[derive(Debug, Clone, PartialEq)]
pub struct SeriesSpec {
    /// Colonne des ordonnées. Obligatoire : une série sans Y ne décrit rien.
    pub y: NodeId,
    /// Colonne des abscisses. `None` = abscisse implicite (indice de ligne).
    pub x: Option<NodeId>,
    /// Colonne des barres d'erreur en X.
    pub x_error: Option<NodeId>,
    /// Colonne des barres d'erreur en Y.
    pub y_error: Option<NodeId>,
    /// Type de tracé.
    pub plot_type: PlotType,
    /// Nom affiché en légende. Vide = dérivé de la colonne à l'affichage, donc
    /// automatiquement à jour après un renommage.
    pub name: String,
    /// Style du trait.
    pub line: LineSpec,
    /// Style des marqueurs.
    pub symbol: SymbolSpec,
    /// Ordonnée de rattachement (axe secondaire).
    pub axis: SeriesAxis,
    /// Série tracée ou non.
    pub visible: bool,
    /// Série présente en légende ou non.
    pub show_in_legend: bool,
    /// Largeur des barres. `None` = déduite du pas des abscisses.
    pub bar_width: Option<f64>,
    /// Nombre de classes d'un histogramme, jamais nul.
    pub histogram_bins: NonZeroU32,
}

impl SeriesSpec {
    /// Série au style par défaut traçant la colonne `y`.
    ///
    /// Il n'y a pas d'implémentation de `Default` : une série sans colonne Y
    /// n'existe pas, et c'est le constructeur qui le dit.
    pub fn new(y: NodeId) -> Self {
        Self {
            y,
            x: None,
            x_error: None,
            y_error: None,
            plot_type: PlotType::Line,
            name: String::new(),
            line: LineSpec::default(),
            symbol: SymbolSpec::default(),
            axis: SeriesAxis::Left,
            visible: true,
            show_in_legend: true,
            bar_width: None,
            histogram_bins: DEFAULT_HISTOGRAM_BINS,
        }
    }

    /// Série traçant `y` contre `x`, au style par défaut.
    pub fn new_xy(x: NodeId, y: NodeId) -> Self {
        Self {
            x: Some(x),
            ..Self::new(y)
        }
    }

    /// Série au style par défaut, coloriée avec la couleur `index` de la
    /// palette — trait et marqueurs ensemble, comme le fait l'UI quand on
    /// ajoute une courbe à un graphe existant.
    pub fn from_palette(y: NodeId, index: usize) -> Self {
        Self {
            line: LineSpec::from_palette(index),
            symbol: SymbolSpec::from_palette(index),
            ..Self::new(y)
        }
    }

    /// Toutes les colonnes référencées, dans l'ordre x, y, erreur X, erreur Y.
    pub fn referenced_columns(&self) -> Vec<NodeId> {
        [self.x, Some(self.y), self.x_error, self.y_error]
            .into_iter()
            .flatten()
            .collect()
    }

    /// Vrai si la série référence cette colonne, à quelque titre que ce soit.
    /// C'est la question que posera la suppression d'une colonne.
    pub fn references(&self, column: NodeId) -> bool {
        self.x == Some(column)
            || self.y == column
            || self.x_error == Some(column)
            || self.y_error == Some(column)
    }
}
