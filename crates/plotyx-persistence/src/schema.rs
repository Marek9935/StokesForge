//! Structures de transport (« schéma sur disque »), privées au crate.
//!
//! Ce module ne connaît **pas** `plotyx-core` au sens inverse : c'est
//! `convert.rs` qui sait construire un `NodeRecord` depuis un `Node` et
//! réciproquement, jamais l'inverse. Le schéma est volontairement
//! indépendant du modèle en mémoire (voir la doc de crate) : il peut évoluer
//! (ajout de champs `#[serde(default)]`) sans jamais dériver `Serialize` sur
//! les types de `plotyx-core`.
//!
//! Toutes ces structures sont sérialisées **comme des maps nommées**
//! (`rmp_serde::to_vec_named`), ce qui rend le fichier auto-descriptif : un
//! champ absent à la lecture prend sa valeur par défaut (`#[serde(default)]`,
//! à ajouter sur tout champ créé après la version 1) et un champ inconnu à la
//! lecture est silencieusement ignoré — c'est le mécanisme qui permettra
//! d'ajouter des données (par exemple une spécification de graphe) sans
//! casser les fichiers déjà écrits.

use serde::{Deserialize, Serialize};

/// Signature de fichier : les quatre premiers octets, avant même le numéro
/// de version.
pub(crate) const MAGIC: &[u8; 4] = b"PLYX";

/// Numéro de version du format que ce lecteur/écrivain sait produire et
/// comprendre. Un fichier dont le numéro de version est supérieur est
/// refusé sans que son corps soit lu (`PersistenceError::UnsupportedVersion`).
pub(crate) const FORMAT_VERSION: u32 = 1;

/// Corps du fichier (après l'en-tête magique + version) : un `Project`
/// complet, à plat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct FileBody {
    /// Identifiant du nœud racine.
    pub(crate) root: u64,
    /// Valeur persistée du compteur d'identifiants (`Project::next_id_hint`).
    pub(crate) next_id_hint: u64,
    /// Tous les nœuds de l'arbre, en parcours préfixe depuis la racine — cet
    /// ordre est ce qui rend deux sauvegardes du même projet identiques
    /// octet à octet.
    pub(crate) nodes: Vec<NodeRecord>,
}

/// Un nœud de l'arbre, à plat (l'appartenance aux enfants d'un parent est
/// reconstruite par `Tree::from_nodes`, pas portée deux fois ici — sauf pour
/// les conteneurs, où l'ordre des enfants doit être persisté explicitement).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct NodeRecord {
    /// Identifiant du nœud.
    pub(crate) id: u64,
    /// Nom court.
    pub(crate) name: String,
    /// Identifiant du parent, `None` pour la racine.
    pub(crate) parent: Option<u64>,
    /// Nature et contenu propre.
    pub(crate) kind: KindRecord,
}

/// Nature d'un nœud, à plat. Enum étiqueté « externally tagged » (le
/// comportement par défaut de `serde`) : chaque valeur se sérialise comme
/// une map à une seule clé (le nom de la variante) dont la valeur est le
/// contenu — c'est ce qui garde le fichier auto-descriptif jusque dans ses
/// enums.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) enum KindRecord {
    /// Dossier.
    Folder {
        /// Enfants ordonnés.
        children: Vec<u64>,
    },
    /// Classeur.
    Workbook {
        /// Feuilles, dans l'ordre des onglets.
        sheets: Vec<u64>,
        /// Étiquette de fenêtre.
        label: String,
        /// Commentaire de fenêtre.
        comment: String,
    },
    /// Feuille de calcul.
    Worksheet {
        /// Colonnes, de gauche à droite.
        columns: Vec<u64>,
    },
    /// Colonne de données.
    Column {
        /// Type de données.
        column_type: TypeRecord,
        /// Rôle de tracé.
        role: RoleRecord,
        /// Unité affichée.
        unit: String,
        /// Nom long / description.
        long_name: String,
        /// Commentaire.
        comment: String,
        /// Format d'affichage.
        display_format: String,
        /// Valeurs de la colonne.
        values: ValuesRecord,
        /// Compteur de modification des données (`Column::version`). Absent
        /// des fichiers écrits avant le 2026-09-10 : `#[serde(default)]` le
        /// ramène alors à 0, ce qui est le comportement voulu — un fichier
        /// sans compteur n'a de toute façon aucun lecteur qui en retienne un.
        #[serde(default)]
        version: u64,
    },
    /// Graphe.
    Graph {
        /// Étiquette de fenêtre.
        label: String,
        /// Commentaire de fenêtre.
        comment: String,
        /// Description du graphe (`PlotSpec`). Absente des fichiers écrits
        /// avant la phase R3 : `#[serde(default)]` la ramène alors à
        /// [`PlotSpecRecord::default`], qui convertit vers le
        /// `PlotSpec::default()` de `plotyx-plot` (un panneau, aucune
        /// série) — exactement ce qu'un fichier sans spec décrivait déjà en
        /// mémoire avant cette phase.
        #[serde(default)]
        spec: PlotSpecRecord,
    },
    /// Matrice.
    Matrix {
        /// Étiquette de fenêtre.
        label: String,
        /// Commentaire de fenêtre.
        comment: String,
    },
    /// Note.
    Note {
        /// Étiquette de fenêtre.
        label: String,
        /// Commentaire de fenêtre.
        comment: String,
        /// Texte de la note.
        text: String,
    },
}

/// Type de données d'une colonne, à plat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum TypeRecord {
    /// Numérique.
    Numeric,
    /// Date-heure.
    DateTime,
    /// Texte.
    Text,
}

/// Rôle de tracé d'une colonne, à plat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum RoleRecord {
    /// Abscisse.
    X,
    /// Ordonnée.
    Y,
    /// Troisième axe.
    Z,
    /// Barre d'erreur en X.
    XError,
    /// Barre d'erreur en Y.
    YError,
    /// Étiquette textuelle.
    Label,
    /// Aucun rôle de tracé.
    None,
}

/// Valeurs d'une colonne, à plat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) enum ValuesRecord {
    /// `f64` little-endian concaténés (8 octets chacun, `NaN` préservés bit à
    /// bit) : un octet brut par octet de donnée, jamais un tableau
    /// MessagePack d'éléments — c'est ce qui garde la taille du fichier
    /// proportionnelle aux données plutôt qu'à leur encodage.
    Float(#[serde(with = "serde_bytes")] Vec<u8>),
    /// Chaînes optionnelles, `None` = valeur manquante (distinct de
    /// `Some(String::new())`).
    Text(Vec<Option<String>>),
}

// -- PlotSpec (phase R3) ------------------------------------------------
//
// Même patron que `Column`/`ColumnValues`/`ValuesRecord` ci-dessus, étendu à
// un arbre de structures : un enregistrement par type de `plotyx-plot`,
// converti à la main par `convert.rs`, jamais dérivé du modèle. Les `NodeId`
// référencés par une série voyagent en `u64`, comme ceux des listes
// d'enfants de `Folder`/`Workbook`/`Worksheet` plus haut — même
// représentation, réutilisée plutôt que réinventée.
//
// Chaque `Default` ci-dessous reproduit *à la main* celui du type
// `plotyx-plot` correspondant : ce module ne connaît toujours pas
// `plotyx-plot` (voir la doc de tête du fichier), donc pas de raccourci du
// genre `PlotSpecRecord::from(PlotSpec::default())`. Un test de
// `convert.rs` verrouille que les deux mondes restent d'accord.

/// [`plotyx_plot::PlotSpec`], à plat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct PlotSpecRecord {
    pub(crate) title: String,
    pub(crate) panels: Vec<PanelSpecRecord>,
    pub(crate) background: String,
    pub(crate) format_version: u32,
}

impl Default for PlotSpecRecord {
    fn default() -> Self {
        Self {
            title: String::new(),
            panels: vec![PanelSpecRecord::default()],
            background: String::new(),
            format_version: 1, // plotyx_plot::SPEC_FORMAT_VERSION
        }
    }
}

/// [`plotyx_plot::PanelSpec`], à plat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct PanelSpecRecord {
    pub(crate) title: String,
    pub(crate) series: Vec<SeriesSpecRecord>,
    pub(crate) bottom: AxisSpecRecord,
    pub(crate) left: AxisSpecRecord,
    pub(crate) right: AxisSpecRecord,
    pub(crate) top: AxisSpecRecord,
    pub(crate) legend: LegendSpecRecord,
    pub(crate) row: u64,
    pub(crate) column: u64,
}

impl Default for PanelSpecRecord {
    fn default() -> Self {
        Self {
            title: String::new(),
            series: Vec::new(),
            bottom: AxisSpecRecord::default(),
            left: AxisSpecRecord::default(),
            right: AxisSpecRecord::default(),
            top: AxisSpecRecord::hidden(),
            legend: LegendSpecRecord::default(),
            row: 0,
            column: 0,
        }
    }
}

/// [`plotyx_plot::AxisSpec`], à plat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct AxisSpecRecord {
    pub(crate) label: String,
    pub(crate) unit: String,
    pub(crate) scale: AxisScaleRecord,
    pub(crate) auto_range: bool,
    pub(crate) range_min: Option<f64>,
    pub(crate) range_max: Option<f64>,
    pub(crate) grid: bool,
    pub(crate) visible: bool,
    pub(crate) inverted: bool,
}

impl AxisSpecRecord {
    /// Axe masqué — la valeur par défaut de l'axe du haut d'un panneau
    /// (`AxisSpec::hidden` côté `plotyx-plot`).
    fn hidden() -> Self {
        Self {
            visible: false,
            ..Self::default()
        }
    }
}

impl Default for AxisSpecRecord {
    fn default() -> Self {
        Self {
            label: String::new(),
            unit: String::new(),
            scale: AxisScaleRecord::Linear,
            auto_range: true,
            range_min: None,
            range_max: None,
            grid: false,
            visible: true,
            inverted: false,
        }
    }
}

/// [`plotyx_plot::AxisScale`], à plat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum AxisScaleRecord {
    Linear,
    Log,
}

/// [`plotyx_plot::SeriesSpec`], à plat. Les colonnes référencées voyagent en
/// `u64` brut, comme partout ailleurs dans ce schéma — un `NodeId` que la
/// colonne visée ait ou non survécu au fichier : une référence pendante
/// n'est pas une erreur de chargement, elle se découvre à la résolution
/// (`plotyx_core::resolve`), jamais avant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct SeriesSpecRecord {
    pub(crate) y: u64,
    pub(crate) x: Option<u64>,
    pub(crate) x_error: Option<u64>,
    pub(crate) y_error: Option<u64>,
    pub(crate) plot_type: PlotTypeRecord,
    pub(crate) name: String,
    pub(crate) line: LineSpecRecord,
    pub(crate) symbol: SymbolSpecRecord,
    pub(crate) axis: SeriesAxisRecord,
    pub(crate) visible: bool,
    pub(crate) show_in_legend: bool,
    pub(crate) bar_width: Option<f64>,
    pub(crate) histogram_bins: u32,
}

/// [`plotyx_plot::PlotType`], à plat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum PlotTypeRecord {
    Scatter,
    Line,
    LineScatter,
    Bar,
    Histogram,
    ErrorBar,
}

/// [`plotyx_plot::SeriesAxis`], à plat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum SeriesAxisRecord {
    Left,
    Right,
}

/// [`plotyx_plot::LineSpec`], à plat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct LineSpecRecord {
    pub(crate) color: String,
    pub(crate) style: LineStyleRecord,
    pub(crate) width: f64,
}

/// [`plotyx_plot::LineStyle`], à plat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum LineStyleRecord {
    None,
    Solid,
    Dash,
    Dot,
    DashDot,
}

/// [`plotyx_plot::SymbolSpec`], à plat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct SymbolSpecRecord {
    pub(crate) shape: SymbolShapeRecord,
    pub(crate) size: f64,
    pub(crate) color: String,
    pub(crate) edge_color: String,
}

/// [`plotyx_plot::SymbolShape`], à plat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum SymbolShapeRecord {
    None,
    Circle,
    Square,
    Triangle,
    Diamond,
    Plus,
    Cross,
}

/// [`plotyx_plot::LegendSpec`], à plat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct LegendSpecRecord {
    pub(crate) visible: bool,
    pub(crate) position: LegendPositionRecord,
}

impl Default for LegendSpecRecord {
    fn default() -> Self {
        // `LegendSpec::default()` : visible, ancrée en haut à droite — pas de
        // `#[derive(Default)]` ici précisément parce que `bool::default()`
        // (`false`) contredirait cette valeur par défaut.
        Self {
            visible: true,
            position: LegendPositionRecord::default(),
        }
    }
}

/// [`plotyx_plot::LegendPosition`], à plat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub(crate) enum LegendPositionRecord {
    TopLeft,
    #[default]
    TopRight,
    BottomLeft,
    BottomRight,
}
