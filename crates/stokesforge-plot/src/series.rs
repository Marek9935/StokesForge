//! Une série : ce qu'on trace, comment on le trace, et sur quel axe.
//!
//! C'est ici que le crate touche au modèle, et d'une seule façon : par des
//! [`NodeId`]. Une série ne porte **aucune donnée** et **aucun nom de
//! colonne** — seulement l'identité des colonnes qu'elle trace.

use std::num::NonZeroU32;

use stokesforge_id::NodeId;

use crate::axis::AxisSide;
use crate::raw::RawVariant;
use crate::style::{LineSpec, SymbolSpec};

/// Type de tracé d'une série. Jeu fermé — sauf pour [`PlotType::Unknown`],
/// qui porte le type de tracé d'une version plus récente de StokesForge que ce
/// lecteur ne connaît pas (C5, niveau 2,
/// `guide-de-correction-robustesse.md` §F3) : une série ainsi marquée ne
/// dessine rien et ne bloque rien, mais son contenu original est conservé
/// pour une ré-écriture fidèle.
///
/// Cette variante retire `Copy`/`Eq`/`Hash` du jeu de dérivations : un
/// `RawVariant` porte une chaîne et des octets, ni l'un ni l'autre `Copy`,
/// et l'égalité/le hachage d'un contenu inconnu n'a pas d'utilité ici.
#[derive(Debug, Clone, PartialEq, Default)]
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
    /// Type de tracé d'une version plus récente de StokesForge, inconnu de ce
    /// lecteur.
    Unknown(RawVariant),
}

impl PlotType {
    /// Vrai si ce type de tracé relie les points par un trait. Toujours
    /// faux pour [`PlotType::Unknown`] : sans savoir ce que c'est, le plus
    /// sûr est de ne rien dessiner.
    pub fn draws_line(&self) -> bool {
        matches!(
            self,
            PlotType::Line | PlotType::LineScatter | PlotType::ErrorBar
        )
    }

    /// Vrai si ce type de tracé pose un marqueur sur chaque point. Toujours
    /// faux pour [`PlotType::Unknown`], même raison que `draws_line`.
    pub fn draws_symbols(&self) -> bool {
        matches!(
            self,
            PlotType::Scatter | PlotType::LineScatter | PlotType::ErrorBar
        )
    }

    /// `false` pour un histogramme : il ne consomme qu'une variable. Vrai
    /// pour [`PlotType::Unknown`] : à défaut de savoir, autant garder
    /// l'abscisse plutôt que la perdre.
    pub fn needs_x(&self) -> bool {
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

/// Plafond de [`BinCount`] (B4, `guide-de-correction-robustesse.md` §F4) :
/// au-delà, un histogramme ne décrit plus rien qu'un écran puisse afficher
/// (quelques milliers de pixels de large au plus, dix fois moins que ce
/// plafond), et le calculer allouerait des dizaines de gigaoctets pour des
/// classes qu'aucun rendu ne montrera jamais.
pub const MAX_HISTOGRAM_BINS: u32 = 1 << 16;

/// Nombre de classes d'un histogramme : jamais nul (décision R2 — « les
/// états illégaux ne s'écrivent pas »), jamais démesuré non plus (B4).
/// `NonZeroU32` rendait déjà le zéro inexprimable ; ce type resserre l'autre
/// bout du même intervalle, à la même échelle — au niveau du type, pas
/// d'une validation qu'un appelant pourrait oublier d'appeler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BinCount(NonZeroU32);

impl BinCount {
    /// `None` si `value` vaut 0 ou dépasse [`MAX_HISTOGRAM_BINS`].
    pub fn new(value: u32) -> Option<Self> {
        if value == 0 || value > MAX_HISTOGRAM_BINS {
            None
        } else {
            NonZeroU32::new(value).map(Self)
        }
    }

    /// La valeur portée.
    pub fn get(self) -> u32 {
        self.0.get()
    }
}

impl std::fmt::Display for BinCount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}

/// Nombre de classes d'un histogramme par défaut.
///
/// La forme `match` est le seul moyen d'écrire un [`NonZeroU32`] littéral dans
/// un contexte constant sans passer par un `unwrap`, que le crate s'interdit.
pub const DEFAULT_HISTOGRAM_BINS: BinCount = match NonZeroU32::new(40) {
    Some(bins) => BinCount(bins),
    None => BinCount(NonZeroU32::MIN),
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
    /// Nombre de classes d'un histogramme, jamais nul, jamais démesuré (B4).
    pub histogram_bins: BinCount,
    /// Ligne de la feuille de `y` tracée **en série** (#166). Présente, la
    /// série n'est plus « une colonne » mais « une ligne » : ses points sont
    /// les valeurs de cette ligne (0 = la première) dans les colonnes de la
    /// feuille qui portent une valeur x', placées à l'abscisse x' de leur
    /// colonne (cinétique : une colonne = un instant). `y` ne sert alors
    /// qu'à désigner la feuille (une de ses colonnes) et `x` est ignoré.
    pub row: Option<usize>,
    /// Groupe de courbes du panneau (#177) : les séries qui portent le même
    /// numéro se règlent ensemble (style commun, nuancier). `None` : courbe
    /// seule. Le numéro n'a de sens que dans son panneau ; voir
    /// [`crate::PanelSpec::push_series_batch`].
    pub group: Option<u32>,
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
            row: None,
            group: None,
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
