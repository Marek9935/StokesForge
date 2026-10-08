//! Colonne du modèle de données : un vecteur de valeurs nommé (le nom vit
//! dans le nœud de l'arbre, pas ici) de longueur libre, plus ses métadonnées
//! d'affichage et de tracé.
//!
//! Trois décisions d'architecture structurent ce module : les valeurs
//! manquantes sont `f64::NAN` pour les colonnes numériques/date-heure et
//! `None` pour les colonnes texte (distinct de `Some(String::new())`) ; les
//! buffers sont des `Vec` contigus (jamais chunkés) partagés derrière un
//! `Arc`, mutés via `Arc::make_mut` pour permettre au pont Python de tenir
//! une vue numpy sans copie tant qu'il détient un clone de l'`Arc` ; et
//! chaque colonne porte un **compteur de modification** ([`Column::version`])
//! qui permet de savoir qu'elle a changé sans relire ses valeurs.
//!
//! # Colonnes à types mixtes
//!
//! [`ColumnType::Mixed`] est le quatrième type, et le seul dont le type
//! **n'est pas celui de la colonne mais celui de la cellule** : son buffer
//! est un `Vec<`[`MixedCell`]`>`, où chaque ligne est indépendamment vide, un
//! nombre ou un texte. C'est ce qui permet à une colonne d'un import réel de
//! porter `1,2  1,3  n/a  1,5` sans choisir entre « tout en nombres » (le
//! `n/a` devient une valeur manquante et l'information est perdue) et « tout
//! en texte » (plus rien n'est traçable).
//!
//! Trois décisions à connaître avant d'y toucher :
//!
//! - **Les trois types homogènes restent des chemins rapides, pas des cas
//!   particuliers d'un cas général.** Faire de *toute* colonne un
//!   `Vec<MixedCell>` coûterait 32 octets par ligne au lieu de 8 (la taille
//!   d'un `MixedCell` est celle d'une `String` plus son discriminant) et,
//!   surtout, retirerait le buffer `Arc<Vec<f64>>` contigu sur lequel
//!   reposent la vue numpy sans copie du pont Python et la résolution
//!   zéro-copie d'une série (`crate::resolve`). `Mixed` est donc un type de
//!   plus, que l'on paie quand on s'en sert, jamais une refonte des trois
//!   autres.
//! - **La valeur manquante d'une colonne mixte est [`MixedCell::Empty`]**,
//!   distincte de `MixedCell::Number(f64::NAN)` (un `NaN` délibérément écrit)
//!   comme de `MixedCell::Text(String::new())` (une chaîne vide délibérée) —
//!   même distinction que `None` contre `Some("")` pour une colonne texte.
//!   Les trois comptent pour « manquante » à l'affichage ([`Cell::
//!   is_missing`]), pas au stockage.
//! - **Une colonne mixte n'est pas une colonne texte.** Elle est traçable :
//!   [`Column::numeric_view`] en tire une vue `f64` où les cellules texte et
//!   vides valent `NaN`, donc où seules les cellules numériques sont tracées
//!   (le renderer ignore déjà les points non finis). Cette vue est la seule
//!   de tout le module qui **copie** — voir sa documentation.

use std::sync::Arc;

use crate::error::{CoreError, CoreResult};

/// Type de données stocké. `Numeric` et `DateTime` partagent la représentation
/// `f64` (pour `DateTime`, un timestamp POSIX en secondes) ; `Mixed` porte le
/// type **par cellule** (voir la doc de tête du module).
///
/// Il n'y a plus de type `Text` homogène : une colonne texte est une colonne
/// `Mixed` dont chaque cellule est [`MixedCell::Text`] — fonctionnellement
/// équivalente (même empreinte mémoire par cellule que l'ancien
/// `Option<String>`), et qui évite un quatrième type homogène à maintenir
/// séparément. Un fichier `.stf` écrit avant ce changement porte encore
/// l'ancienne étiquette `Text` : `stokesforge-persistence` la relit et migre la
/// colonne en `Mixed` (voir `crates/stokesforge-persistence/src/convert.rs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColumnType {
    /// Valeurs numériques (`f64`), manquante = `NaN`.
    Numeric,
    /// Timestamps POSIX en secondes (`f64`), manquante = `NaN`.
    DateTime,
    /// Types mixtes : chaque ligne est indépendamment vide, un nombre ou un
    /// texte ([`MixedCell`]). Manquante = [`MixedCell::Empty`].
    Mixed,
}

impl ColumnType {
    /// Vrai si le buffer sous-jacent est un `Vec<f64>` contigu — donc si la
    /// colonne a une [`Column::float_values`] et peut alimenter une vue numpy
    /// sans copie. Faux pour `Mixed`.
    pub fn is_float_backed(self) -> bool {
        matches!(self, ColumnType::Numeric | ColumnType::DateTime)
    }

    /// Nom du type en français, pour les messages destinés à un humain —
    /// pendant de [`crate::NodeKind::type_name`]. Un seul endroit, pour que
    /// deux messages ne nomment pas le même type différemment.
    pub fn type_name(self) -> &'static str {
        match self {
            ColumnType::Numeric => "numeric",
            ColumnType::DateTime => "date/time",
            ColumnType::Mixed => "mixed",
        }
    }
}

/// Une cellule d'une colonne à types mixtes ([`ColumnType::Mixed`]) : le type
/// y est porté par la ligne, pas par la colonne.
///
/// `Empty` est la valeur manquante de ce type de colonne, **distincte** de
/// `Number(f64::NAN)` et de `Text(String::new())` : les trois s'affichent
/// comme une cellule vide ([`Cell::is_missing`]), mais seules `Number` et
/// `Text` gardent en mémoire la trace d'une saisie délibérée — c'est la même
/// distinction que `None` contre `Some("")` pour une colonne texte.
#[derive(Debug, Clone, Default)]
pub enum MixedCell {
    /// Cellule vide : la valeur manquante d'une colonne mixte.
    #[default]
    Empty,
    /// Un nombre (ou un timestamp POSIX, comme pour `DateTime`).
    Number(f64),
    /// Un texte. `Text(String::new())` est une chaîne vide délibérée,
    /// distincte de [`MixedCell::Empty`].
    Text(String),
}

impl PartialEq for MixedCell {
    /// Égalité de contenu stocké, pas égalité IEEE : les `f64` sont comparés
    /// par motif binaire (`to_bits`), donc `NaN == NaN` et `0.0 != -0.0` —
    /// même règle que [`Cell`], [`ValueBlock`] et [`ColumnValues`].
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (MixedCell::Empty, MixedCell::Empty) => true,
            (MixedCell::Number(a), MixedCell::Number(b)) => a.to_bits() == b.to_bits(),
            (MixedCell::Text(a), MixedCell::Text(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for MixedCell {}

impl MixedCell {
    /// La valeur numérique de la cellule, ou `NaN` si elle n'en porte pas.
    ///
    /// Un texte **n'est pas réanalysé** ici : c'est la lecture d'une valeur
    /// déjà stockée, pas une conversion. La promotion d'un texte numérique en
    /// nombre a lieu une fois pour toutes à l'écriture
    /// ([`MixedCell::from_text`]) ou au changement de type
    /// ([`Column::set_column_type`]).
    pub fn as_number(&self) -> f64 {
        match self {
            MixedCell::Number(value) => *value,
            MixedCell::Empty | MixedCell::Text(_) => f64::NAN,
        }
    }

    /// Vrai si la cellule compte comme manquante à l'affichage : vide, `NaN`,
    /// ou chaîne vide — même règle que [`Cell::is_missing`].
    pub fn is_missing(&self) -> bool {
        match self {
            MixedCell::Empty => true,
            MixedCell::Number(value) => value.is_nan(),
            MixedCell::Text(text) => text.is_empty(),
        }
    }

    /// La cellule d'un texte saisi : `None` (cellule effacée) donne `Empty`,
    /// un texte qui s'analyse en nombre fini donne `Number`, tout le reste
    /// donne `Text`.
    ///
    /// L'analyse est celle de [`parse_f64_text`] — la **seule** du module,
    /// partagée avec `Text → Numeric` : les deux chemins ne peuvent donc pas
    /// diverger sur « ce texte est-il un nombre ? ».
    pub fn from_text(text: Option<String>) -> Self {
        match text {
            None => MixedCell::Empty,
            Some(text) => match parse_f64_text(&text) {
                Some(value) => MixedCell::Number(value),
                None => MixedCell::Text(text),
            },
        }
    }

    /// La cellule d'une valeur numérique : `NaN` — la valeur manquante des
    /// colonnes `f64` — devient `Empty`, tout le reste devient `Number`.
    pub fn from_f64(value: f64) -> Self {
        if value.is_nan() {
            MixedCell::Empty
        } else {
            MixedCell::Number(value)
        }
    }

    /// La forme écrite de la cellule, pour `Mixed → Text` : `None` pour une
    /// cellule vide, le texte tel quel pour un texte, et la forme retenue par
    /// [`format_f64_for_display`] pour un nombre.
    pub fn into_text(self) -> Option<String> {
        match self {
            MixedCell::Empty => None,
            MixedCell::Number(value) if value.is_nan() => None,
            MixedCell::Number(value) => Some(format_f64_for_display(value)),
            MixedCell::Text(text) => Some(text),
        }
    }
}

/// Rôle de la colonne dans un tracé, façon Origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ColumnRole {
    /// Abscisse.
    X,
    /// Ordonnée (rôle par défaut d'une colonne nouvellement créée).
    #[default]
    Y,
    /// Abscisse du second calque (#167) : axe X en haut du graphique.
    X2,
    /// Ordonnée du second calque (#167), appariée à la colonne X2 la plus
    /// proche à sa gauche : axe Y à droite du graphique.
    Y2,
    /// Troisième axe (surfaces, matrices affichées en colonnes).
    Z,
    /// Barre d'erreur en X.
    XError,
    /// Barre d'erreur en Y.
    YError,
    /// Étiquette textuelle associée aux points.
    Label,
    /// Aucun rôle de tracé (colonne auxiliaire).
    None,
}

/// Métadonnées d'affichage et de tracé d'une colonne (hors nom et type).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ColumnMeta {
    /// Rôle de tracé (X, Y, Z, barre d'erreur, étiquette, ...).
    pub role: ColumnRole,
    /// Unité affichée (`"ms"`, `"°C"`, ...), libre.
    pub unit: String,
    /// Nom long / description, distinct du nom court porté par le nœud.
    pub long_name: String,
    /// Commentaire libre.
    pub comment: String,
    /// Valeur de la ligne spéciale « x' » (#164) : une abscisse propre à la
    /// colonne — typiquement l'instant d'un spectre dans une cinétique —,
    /// qui joue le rôle d'un second X quand on trace une **ligne** de
    /// données. `None` : ligne x' vide pour cette colonne. Toujours finie
    /// (la couche d'interface refuse NaN et l'infini).
    pub x_prime: Option<f64>,
    /// Format d'affichage (chaîne libre interprétée par la couche UI).
    pub display_format: String,
    /// Formule de calcul (`"A+B"`, `"(A-B)*2"`, ...), `None` pour une colonne
    /// ordinaire. Une colonne dont la formule est `Some(_)` est recalculée
    /// automatiquement — voir `stokesforge_core::formula` pour l'évaluateur et
    /// `stokesforge-session::actor` pour l'orchestration du recalcul — et refuse
    /// une écriture directe (`Command::SetCells`, voir `stokesforge-commands`) :
    /// ses valeurs ne sont jamais la source de vérité, seule la formule
    /// l'est. `#[serde(default)]` côté persistance (`stokesforge-persistence`) :
    /// un fichier écrit avant ce champ n'a que des colonnes ordinaires.
    pub formula: Option<String>,
    /// Métadonnées du fichier d'origine (en-tête, et pied de page si la
    /// recette le demande), conservées à l'import — voir [`MetaLine`]. Vide
    /// pour une colonne saisie à la main. Lecture seule côté interface : seule
    /// l'écriture initiale (macro « Importer ») les pose, aucune commande
    /// d'édition.
    pub file_metadata: Vec<MetaLine>,
    /// D'où viennent les valeurs quand la colonne a été importée d'un fichier :
    /// de quoi les **actualiser** depuis ce fichier (équivalent du *Data
    /// Connector* d'Origin). `None` pour une colonne saisie à la main.
    pub import_source: Option<ImportSource>,
}

/// Source d'une colonne importée : le fichier, la recette qui l'a lu
/// (sérialisée en JSON, le modèle ne connaît pas `stokesforge-import`) et la
/// position de la colonne dans le fichier.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ImportSource {
    /// Chemin du fichier source.
    pub path: String,
    /// La recette d'import, en JSON.
    pub recipe_json: String,
    /// Index de la colonne dans le fichier (0 = la première).
    pub column: usize,
    /// Rang du bloc de données dans le fichier (0 = le premier), pour un
    /// fichier à plusieurs blocs.
    pub block: usize,
}

/// Une ligne de métadonnées de fichier, découpée en clé / valeur
/// (`XUNITS;NANOMETERS` → `XUNITS` | `NANOMETERS`). Une ligne `[Section]` est
/// une clé entre crochets sans valeur ; une ligne qui n'a pas la forme
/// clé/valeur est gardée telle quelle, dans `value`, avec une clé vide.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MetaLine {
    /// Clé (vide pour une ligne sans forme clé/valeur).
    pub key: String,
    /// Valeur, ou la ligne entière si la clé est vide.
    pub value: String,
}

impl MetaLine {
    /// Vrai pour un titre de section (`[Measurement Information]`).
    pub fn is_section(&self) -> bool {
        self.value.is_empty() && self.key.starts_with('[') && self.key.ends_with(']')
    }
}

/// Bloc de valeurs typé, tel qu'on l'écrit dans une colonne ou qu'on le
/// récupère en delta d'annulation.
#[derive(Debug, Clone)]
pub enum ValueBlock {
    /// Valeurs numériques ou date-heure.
    Float(Vec<f64>),
    /// Valeurs à types mixtes, une [`MixedCell`] par ligne.
    Mixed(Vec<MixedCell>),
}

impl PartialEq for ValueBlock {
    /// Égalité de contenu stocké, pas égalité IEEE : les `f64` sont comparés par
    /// motif binaire (`to_bits`), donc `NaN == NaN` et `0.0 != -0.0`.
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ValueBlock::Float(a), ValueBlock::Float(b)) => {
                a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.to_bits() == y.to_bits())
            }
            (ValueBlock::Mixed(a), ValueBlock::Mixed(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for ValueBlock {}

impl ValueBlock {
    /// Nombre de valeurs du bloc.
    pub fn len(&self) -> usize {
        match self {
            ValueBlock::Float(values) => values.len(),
            ValueBlock::Mixed(values) => values.len(),
        }
    }

    /// Vrai si le bloc ne contient aucune valeur.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Bloc vide du type de colonne donné.
    pub fn empty_for(column_type: ColumnType) -> Self {
        match column_type {
            ColumnType::Numeric | ColumnType::DateTime => ValueBlock::Float(Vec::new()),
            ColumnType::Mixed => ValueBlock::Mixed(Vec::new()),
        }
    }

    /// `count` valeurs manquantes du type donné.
    pub fn missing_for(column_type: ColumnType, count: usize) -> Self {
        match column_type {
            ColumnType::Numeric | ColumnType::DateTime => ValueBlock::Float(vec![f64::NAN; count]),
            ColumnType::Mixed => ValueBlock::Mixed(vec![MixedCell::Empty; count]),
        }
    }

    /// Le type de colonne que ce bloc peut alimenter directement, ou `None`
    /// pour un bloc `Float` — qui en alimente deux (`Numeric` et `DateTime`)
    /// et ne désigne donc pas un type à lui seul. Utilisé par
    /// [`ValueBlock::fits`].
    fn sole_column_type(&self) -> Option<ColumnType> {
        match self {
            ValueBlock::Float(_) => None,
            ValueBlock::Mixed(_) => Some(ColumnType::Mixed),
        }
    }

    /// Vrai si ce bloc peut s'écrire tel quel dans une colonne de type
    /// `column_type` — la condition exacte que [`Column::set_values`] vérifie
    /// avant de refuser par `CoreError::TypeMismatch`. `pub` parce qu'un
    /// appelant qui sait d'avance qu'un bloc ne convient pas peut choisir de
    /// convertir plutôt que d'essuyer un refus (c'est ce que fait
    /// `Command::SetCellsPromoting`, dans `stokesforge-commands`).
    pub fn fits(&self, column_type: ColumnType) -> bool {
        match self.sole_column_type() {
            Some(sole) => sole == column_type,
            None => column_type.is_float_backed(),
        }
    }

    /// Le même bloc, exprimé en cellules mixtes — sans perte pour `Mixed`
    /// (aucune conversion), et selon les mêmes règles que
    /// [`Column::set_column_type`] pour les deux autres : `NaN` et `None`
    /// deviennent [`MixedCell::Empty`], et un texte qui s'analyse en nombre
    /// **est promu en nombre** ([`MixedCell::from_text`]).
    pub fn into_mixed(self) -> Vec<MixedCell> {
        match self {
            ValueBlock::Float(values) => values.into_iter().map(MixedCell::from_f64).collect(),
            ValueBlock::Mixed(values) => values,
        }
    }
}

/// Une cellule lue.
#[derive(Debug, Clone)]
pub enum Cell {
    /// Valeur numérique ou date-heure.
    Float(f64),
    /// Valeur texte ; `None` = manquante.
    Text(Option<String>),
}

impl PartialEq for Cell {
    /// Égalité de contenu stocké, pas égalité IEEE : les `f64` sont comparés par
    /// motif binaire (`to_bits`), donc `NaN == NaN` et `0.0 != -0.0`.
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Cell::Float(a), Cell::Float(b)) => a.to_bits() == b.to_bits(),
            (Cell::Text(a), Cell::Text(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for Cell {}

impl Cell {
    /// Vrai si la valeur est manquante : `NaN` pour `Float`, `None` **ou `Some("")`** pour
    /// `Text` (une chaîne vide est stockée distinctement mais compte comme manquante à
    /// l'affichage — c'est le comportement de la version Python).
    pub fn is_missing(&self) -> bool {
        match self {
            Cell::Float(value) => value.is_nan(),
            Cell::Text(None) => true,
            Cell::Text(Some(text)) => text.is_empty(),
        }
    }
}

impl From<MixedCell> for Cell {
    /// La lecture d'une cellule mixte dans le vocabulaire commun à tous les
    /// types de colonne : un nombre est un `Float`, un texte un
    /// `Text(Some(_))`, et une cellule vide un `Text(None)` — la forme que
    /// tous les lecteurs de [`Cell`] traitent déjà comme manquante.
    ///
    /// La conversion **perd** la distinction entre `Empty` et une colonne
    /// texte sans valeur, parce qu'aucun lecteur de `Cell` n'en a besoin :
    /// les deux se lisent « cellule sans valeur ». Un appelant qui veut la
    /// cellule telle qu'elle est stockée lit [`Column::cell`], pas
    /// [`Column::get`].
    fn from(cell: MixedCell) -> Self {
        match cell {
            MixedCell::Empty => Cell::Text(None),
            MixedCell::Number(value) => Cell::Float(value),
            MixedCell::Text(text) => Cell::Text(Some(text)),
        }
    }
}

/// Buffer sous-jacent, partagé par `Arc`. C'est cet `Arc` que le pont Python
/// clonera pour tenir une vue numpy vivante.
#[derive(Debug, Clone)]
pub enum ColumnValues {
    /// Buffer numérique ou date-heure partagé.
    Float(Arc<Vec<f64>>),
    /// Buffer à types mixtes partagé, une [`MixedCell`] par ligne.
    Mixed(Arc<Vec<MixedCell>>),
}

impl PartialEq for ColumnValues {
    /// Égalité de contenu stocké, pas égalité IEEE : les `f64` sont comparés par
    /// motif binaire (`to_bits`), donc `NaN == NaN` et `0.0 != -0.0`.
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ColumnValues::Float(a), ColumnValues::Float(b)) => {
                a.len() == b.len()
                    && a.iter()
                        .zip(b.iter())
                        .all(|(x, y)| x.to_bits() == y.to_bits())
            }
            (ColumnValues::Mixed(a), ColumnValues::Mixed(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for ColumnValues {}

impl ColumnValues {
    /// Longueur du buffer.
    pub fn len(&self) -> usize {
        match self {
            ColumnValues::Float(values) => values.len(),
            ColumnValues::Mixed(values) => values.len(),
        }
    }

    /// Vrai si le buffer est vide.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Cellule à la ligne `row` ; valeur manquante au-delà de la longueur (jamais une erreur).
    pub fn get(&self, row: usize) -> Cell {
        match self {
            ColumnValues::Float(values) => {
                Cell::Float(values.get(row).copied().unwrap_or(f64::NAN))
            }
            ColumnValues::Mixed(values) => Cell::from(values.get(row).cloned().unwrap_or_default()),
        }
    }

    /// Cellule à la ligne `row`, **telle qu'elle est stockée** — la lecture
    /// qui ne perd pas la distinction entre une cellule vide et une cellule
    /// texte sans valeur (voir `impl From<MixedCell> for Cell`).
    /// [`MixedCell::Empty`] au-delà de la longueur, jamais une erreur.
    pub fn cell(&self, row: usize) -> MixedCell {
        match self {
            ColumnValues::Float(values) => values
                .get(row)
                .copied()
                .map_or(MixedCell::Empty, MixedCell::from_f64),
            ColumnValues::Mixed(values) => values.get(row).cloned().unwrap_or_default(),
        }
    }

    /// La valeur numérique à la ligne `row`, ou `NaN` — sans allocation,
    /// quel que soit le type de colonne.
    ///
    /// C'est la lecture des **calculs** (`crate::formula`), là où [`Cell`]
    /// ferait cloner une `String` par ligne pour la jeter aussitôt. Un texte
    /// n'est jamais réanalysé : voir [`MixedCell::as_number`].
    pub fn number_at(&self, row: usize) -> f64 {
        match self {
            ColumnValues::Float(values) => values.get(row).copied().unwrap_or(f64::NAN),
            ColumnValues::Mixed(values) => values.get(row).map_or(f64::NAN, MixedCell::as_number),
        }
    }

    /// Statut numérique de la cellule à la ligne `row` — pendant de
    /// [`ColumnValues::number_at`], mais qui distingue **pourquoi** une
    /// cellule ne porte pas de nombre : [`NumericStatus::Missing`] (rien à
    /// calculer : cellule vide, ou hors bornes du buffer) contre
    /// [`NumericStatus::Failed`] (un texte non vide, non numérique, est
    /// présent — un vrai échec de calcul, pas une absence). Sert à
    /// `crate::formula` pour distinguer, ligne à ligne, une formule qui n'a
    /// simplement rien à faire d'une formule qui échoue vraiment (affichée
    /// `---` côté UI, contre une cellule laissée vide).
    ///
    pub fn status_at(&self, row: usize) -> NumericStatus {
        match self {
            ColumnValues::Float(values) => match values.get(row) {
                Some(value) if value.is_nan() => NumericStatus::Missing,
                Some(_) => NumericStatus::Ok,
                None => NumericStatus::Missing,
            },
            ColumnValues::Mixed(values) => match values.get(row) {
                None | Some(MixedCell::Empty) => NumericStatus::Missing,
                Some(MixedCell::Number(value)) if value.is_nan() => NumericStatus::Missing,
                Some(MixedCell::Number(_)) => NumericStatus::Ok,
                Some(MixedCell::Text(text)) if text.is_empty() => NumericStatus::Missing,
                Some(MixedCell::Text(_)) => NumericStatus::Failed,
            },
        }
    }
}

/// Statut d'une cellule vis-à-vis d'un calcul numérique — voir
/// [`ColumnValues::status_at`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumericStatus {
    /// Une valeur numérique finie est présente.
    Ok,
    /// Rien à cette ligne : cellule vide, ou ligne au-delà de la longueur de
    /// la colonne.
    Missing,
    /// Un texte non vide, non numérique, occupe la cellule : le calcul ne
    /// peut pas se faire, ce n'est pas une simple absence.
    Failed,
}

impl NumericStatus {
    /// Combine deux statuts d'opérandes d'un même opérateur binaire :
    /// `Failed` l'emporte toujours (un vrai échec ne doit jamais se cacher
    /// derrière une case vide), sinon `Missing` l'emporte sur `Ok`.
    pub fn combine(self, other: NumericStatus) -> NumericStatus {
        match (self, other) {
            (NumericStatus::Failed, _) | (_, NumericStatus::Failed) => NumericStatus::Failed,
            (NumericStatus::Missing, _) | (_, NumericStatus::Missing) => NumericStatus::Missing,
            (NumericStatus::Ok, NumericStatus::Ok) => NumericStatus::Ok,
        }
    }
}

/// Ce qu'il faut pour annuler un `set_values` sans copier la colonne : les
/// valeurs écrasées et la longueur précédente. Ne contient pas de `f64` en
/// propre (ils sont dans `previous: ValueBlock`) : dériver `PartialEq`/`Eq`
/// reste correct puisqu'il délègue à l'égalité bit à bit de `ValueBlock`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValuesDelta {
    /// Ligne de départ de l'écriture qui a produit ce delta.
    pub start: usize,
    /// Valeurs écrasées par l'écriture (partie recouvrant des lignes déjà présentes).
    pub previous: ValueBlock,
    /// Longueur de la colonne avant l'écriture.
    pub previous_length: usize,
}

/// Vecteur de données de longueur libre plus ses métadonnées. Le nom vit
/// dans le nœud de l'arbre.
#[derive(Debug, Clone)]
pub struct Column {
    column_type: ColumnType,
    values: ColumnValues,
    meta: ColumnMeta,
    version: u64,
}

/// **Le compteur de version est exclu de l'égalité, délibérément.**
///
/// L'invariant que teste `stokesforge-commands` est « après undo, l'arbre est
/// bit à bit celui d'avant » ; or annuler une écriture est *une écriture de
/// plus*, donc incrémente le compteur. L'inclure ferait échouer cette
/// propriété alors que les données sont bien identiques.
///
/// Conséquence à connaître : un aller-retour disque n'est **pas** couvert
/// par cette égalité pour le compteur — d'où un test dédié dans
/// `stokesforge-persistence`.
impl PartialEq for Column {
    fn eq(&self, other: &Self) -> bool {
        self.column_type == other.column_type
            && self.values == other.values
            && self.meta == other.meta
    }
}

/// B3 (`guide-de-correction-robustesse.md` §F4) — plafond partagé par
/// `write_block`, `insert_rows` et `set_length` : au-delà, dimensionner
/// l'allocation demandée fait `abort()` (échec d'allocation, non
/// rattrapable — voir `stokesforge-sonde-alloc`) avant même qu'une erreur
/// n'ait la moindre chance d'être construite. Cent millions de lignes,
/// arbitraire mais documenté : très au-delà de tout projet réaliste, très en
/// deçà de ce qui menace le processus (à 8 octets/valeur, l'allocation reste
/// de l'ordre du gigaoctet).
/// Plafond commun aux écritures du modèle et à la copie des entrées de script.
pub const MAX_COLUMN_ROWS: usize = 100_000_000;

/// Écrit `new_values` dans `vec` à partir de `start`, en étendant `vec` avec
/// des valeurs manquantes (`missing`) si besoin. Rend les valeurs écrasées
/// (uniquement la portion de `new_values` qui recouvre des lignes déjà
/// présentes) ainsi que la longueur précédente de `vec`.
///
/// B3 — `Err(CoreError::IndexOutOfBounds)` si la longueur résultante
/// dépasserait [`MAX_COLUMN_ROWS`], **avant** tout redimensionnement : un
/// `start` ou un `new_values.len()` incontrôlés (`SetCells { start: 1<<45 }`)
/// ne doivent jamais atteindre `resize_with`/`extend`, seule façon d'éviter
/// l'`abort()` d'un échec d'allocation, qu'aucun `Result` ne rattrape après
/// coup.
fn write_block<T: Clone>(
    vec: &mut Vec<T>,
    start: usize,
    new_values: Vec<T>,
    missing: impl Fn() -> T,
) -> CoreResult<(Vec<T>, usize)> {
    // C3 (`guide-de-correction-robustesse.md` §F5, piège en §6) — un bloc
    // vide n'a rien à écrire : sortie anticipée plutôt que de laisser
    // `resize_with(start, missing)` ci-dessous allonger la colonne jusqu'à
    // `start` avant de découvrir qu'il n'y a aucune valeur à y placer. Rendre
    // `Ok` avec un delta neutre (`previous` vide, `previous_length`
    // inchangée) est nécessaire, pas seulement suffisant :
    // `History::revert_delta` appelle `set_values(start, before)` avec un
    // `before` vide chaque fois qu'on annule une écriture faite au-delà de
    // la fin de la colonne — un bloc vide qui échouerait casserait
    // l'annulation.
    if new_values.is_empty() {
        return Ok((Vec::new(), vec.len()));
    }
    let resulting_len = start.saturating_add(new_values.len()).max(vec.len());
    if resulting_len > MAX_COLUMN_ROWS {
        return Err(CoreError::IndexOutOfBounds(format!(
            "the write would grow the column to {resulting_len} rows, beyond the limit of {MAX_COLUMN_ROWS}"
        )));
    }
    let previous_length = vec.len();
    let overlap_start = start.min(previous_length);
    let overlap_end = previous_length.min(start.saturating_add(new_values.len()));
    let previous = vec
        .get(overlap_start..overlap_end)
        .map(|slice| slice.to_vec())
        .unwrap_or_default();

    if vec.len() < start {
        vec.resize_with(start, missing);
    }

    let mut incoming = new_values.into_iter();
    if let Some(tail) = vec.get_mut(start..) {
        for slot in tail.iter_mut() {
            match incoming.next() {
                Some(value) => *slot = value,
                None => break,
            }
        }
    }
    vec.extend(incoming);

    Ok((previous, previous_length))
}

/// Tronque ou complète `vec` à `length`, en remplissant l'extension
/// éventuelle avec `missing`. Rend la queue retirée (vide si extension ou
/// longueur inchangée) et la longueur précédente — le corps commun aux trois
/// branches de [`Column::set_length`], une par type de buffer.
///
/// N'applique pas [`MAX_COLUMN_ROWS`] : c'est l'appelant qui borne `length`
/// avant d'appeler (voir la doc de `set_length`, qui doit rester infaillible).
fn resize_block<T: Clone>(vec: &mut Vec<T>, length: usize, missing: T) -> (Vec<T>, usize) {
    let previous_length = vec.len();
    if length < previous_length {
        return (vec.split_off(length), previous_length);
    }
    if length > previous_length {
        vec.resize(length, missing);
    }
    (Vec::new(), previous_length)
}

/// Analyse un texte en valeur `f64` pour `Column::set_column_type`
/// (`Text → Numeric`/`DateTime`). Rend `None` — même traitement que la valeur
/// manquante du modèle — dans deux cas que ce parseur distingue
/// délibérément l'un de l'autre :
///
/// - le texte ne s'analyse pas du tout (`"pas un nombre"`) ;
/// - le texte s'analyse comme un nombre **fini** en toutes lettres (chiffres,
///   point, exposant) mais la valeur qu'il désigne déborde `f64` — `"1e400"`
///   est syntaxiquement un nombre, mais aucun `f64` fini ne le porte.
///
/// Un texte qui **demande** explicitement une non-finitude — `"inf"`,
/// `"-Infinity"`, `"NaN"`, avec ou sans signe, insensible à la casse — n'est
/// pas ce débordement : l'utilisateur a écrit ce qu'il voulait, on le lui
/// rend tel quel (`"NaN"` devient d'ailleurs la valeur manquante du modèle,
/// pas une erreur : c'est la même valeur, écrite en toutes lettres).
///
/// C2 (`guide-de-correction-robustesse.md` §F1, §F8) : la décision du dépôt
/// pour une valeur d'entrée non représentable est de la refuser plutôt que
/// de la convertir en silence — c'est déjà la règle pour `int64` côté pont
/// Python. Ici, « refuser » un débordement veut dire : ne jamais produire de
/// `+inf`/`-inf` implicite, retomber sur la valeur manquante du modèle,
/// exactement comme pour un texte qui ne s'analyse pas.
fn parse_f64_text(text: &str) -> Option<f64> {
    let trimmed = text.trim();
    let lower = trimmed.to_ascii_lowercase();
    match lower.as_str() {
        "inf" | "+inf" | "infinity" | "+infinity" => return Some(f64::INFINITY),
        "-inf" | "-infinity" => return Some(f64::NEG_INFINITY),
        "nan" | "+nan" | "-nan" => return Some(f64::NAN),
        _ => {}
    }
    match trimmed.parse::<f64>() {
        Ok(value) if value.is_finite() => Some(value),
        // Débordement réel (`"1e400"`) ou échec d'analyse : refusé, jamais
        // silencieusement `+inf`/`-inf`.
        Ok(_) | Err(_) => None,
    }
}

/// Formate un `f64` pour l'affichage ou pour `Column::set_column_type`
/// (`Numeric → Text`) — **la** forme écrite d'une valeur du modèle (C4,
/// `guide-de-correction-robustesse.md` §F8) : `Column::set_column_type` et
/// `AppState::cell_text` (`stokesforge-app`) doivent produire la même chaîne pour
/// la même valeur, faute de quoi l'affichage et la conversion se
/// contrediraient.
///
/// Bascule vers la notation scientifique (`{v:e}`, jamais réémise par le
/// `Display` de `f64`) chaque fois qu'elle est plus courte que la forme
/// décimale : une mesure de l'ordre de 10⁻¹⁹ s'écrit alors sur une quinzaine
/// de caractères plutôt que sur trente, et un entier ordinaire (`1`, `100`)
/// garde sa forme décimale, plus courte que son exposant (`1e0`, `1e2`).
///
/// Fidèle bit à bit à l'aller-retour : `Display` et `LowerExp` de `f64`
/// produisent chacun la représentation décimale la plus courte qui reparse
/// exactement à la valeur d'origine (dénormaux et `-0.0` compris) ; ce
/// choix ne fait que sélectionner laquelle des deux formes écrire.
pub fn format_f64_for_display(v: f64) -> String {
    if !v.is_finite() {
        // "NaN" n'atteint jamais cette fonction en pratique (c'est la valeur
        // manquante du modèle, traduite en `None` avant d'appeler ici) ;
        // ±inf, en revanche, est une valeur du modèle comme une autre.
        return v.to_string();
    }
    let decimal = v.to_string();
    let scientific = format!("{v:e}");
    if scientific.len() < decimal.len() {
        scientific
    } else {
        decimal
    }
}

impl Column {
    /// Colonne vide du type donné, métadonnées par défaut.
    pub fn new(column_type: ColumnType) -> Self {
        let values = match column_type {
            ColumnType::Numeric | ColumnType::DateTime => ColumnValues::Float(Arc::new(Vec::new())),
            ColumnType::Mixed => ColumnValues::Mixed(Arc::new(Vec::new())),
        };
        Self {
            column_type,
            values,
            meta: ColumnMeta::default(),
            version: 0,
        }
    }

    /// Colonne numérique initialisée (copie le slice).
    pub fn from_f64(values: &[f64]) -> Self {
        Self {
            column_type: ColumnType::Numeric,
            values: ColumnValues::Float(Arc::new(values.to_vec())),
            meta: ColumnMeta::default(),
            version: 0,
        }
    }

    /// Colonne mixte initialisée depuis des chaînes optionnelles : `None`
    /// devient [`MixedCell::Empty`], chaque texte devient [`MixedCell::Text`]
    /// **tel quel**, sans être réanalysé en nombre (contrairement à
    /// [`ValueBlock::into_mixed`]) — c'est l'équivalent, pour une colonne
    /// mixte, de l'ancienne colonne texte homogène.
    pub fn from_text(values: &[Option<String>]) -> Self {
        let cells: Vec<MixedCell> = values
            .iter()
            .cloned()
            .map(|value| match value {
                None => MixedCell::Empty,
                Some(text) => MixedCell::Text(text),
            })
            .collect();
        Self::from_mixed(&cells)
    }

    /// Colonne à types mixtes initialisée (copie). Les cellules sont prises
    /// **telles quelles** : contrairement à [`ValueBlock::into_mixed`], rien
    /// n'est promu ni analysé ici, l'appelant a déjà décidé de chaque ligne.
    pub fn from_mixed(values: &[MixedCell]) -> Self {
        Self {
            column_type: ColumnType::Mixed,
            values: ColumnValues::Mixed(Arc::new(values.to_vec())),
            meta: ColumnMeta::default(),
            version: 0,
        }
    }

    /// Colonne depuis un bloc typé et des métadonnées. `Err(TypeMismatch)` si le bloc ne
    /// correspond pas au type (`Float` pour `Numeric`/`DateTime`, `Mixed` pour `Mixed`).
    pub fn from_block(
        column_type: ColumnType,
        block: ValueBlock,
        meta: ColumnMeta,
    ) -> CoreResult<Self> {
        let values = match (column_type, block) {
            (ColumnType::Numeric | ColumnType::DateTime, ValueBlock::Float(values)) => {
                ColumnValues::Float(Arc::new(values))
            }
            (ColumnType::Mixed, ValueBlock::Mixed(values)) => ColumnValues::Mixed(Arc::new(values)),
            _ => {
                return Err(CoreError::TypeMismatch(
                    "the value block does not match the column type".to_string(),
                ))
            }
        };
        Ok(Self {
            column_type,
            values,
            meta,
            version: 0,
        })
    }

    /// Comme [`Column::from_block`], mais avec un compteur de version imposé.
    /// Réservé à la **reconstruction depuis un fichier** : le compteur est
    /// persisté, et le repartir de zéro au chargement rendrait le compteur
    /// menteur (voir [`Column::version`]).
    pub fn from_block_with_version(
        column_type: ColumnType,
        block: ValueBlock,
        meta: ColumnMeta,
        version: u64,
    ) -> CoreResult<Self> {
        let mut column = Self::from_block(column_type, block, meta)?;
        column.version = version;
        Ok(column)
    }

    /// Type de données de la colonne.
    pub fn column_type(&self) -> ColumnType {
        self.column_type
    }

    /// Buffer de valeurs partagé.
    pub fn values(&self) -> &ColumnValues {
        &self.values
    }

    /// Métadonnées d'affichage et de tracé.
    pub fn meta(&self) -> &ColumnMeta {
        &self.meta
    }

    /// Compteur de modification des **données** : il répond à « cette colonne
    /// a-t-elle changé depuis la dernière fois que je l'ai lue ? » sans relire
    /// les valeurs. Un lecteur retient le compteur avec ce qu'il a produit et
    /// se sait périmé quand il diffère.
    ///
    /// # Règle exacte
    ///
    /// Incrémenté par **tout appel réussi** à un mutateur de données
    /// ([`Column::set_values`], [`Column::set_length`],
    /// [`Column::insert_rows`], [`Column::delete_rows`],
    /// [`Column::set_column_type`], [`Column::restore`]), **y compris quand
    /// l'appel s'avère sans effet** (`insert_rows(_, 0)`, `set_length` à la
    /// même longueur). Un appel refusé (`Err`) n'incrémente pas.
    ///
    /// [`Column::set_meta`] **n'incrémente pas** : une unité ou un commentaire
    /// ne sont pas des données.
    ///
    /// Le sens de l'approximation est délibéré : un faux « a changé » coûte un
    /// recalcul inutile, un faux « n'a pas changé » afficherait un résultat
    /// faux. Seule la première erreur est tolérable.
    ///
    /// # Qui s'y fie
    ///
    /// Le recalcul ciblé des colonnes-formule (`stokesforge-session`,
    /// `recalc.rs`, #193) ne recalcule une formule que si la version d'une de
    /// ses sources a changé, et `Command::SetCells::expected_version` refuse
    /// une écriture fondée sur une lecture périmée. Les deux supposent qu'un
    /// numéro ne désigne jamais deux contenus d'une même colonne : toute
    /// écriture passe par `Project`, qui remplace ce compteur par un numéro
    /// neuf de son compteur unique (`Project::next_column_version`), et une
    /// colonne insérée avec son numéro fait passer ce compteur au-delà
    /// (`Project::insert_node`).
    ///
    /// # Il est persisté
    ///
    /// Repartir de zéro au chargement le rendrait **menteur** : après un
    /// rechargement, quarante-sept écritures ramèneraient le compteur à une
    /// valeur qu'un lecteur d'avant la sauvegarde croirait reconnaître, sur
    /// des données entre-temps devenues tout autres. Voir
    /// [`Column::from_block_with_version`].
    ///
    /// # Il est hors de l'égalité
    ///
    /// `PartialEq` l'ignore — voir l'implémentation et sa justification.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Nombre de lignes de la colonne.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Vrai si la colonne ne contient aucune ligne.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Cellule à `row` ; manquante hors bornes, jamais d'erreur.
    pub fn get(&self, row: usize) -> Cell {
        self.values.get(row)
    }

    /// Cellule à `row` **telle qu'elle est stockée** — voir
    /// [`ColumnValues::cell`]. C'est la lecture à préférer dès qu'une colonne
    /// mixte est possible : [`Column::get`] confond une cellule vide et une
    /// cellule texte sans valeur.
    pub fn cell(&self, row: usize) -> MixedCell {
        self.values.cell(row)
    }

    /// Vrai si la cellule à `row` est manquante (voir `Cell::is_missing`).
    pub fn is_missing(&self, row: usize) -> bool {
        self.get(row).is_missing()
    }

    /// Le buffer `f64` partagé, ou `None` pour une colonne mixte.
    /// C'est l'`Arc` que le pont Python clone pour tenir une vue sans copie —
    /// une colonne mixte n'a pas de buffer `f64` contigu, donc pas de vue
    /// numpy sans copie (voir [`Column::numeric_view`]).
    pub fn float_values(&self) -> Option<&Arc<Vec<f64>>> {
        match &self.values {
            ColumnValues::Float(values) => Some(values),
            ColumnValues::Mixed(_) => None,
        }
    }

    /// Une vue `f64` de la colonne pour le tracé.
    ///
    /// - `Numeric`/`DateTime` : l'`Arc` du buffer, **sans copie** — c'est la
    ///   propriété « zéro copie » de `crate::resolve`.
    /// - `Mixed` : un buffer **neuf**, où chaque cellule numérique donne sa
    ///   valeur et où tout le reste (texte, cellule vide) donne `NaN`. Le
    ///   renderer écarte déjà les points non finis : le résultat est qu'une
    ///   colonne mixte trace exactement ses cellules numériques, sans qu'un
    ///   seul backend ait à le savoir (invariant 5).
    ///
    /// **C'est la seule lecture du module qui copie**, et c'est le prix de la
    /// représentation par cellule : un `Vec<MixedCell>` n'est pas un
    /// `Vec<f64>` contigu, il n'y a rien à partager. Le coût est `O(n)` par
    /// résolution et par colonne mixte **référencée par une série** — pas par
    /// colonne du projet — et il ne concerne aucune colonne homogène.
    pub fn numeric_view(&self) -> Option<Arc<Vec<f64>>> {
        match &self.values {
            ColumnValues::Float(values) => Some(Arc::clone(values)),
            ColumnValues::Mixed(values) => {
                Some(Arc::new(values.iter().map(MixedCell::as_number).collect()))
            }
        }
    }

    /// Écrit `block` à partir de la ligne `start`, en étendant la colonne si besoin ;
    /// l'extension remplit les trous éventuels avec la valeur manquante. Rend les valeurs
    /// écrasées (uniquement la partie de `block` qui recouvre des lignes existantes) et la
    /// longueur précédente. `Err(TypeMismatch)` si la variante du bloc ne correspond pas au type.
    pub fn set_values(&mut self, start: usize, block: ValueBlock) -> CoreResult<ValuesDelta> {
        // Incrémenté sur les seules branches qui écrivent : la branche
        // `Err(TypeMismatch)` laisse la colonne — compteur compris — intacte.
        match (&mut self.values, block) {
            (ColumnValues::Float(arc), ValueBlock::Float(new_values)) => {
                let vec = Arc::make_mut(arc);
                let (previous, previous_length) = write_block(vec, start, new_values, || f64::NAN)?;
                self.version = self.version.wrapping_add(1);
                Ok(ValuesDelta {
                    start,
                    previous: ValueBlock::Float(previous),
                    previous_length,
                })
            }
            (ColumnValues::Mixed(arc), ValueBlock::Mixed(new_values)) => {
                let vec = Arc::make_mut(arc);
                let (previous, previous_length) =
                    write_block(vec, start, new_values, || MixedCell::Empty)?;
                self.version = self.version.wrapping_add(1);
                Ok(ValuesDelta {
                    start,
                    previous: ValueBlock::Mixed(previous),
                    previous_length,
                })
            }
            _ => Err(CoreError::TypeMismatch(format!(
                "value block incompatible with the column type ({:?})",
                self.column_type
            ))),
        }
    }

    /// Tronque ou complète à `length` lignes. Le delta porte la queue retirée (vide si extension
    /// ou inchangé), `start = length`, `previous` = valeurs retirées (vide si aucune),
    /// `previous_length` = ancienne longueur.
    ///
    /// B3 (`guide-de-correction-robustesse.md` §F4) : `length` est d'abord borné à
    /// [`MAX_COLUMN_ROWS`], pour ne jamais tenter un `resize` qui dimensionnerait une
    /// allocation catastrophique (échec d'allocation → `abort()`, non rattrapable — voir
    /// `stokesforge-sonde-alloc`). Contrairement à [`Column::set_values`] et
    /// [`Column::insert_rows`], cette méthode reste **infaillible** par nécessité : c'est la
    /// primitive qu'emploie `History::revert_delta` pour restaurer une longueur antérieure
    /// pendant une annulation, et annuler ne doit jamais pouvoir échouer. En pratique une
    /// longueur restaurée a toujours déjà traversé `write_block`/`insert_rows`, donc jamais
    /// dépassé la borne ; le plafond ici protège un appel direct de l'API cœur avec une
    /// longueur arbitraire (c'est le cas que couvre la sonde).
    pub fn set_length(&mut self, length: usize) -> ValuesDelta {
        let length = length.min(MAX_COLUMN_ROWS);
        self.version = self.version.wrapping_add(1);
        match &mut self.values {
            ColumnValues::Float(arc) => {
                let (previous, previous_length) =
                    resize_block(Arc::make_mut(arc), length, f64::NAN);
                ValuesDelta {
                    start: length,
                    previous: ValueBlock::Float(previous),
                    previous_length,
                }
            }
            ColumnValues::Mixed(arc) => {
                let (previous, previous_length) =
                    resize_block(Arc::make_mut(arc), length, MixedCell::Empty);
                ValuesDelta {
                    start: length,
                    previous: ValueBlock::Mixed(previous),
                    previous_length,
                }
            }
        }
    }

    /// Insère `count` lignes manquantes à partir de `at`. `at` doit être dans `0..=len`
    /// (l'égalité insère en fin) sinon `Err(IndexOutOfBounds)`. B3 (`guide-de-correction-
    /// robustesse.md` §F4) : `Err(IndexOutOfBounds)` aussi si la longueur résultante
    /// dépasserait [`MAX_COLUMN_ROWS`] — un `count` incontrôlé ne doit jamais atteindre
    /// `Vec::splice`, seule façon d'éviter l'`abort()` d'un échec d'allocation.
    pub fn insert_rows(&mut self, at: usize, count: usize) -> CoreResult<()> {
        let len = self.len();
        if at > len {
            return Err(CoreError::IndexOutOfBounds(format!(
                "row {at} out of bounds (length {len})"
            )));
        }
        let resulting_len = len.saturating_add(count);
        if resulting_len > MAX_COLUMN_ROWS {
            return Err(CoreError::IndexOutOfBounds(format!(
                "the insertion would grow the column to {resulting_len} rows, beyond the limit of {MAX_COLUMN_ROWS}"
            )));
        }
        self.version = self.version.wrapping_add(1);
        if count == 0 {
            return Ok(());
        }
        match &mut self.values {
            ColumnValues::Float(arc) => {
                let vec = Arc::make_mut(arc);
                vec.splice(at..at, std::iter::repeat_n(f64::NAN, count));
            }
            ColumnValues::Mixed(arc) => {
                let vec = Arc::make_mut(arc);
                vec.splice(at..at, std::iter::repeat_n(MixedCell::Empty, count));
            }
        }
        Ok(())
    }

    /// Supprime `count` lignes à partir de `at`, rabattu à la fin (`at + count > len` supprime
    /// jusqu'à la fin, sans erreur). `at` doit être dans `0..=len` sinon `Err(IndexOutOfBounds)`.
    /// Rend les valeurs retirées.
    pub fn delete_rows(&mut self, at: usize, count: usize) -> CoreResult<ValueBlock> {
        let len = self.len();
        if at > len {
            return Err(CoreError::IndexOutOfBounds(format!(
                "row {at} out of bounds (length {len})"
            )));
        }
        self.version = self.version.wrapping_add(1);
        let stop = len.min(at.saturating_add(count));
        match &mut self.values {
            ColumnValues::Float(arc) => {
                let vec = Arc::make_mut(arc);
                Ok(ValueBlock::Float(vec.drain(at..stop).collect()))
            }
            ColumnValues::Mixed(arc) => {
                let vec = Arc::make_mut(arc);
                Ok(ValueBlock::Mixed(vec.drain(at..stop).collect()))
            }
        }
    }

    /// Remplace les métadonnées. Rend les précédentes.
    pub fn set_meta(&mut self, meta: ColumnMeta) -> ColumnMeta {
        std::mem::replace(&mut self.meta, meta)
    }

    /// Change le type et convertit les données. Rend `(ancien type, anciennes valeurs)` —
    /// **snapshot** volontaire : la conversion est destructrice dans les deux sens.
    /// `Numeric ↔ DateTime` : aucun changement de valeurs. Même type → aucun changement, rend
    /// quand même `(type, clone des valeurs)`.
    ///
    /// # Vers et depuis `Mixed`
    ///
    /// - `Float → Mixed` : `NaN → Empty`, sinon `Number(v)`. **Sans perte** —
    ///   c'est la conversion que fait une promotion de colonne.
    /// - `Mixed → Float` : `Number(v) → v`, `Empty → NaN`, et un texte est
    ///   analysé par [`parse_f64_text`] (`NaN` si l'analyse échoue ou déborde
    ///   — C2, `guide-de-correction-robustesse.md` §F1/§F8 : jamais de
    ///   `+inf`/`-inf` implicite).
    pub fn set_column_type(&mut self, column_type: ColumnType) -> (ColumnType, ColumnValues) {
        let old_type = self.column_type;
        let old_values = self.values.clone();
        self.version = self.version.wrapping_add(1);

        if column_type == old_type {
            return (old_type, old_values);
        }

        let new_values = match (&self.values, column_type) {
            (ColumnValues::Float(values), ColumnType::Numeric | ColumnType::DateTime) => {
                ColumnValues::Float(Arc::clone(values))
            }
            (ColumnValues::Float(values), ColumnType::Mixed) => ColumnValues::Mixed(Arc::new(
                values.iter().copied().map(MixedCell::from_f64).collect(),
            )),
            (ColumnValues::Mixed(values), ColumnType::Numeric | ColumnType::DateTime) => {
                let converted = values
                    .iter()
                    .map(|cell| match cell {
                        MixedCell::Empty => f64::NAN,
                        MixedCell::Number(value) => *value,
                        MixedCell::Text(text) => parse_f64_text(text).unwrap_or(f64::NAN),
                    })
                    .collect();
                ColumnValues::Float(Arc::new(converted))
            }
            (ColumnValues::Mixed(values), ColumnType::Mixed) => {
                ColumnValues::Mixed(Arc::clone(values))
            }
        };

        self.column_type = column_type;
        self.values = new_values;

        (old_type, old_values)
    }

    /// Remplace entièrement le type et les valeurs (utilisé pour annuler `set_column_type`).
    /// `Err(TypeMismatch)` si `values` ne correspond pas à `column_type`.
    pub fn restore(&mut self, column_type: ColumnType, values: ColumnValues) -> CoreResult<()> {
        let matches_type = matches!(
            (column_type, &values),
            (
                ColumnType::Numeric | ColumnType::DateTime,
                ColumnValues::Float(_)
            ) | (ColumnType::Mixed, ColumnValues::Mixed(_))
        );
        if !matches_type {
            return Err(CoreError::TypeMismatch(format!(
                "the values do not match the type {column_type:?}"
            )));
        }
        self.column_type = column_type;
        self.values = values;
        self.version = self.version.wrapping_add(1);
        Ok(())
    }

    /// Impose un numéro de version, sans toucher ni aux valeurs ni au type.
    ///
    /// Réservé à [`crate::Project::restamp_column_version`], lui-même
    /// réservé à `History::revert` dans `stokesforge-commands` (A7,
    /// `guide-de-correction-robustesse.md` §F2) : après qu'une restauration
    /// de structure (`replace_tree`) a ramené un ancien compteur en
    /// arrière, ré-attribuer un numéro **frais** (jamais vu) empêche qu'une
    /// écriture future ne retombe, pour cette colonne, sur un numéro déjà
    /// porté par un contenu différent — voir la doctrine des estampilles du
    /// guide (§F2 : « une version de contenu ne désigne jamais deux
    /// contenus »).
    ///
    /// Ne clone jamais le buffer (`self.values` n'est pas touché) : coût
    /// `O(1)`, le buffer reste partagé avec l'arbre qu'on vient de quitter.
    pub(crate) fn restamp_version(&mut self, version: u64) {
        self.version = version;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values_of(column: &Column) -> Vec<f64> {
        column
            .float_values()
            .expect("colonne numérique")
            .as_slice()
            .to_vec()
    }

    #[test]
    fn new_numeric_column_is_empty_with_default_metadata() {
        let column = Column::new(ColumnType::Numeric);
        assert_eq!(column.column_type(), ColumnType::Numeric);
        assert_eq!(column.len(), 0);
        assert!(column.float_values().is_some());
        assert_eq!(column.meta().role, ColumnRole::Y);
    }

    #[test]
    fn get_reports_missing_for_nan_and_present_otherwise() {
        let column = Column::from_f64(&[1.0, f64::NAN, 3.0]);
        assert!(!column.get(0).is_missing());
        assert!(column.get(1).is_missing());
        assert!(column.is_missing(1));
    }

    #[test]
    fn reading_past_the_end_yields_missing_without_growing_length() {
        let column = Column::from_f64(&[1.0, 2.0]);
        assert!(column.get(99).is_missing());
        assert_eq!(column.len(), 2);
    }

    #[test]
    fn from_f64_copies_the_source_slice() {
        let mut source = vec![1.0, 2.0, 3.0];
        let column = Column::from_f64(&source);
        source[0] = 99.0;
        assert_eq!(column.get(0), Cell::Float(1.0));
    }

    #[test]
    fn text_columns_distinguish_empty_string_from_missing() {
        let column = Column::from_text(&[Some("a".to_string()), Some(String::new()), None]);
        assert_eq!(column.get(0), Cell::Text(Some("a".to_string())));
        assert_eq!(column.get(1), Cell::Text(Some(String::new())));
        assert!(column.get(1).is_missing());
        assert_eq!(column.get(2), Cell::Text(None));
        assert_eq!(column.get(9), Cell::Text(None));
        assert!(column.float_values().is_none());
    }

    #[test]
    fn from_block_rejects_mismatched_block_variant() {
        let err = Column::from_block(
            ColumnType::Numeric,
            ValueBlock::Mixed(vec![MixedCell::Empty]),
            ColumnMeta::default(),
        );
        assert!(matches!(err, Err(CoreError::TypeMismatch(_))));

        let err = Column::from_block(
            ColumnType::Mixed,
            ValueBlock::Float(vec![1.0]),
            ColumnMeta::default(),
        );
        assert!(matches!(err, Err(CoreError::TypeMismatch(_))));
    }

    #[test]
    fn set_values_overwrites_and_reports_delta_for_replay() {
        let mut column = Column::from_f64(&[1.0, 2.0, 3.0]);
        let delta = column
            .set_values(1, ValueBlock::Float(vec![20.0, 30.0]))
            .expect("écriture valide");
        assert_eq!(values_of(&column), vec![1.0, 20.0, 30.0]);
        assert_eq!(delta.start, 1);
        assert_eq!(delta.previous, ValueBlock::Float(vec![2.0, 3.0]));
        assert_eq!(delta.previous_length, 3);

        column
            .set_values(delta.start, delta.previous.clone())
            .expect("rejeu valide");
        column.set_length(delta.previous_length);
        assert_eq!(values_of(&column), vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn set_values_extends_and_fills_gap_with_missing() {
        let mut column = Column::from_f64(&[1.0]);
        let delta = column
            .set_values(3, ValueBlock::Float(vec![4.0]))
            .expect("écriture valide");
        assert_eq!(column.len(), 4);
        assert!(column.get(1).is_missing());
        assert!(column.get(2).is_missing());
        assert_eq!(column.get(3), Cell::Float(4.0));
        assert_eq!(delta.previous, ValueBlock::Float(Vec::new()));
        assert_eq!(delta.previous_length, 1);
    }

    #[test]
    fn set_values_rejects_mismatched_block_and_leaves_column_untouched() {
        let mut column = Column::from_f64(&[1.0, 2.0, 3.0]);
        let err = column.set_values(0, ValueBlock::Mixed(vec![MixedCell::Empty]));
        assert!(matches!(err, Err(CoreError::TypeMismatch(_))));
        assert_eq!(values_of(&column), vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn set_values_copies_on_write_when_a_reader_holds_the_arc() {
        let mut column = Column::from_f64(&[1.0, 2.0, 3.0]);

        // Simule une vue numpy vivante côté pont Python : refcount >= 2.
        let held = column.float_values().cloned().expect("colonne numérique");
        column
            .set_values(0, ValueBlock::Float(vec![9.0]))
            .expect("écriture valide");

        assert!(!Arc::ptr_eq(
            &held,
            column.float_values().expect("colonne numérique")
        ));
        assert_eq!(held.as_slice(), &[1.0, 2.0, 3.0]);
        assert_eq!(values_of(&column), vec![9.0, 2.0, 3.0]);
    }

    #[test]
    fn set_values_keeps_the_same_arc_without_a_concurrent_reader() {
        let mut column = Column::from_f64(&[1.0, 2.0, 3.0]);
        let before_ptr = Arc::as_ptr(column.float_values().expect("colonne numérique"));

        column
            .set_values(0, ValueBlock::Float(vec![9.0]))
            .expect("écriture valide");

        let after_ptr = Arc::as_ptr(column.float_values().expect("colonne numérique"));
        assert!(std::ptr::eq(before_ptr, after_ptr));
    }

    #[test]
    fn set_length_truncates_and_reports_removed_tail() {
        let mut column = Column::from_f64(&[1.0, 2.0, 3.0]);
        let delta = column.set_length(1);
        assert_eq!(values_of(&column), vec![1.0]);
        assert_eq!(delta.previous, ValueBlock::Float(vec![2.0, 3.0]));
        assert_eq!(delta.previous_length, 3);
    }

    #[test]
    fn set_length_extends_with_missing() {
        let mut column = Column::from_f64(&[1.0]);
        let delta = column.set_length(3);
        assert_eq!(column.len(), 3);
        assert!(column.get(1).is_missing());
        assert!(column.get(2).is_missing());
        assert_eq!(delta.previous, ValueBlock::Float(Vec::new()));
    }

    #[test]
    fn set_length_to_same_length_is_a_no_op() {
        let mut column = Column::from_f64(&[1.0, 2.0]);
        let delta = column.set_length(2);
        assert_eq!(delta.previous, ValueBlock::Float(Vec::new()));
        assert_eq!(column.len(), 2);
    }

    #[test]
    fn insert_then_delete_rows_round_trips() {
        let mut column = Column::from_f64(&[1.0, 2.0, 3.0]);
        column.insert_rows(1, 2).expect("insertion valide");
        assert_eq!(column.len(), 5);
        assert!(column.get(1).is_missing());
        assert!(column.get(2).is_missing());
        assert_eq!(column.get(3), Cell::Float(2.0));

        let removed = column.delete_rows(1, 2).expect("suppression valide");
        match removed {
            ValueBlock::Float(values) => {
                assert_eq!(values.len(), 2);
                assert!(values.iter().all(|value| value.is_nan()));
            }
            ValueBlock::Mixed(_) => panic!("bloc attendu de type Float"),
        }
        assert_eq!(values_of(&column), vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn delete_rows_clamps_to_end_of_column() {
        let mut column = Column::from_f64(&[1.0, 2.0]);
        let removed = column.delete_rows(1, 99).expect("suppression valide");
        assert_eq!(removed, ValueBlock::Float(vec![2.0]));
        assert_eq!(values_of(&column), vec![1.0]);
    }

    #[test]
    fn insert_rows_rejects_out_of_bounds_but_accepts_end_position() {
        let mut column = Column::from_f64(&[1.0]);
        assert!(matches!(
            column.insert_rows(5, 1),
            Err(CoreError::IndexOutOfBounds(_))
        ));
        column
            .insert_rows(1, 1)
            .expect("insertion en fin de colonne valide");
        assert_eq!(column.len(), 2);
    }

    #[test]
    fn delete_rows_of_zero_count_is_a_no_op() {
        let mut column = Column::from_f64(&[1.0, 2.0]);
        let removed = column.delete_rows(0, 0).expect("suppression valide");
        assert_eq!(removed, ValueBlock::Float(Vec::new()));
        assert_eq!(values_of(&column), vec![1.0, 2.0]);
    }

    #[test]
    fn text_column_insert_and_delete_rows() {
        let mut column = Column::from_text(&[Some("a".to_string()), Some("b".to_string())]);
        column.insert_rows(1, 1).expect("insertion valide");
        assert_eq!(column.get(1), Cell::Text(None));

        let removed = column.delete_rows(0, 1).expect("suppression valide");
        assert_eq!(
            removed,
            ValueBlock::Mixed(vec![MixedCell::Text("a".to_string())])
        );
    }

    #[test]
    fn set_meta_replaces_and_returns_the_previous_metadata() {
        let mut column = Column::new(ColumnType::Numeric);
        let new_meta = ColumnMeta {
            role: ColumnRole::X,
            unit: "ms".to_string(),
            ..ColumnMeta::default()
        };
        let previous = column.set_meta(new_meta.clone());
        assert_eq!(previous.unit, "");
        assert_eq!(previous.role, ColumnRole::Y);
        assert_eq!(column.meta(), &new_meta);
    }

    // `set_column_type_text_to_numeric_parses_values` a été retiré : il
    // testait `Text → Numeric` via `Column::from_text`, qui construit
    // désormais directement une colonne `Mixed` — la même conversion est
    // couverte par `mixed_to_numeric_parses_text_cells_like_a_text_column_would`
    // ci-dessous.

    #[test]
    fn mixed_cell_into_text_formats_numbers_with_the_display_form() {
        // Ancien `set_column_type_numeric_to_text_formats_values` : il n'y a
        // plus de `ColumnType::Text` vers lequel convertir, donc la même
        // couverture (format d'affichage d'un nombre, `NaN` → `None`) porte
        // directement sur `MixedCell::into_text`.
        assert_eq!(MixedCell::from_f64(1.0).into_text(), Some("1".to_string()));
        assert_eq!(MixedCell::from_f64(f64::NAN).into_text(), None);
    }

    #[test]
    fn set_column_type_to_same_type_is_a_no_op_snapshot() {
        let mut column = Column::from_f64(&[1.0]);
        let (old_type, old_values) = column.set_column_type(ColumnType::Numeric);
        assert_eq!(old_type, ColumnType::Numeric);
        assert_eq!(old_values, ColumnValues::Float(Arc::new(vec![1.0])));
        assert_eq!(values_of(&column), vec![1.0]);
    }

    #[test]
    fn set_column_type_numeric_to_datetime_keeps_values() {
        let mut column = Column::from_f64(&[1.0, 2.0]);
        column.set_column_type(ColumnType::DateTime);
        assert_eq!(column.column_type(), ColumnType::DateTime);
        assert_eq!(values_of(&column), vec![1.0, 2.0]);
        assert!(column.float_values().is_some());
    }

    #[test]
    fn restore_undoes_set_column_type_exactly() {
        let original = Column::from_f64(&[1.0, 2.0]);
        let mut column = original.clone();
        let (old_type, old_values) = column.set_column_type(ColumnType::Mixed);
        column
            .restore(old_type, old_values)
            .expect("restauration valide");
        assert_eq!(column, original);

        let err = column.restore(
            ColumnType::Numeric,
            ColumnValues::Mixed(Arc::new(vec![MixedCell::Empty])),
        );
        assert!(matches!(err, Err(CoreError::TypeMismatch(_))));
    }

    #[test]
    fn float_equality_is_bit_for_bit_not_ieee() {
        // Deux colonnes identiques, `NaN` compris, sont égales.
        assert_eq!(
            Column::from_f64(&[1.0, f64::NAN]),
            Column::from_f64(&[1.0, f64::NAN])
        );
        assert_ne!(
            Column::from_f64(&[1.0, f64::NAN]),
            Column::from_f64(&[1.0, 2.0])
        );

        assert_eq!(Cell::Float(f64::NAN), Cell::Float(f64::NAN));
        assert_eq!(
            ValueBlock::Float(vec![f64::NAN]),
            ValueBlock::Float(vec![f64::NAN])
        );
        assert_eq!(
            ColumnValues::Float(Arc::new(vec![f64::NAN])),
            ColumnValues::Float(Arc::new(vec![f64::NAN]))
        );

        // Motif binaire distinct : `0.0` et `-0.0` restent différents.
        assert_ne!(Cell::Float(0.0), Cell::Float(-0.0));

        // Les types texte gardent une comparaison normale, non affectée.
        assert_eq!(
            Cell::Text(Some("a".to_string())),
            Cell::Text(Some("a".to_string()))
        );
        assert_ne!(Cell::Text(None), Cell::Text(Some(String::new())));
    }

    #[test]
    fn version_starts_at_zero_and_advances_on_every_data_mutator() {
        let mut column = Column::from_f64(&[1.0, 2.0, 3.0]);
        assert_eq!(column.version(), 0);

        column.set_values(0, ValueBlock::Float(vec![9.0])).unwrap();
        assert_eq!(column.version(), 1);
        column.set_length(2);
        assert_eq!(column.version(), 2);
        column.insert_rows(0, 1).unwrap();
        assert_eq!(column.version(), 3);
        column.delete_rows(0, 1).unwrap();
        assert_eq!(column.version(), 4);

        let (old_type, old_values) = column.set_column_type(ColumnType::Mixed);
        assert_eq!(column.version(), 5);
        column.restore(old_type, old_values).unwrap();
        assert_eq!(column.version(), 6);
    }

    #[test]
    fn version_advances_even_when_the_call_turns_out_to_be_a_no_op() {
        // Le sens de l'approximation est délibéré : un faux « a changé »
        // coûte un recalcul, un faux « n'a pas changé » afficherait un
        // résultat faux.
        let mut column = Column::from_f64(&[1.0, 2.0]);
        column.insert_rows(0, 0).unwrap();
        assert_eq!(column.version(), 1);
        column.set_length(2);
        assert_eq!(column.version(), 2);
        column.delete_rows(0, 0).unwrap();
        assert_eq!(column.version(), 3);
        assert_eq!(values_of(&column), vec![1.0, 2.0]);
    }

    #[test]
    fn version_ignores_metadata_and_refused_calls() {
        let mut column = Column::from_f64(&[1.0, 2.0]);

        column.set_meta(ColumnMeta {
            unit: "ms".to_string(),
            ..ColumnMeta::default()
        });
        assert_eq!(column.version(), 0, "une unité n'est pas une donnée");

        assert!(column
            .set_values(0, ValueBlock::Mixed(vec![MixedCell::Empty]))
            .is_err());
        assert_eq!(column.version(), 0, "un appel refusé n'écrit rien");

        assert!(column.insert_rows(99, 1).is_err());
        assert_eq!(column.version(), 0, "un appel hors bornes n'écrit rien");

        assert!(column
            .restore(
                ColumnType::Numeric,
                ColumnValues::Mixed(Arc::new(vec![MixedCell::Empty]))
            )
            .is_err());
        assert_eq!(column.version(), 0);
    }

    #[test]
    fn equality_ignores_the_version_counter() {
        // C'est ce qui permet à la propriété « après undo, l'arbre est bit à
        // bit celui d'avant » de tenir : annuler est une écriture de plus.
        let original = Column::from_f64(&[1.0, 2.0]);
        let mut column = original.clone();

        let delta = column
            .set_values(0, ValueBlock::Float(vec![9.0]))
            .expect("écriture valide");
        column.set_values(delta.start, delta.previous).unwrap();
        column.set_length(delta.previous_length);

        assert_eq!(column.version(), 3);
        assert_eq!(original.version(), 0);
        assert_eq!(column, original, "mêmes données, compteurs différents");
    }

    #[test]
    fn from_block_with_version_restores_an_imposed_counter() {
        let column = Column::from_block_with_version(
            ColumnType::Numeric,
            ValueBlock::Float(vec![1.0]),
            ColumnMeta::default(),
            47,
        )
        .expect("bloc valide");
        assert_eq!(column.version(), 47);
        assert_eq!(
            Column::from_block(
                ColumnType::Numeric,
                ValueBlock::Float(vec![1.0]),
                ColumnMeta::default()
            )
            .unwrap()
            .version(),
            0
        );
    }

    // -- colonnes à types mixtes -----------------------------------------

    fn mixed_cells(column: &Column) -> Vec<MixedCell> {
        (0..column.len()).map(|row| column.cell(row)).collect()
    }

    #[test]
    fn a_mixed_column_holds_a_different_type_on_every_row() {
        let column = Column::from_mixed(&[
            MixedCell::Number(1.2),
            MixedCell::Text("n/a".to_string()),
            MixedCell::Empty,
            MixedCell::Number(1.5),
        ]);
        assert_eq!(column.column_type(), ColumnType::Mixed);
        assert_eq!(column.len(), 4);
        assert_eq!(column.cell(0), MixedCell::Number(1.2));
        assert_eq!(column.cell(1), MixedCell::Text("n/a".to_string()));
        assert_eq!(column.cell(2), MixedCell::Empty);
        // Hors bornes : vide, jamais une erreur.
        assert_eq!(column.cell(99), MixedCell::Empty);
    }

    #[test]
    fn a_mixed_column_has_no_contiguous_float_buffer_but_a_numeric_view() {
        let column = Column::from_mixed(&[
            MixedCell::Number(1.2),
            MixedCell::Text("n/a".to_string()),
            MixedCell::Empty,
        ]);
        assert!(
            column.float_values().is_none(),
            "pas de buffer f64 contigu, donc pas de vue numpy sans copie"
        );
        let view = column
            .numeric_view()
            .expect("une colonne mixte est traçable");
        assert_eq!(view[0], 1.2);
        assert!(view[1].is_nan(), "le texte ne se trace pas");
        assert!(view[2].is_nan(), "la cellule vide ne se trace pas");
    }

    #[test]
    fn numeric_view_is_zero_copy_for_homogeneous_columns_only() {
        let numeric = Column::from_f64(&[1.0, 2.0]);
        let shared = numeric.numeric_view().expect("colonne numérique traçable");
        assert!(
            Arc::ptr_eq(&shared, numeric.float_values().expect("buffer f64")),
            "aucune copie pour une colonne numérique"
        );
        let text_only = Column::from_text(&[Some("a".to_string())]);
        let view = text_only
            .numeric_view()
            .expect("une colonne mixte reste traçable (copie neuve)");
        assert!(
            view[0].is_nan(),
            "une cellule texte ne trace rien, mais la colonne n'est pas refusée"
        );
        assert!(
            text_only.float_values().is_none(),
            "pas de buffer f64 contigu pour une colonne mixte"
        );
    }

    #[test]
    fn mixed_cells_distinguish_empty_from_an_empty_string_and_from_nan() {
        let column = Column::from_mixed(&[
            MixedCell::Empty,
            MixedCell::Text(String::new()),
            MixedCell::Number(f64::NAN),
        ]);
        // Stockées distinctement...
        assert_ne!(column.cell(0), column.cell(1));
        assert_ne!(column.cell(0), column.cell(2));
        // ... mais toutes les trois « manquantes » à l'affichage.
        assert!((0..3).all(|row| column.cell(row).is_missing()));
        assert!((0..3).all(|row| column.is_missing(row)));
    }

    #[test]
    fn writing_a_mixed_block_reports_a_replayable_delta() {
        let mut column = Column::from_mixed(&[
            MixedCell::Number(1.0),
            MixedCell::Number(2.0),
            MixedCell::Number(3.0),
        ]);
        let delta = column
            .set_values(
                1,
                ValueBlock::Mixed(vec![MixedCell::Text("oups".to_string()), MixedCell::Empty]),
            )
            .expect("écriture valide");
        assert_eq!(
            mixed_cells(&column),
            vec![
                MixedCell::Number(1.0),
                MixedCell::Text("oups".to_string()),
                MixedCell::Empty,
            ]
        );
        assert_eq!(
            delta.previous,
            ValueBlock::Mixed(vec![MixedCell::Number(2.0), MixedCell::Number(3.0)])
        );

        column
            .set_values(delta.start, delta.previous.clone())
            .expect("rejeu valide");
        column.set_length(delta.previous_length);
        assert_eq!(
            mixed_cells(&column),
            vec![
                MixedCell::Number(1.0),
                MixedCell::Number(2.0),
                MixedCell::Number(3.0),
            ]
        );
    }

    #[test]
    fn a_mixed_column_refuses_a_float_block() {
        let mut column = Column::from_mixed(&[MixedCell::Number(1.0)]);
        assert!(matches!(
            column.set_values(0, ValueBlock::Float(vec![2.0])),
            Err(CoreError::TypeMismatch(_))
        ));
        assert_eq!(column.version(), 0, "un appel refusé n'écrit rien");
    }

    #[test]
    fn mixed_rows_insert_and_delete_with_empty_as_the_missing_value() {
        let mut column =
            Column::from_mixed(&[MixedCell::Number(1.0), MixedCell::Text("a".to_string())]);
        column.insert_rows(1, 1).expect("insertion valide");
        assert_eq!(column.cell(1), MixedCell::Empty);
        assert_eq!(column.len(), 3);

        let removed = column.delete_rows(0, 2).expect("suppression valide");
        assert_eq!(
            removed,
            ValueBlock::Mixed(vec![MixedCell::Number(1.0), MixedCell::Empty])
        );
        assert_eq!(mixed_cells(&column), vec![MixedCell::Text("a".to_string())]);
    }

    #[test]
    fn set_length_extends_a_mixed_column_with_empty_cells() {
        let mut column = Column::from_mixed(&[MixedCell::Number(1.0)]);
        let delta = column.set_length(3);
        assert_eq!(column.len(), 3);
        assert_eq!(column.cell(2), MixedCell::Empty);
        assert_eq!(delta.previous, ValueBlock::Mixed(Vec::new()));

        let delta = column.set_length(1);
        assert_eq!(
            delta.previous,
            ValueBlock::Mixed(vec![MixedCell::Empty, MixedCell::Empty])
        );
    }

    #[test]
    fn mixed_cell_from_text_promotes_what_parses_and_keeps_the_rest() {
        // Ancien `text_to_mixed_promotes_what_parses_and_keeps_the_rest` : la
        // promotion « texte saisi → cellule mixte » n'a plus de type `Text`
        // homogène en amont (`Column::from_text` construit directement du
        // `Mixed`, cellule par cellule, sans réanalyse — voir sa doc), donc
        // la couverture porte directement sur `MixedCell::from_text`, le
        // point d'entrée réel de cette promotion (saisie dans une colonne
        // mixte, `stokesforge-app::state`).
        assert_eq!(
            MixedCell::from_text(Some("1.2".to_string())),
            MixedCell::Number(1.2)
        );
        assert_eq!(
            MixedCell::from_text(Some("n/a".to_string())),
            MixedCell::Text("n/a".to_string())
        );
        assert_eq!(MixedCell::from_text(None), MixedCell::Empty);
        assert_eq!(
            MixedCell::from_text(Some(String::new())),
            MixedCell::Text(String::new()),
            "une chaîne vide délibérée reste un texte, pas une cellule vide"
        );
    }

    #[test]
    fn numeric_to_mixed_is_lossless_and_maps_nan_to_empty() {
        let mut column = Column::from_f64(&[1.0, f64::NAN, -0.0]);
        column.set_column_type(ColumnType::Mixed);
        assert_eq!(column.cell(0), MixedCell::Number(1.0));
        assert_eq!(column.cell(1), MixedCell::Empty);
        assert_eq!(column.cell(2), MixedCell::Number(-0.0));

        // Et retour : la valeur manquante refait un `NaN`.
        column.set_column_type(ColumnType::Numeric);
        assert_eq!(column.get(0), Cell::Float(1.0));
        assert!(column.get(1).is_missing());
        assert_eq!(column.get(2), Cell::Float(-0.0));
    }

    #[test]
    fn mixed_to_numeric_parses_text_cells_like_a_text_column_would() {
        let mut column = Column::from_mixed(&[
            MixedCell::Number(1.5),
            MixedCell::Text("3".to_string()),
            MixedCell::Text("abc".to_string()),
            MixedCell::Empty,
        ]);
        column.set_column_type(ColumnType::Numeric);
        assert_eq!(column.column_type(), ColumnType::Numeric);
        assert_eq!(column.get(0), Cell::Float(1.5));
        assert_eq!(column.get(1), Cell::Float(3.0));
        assert!(column.get(2).is_missing());
        assert!(column.get(3).is_missing());
    }

    #[test]
    fn mixed_cell_into_text_reads_each_cell_kind() {
        // Ancien `mixed_to_text_writes_numbers_with_the_display_form`, qui
        // passait par `set_column_type(ColumnType::Text)` — plus de cible
        // `Text` pour un type de colonne, donc la même couverture porte
        // directement sur `MixedCell::into_text`.
        assert_eq!(MixedCell::Number(1.0).into_text(), Some("1".to_string()));
        assert_eq!(
            MixedCell::Text("n/a".to_string()).into_text(),
            Some("n/a".to_string())
        );
        assert_eq!(MixedCell::Empty.into_text(), None);
    }

    #[test]
    fn restore_undoes_a_conversion_from_mixed_to_numeric_exactly() {
        let original =
            Column::from_mixed(&[MixedCell::Number(1.2), MixedCell::Text("n/a".to_string())]);
        let mut column = original.clone();
        let (old_type, old_values) = column.set_column_type(ColumnType::Numeric);
        assert_ne!(column, original);
        column
            .restore(old_type, old_values)
            .expect("restauration valide");
        assert_eq!(column, original);

        // Et le refus symétrique : un type qui ne correspond pas aux valeurs.
        assert!(matches!(
            column.restore(ColumnType::Mixed, ColumnValues::Float(Arc::new(vec![0.0]))),
            Err(CoreError::TypeMismatch(_))
        ));
    }

    #[test]
    fn a_block_knows_which_column_type_it_fits() {
        assert!(ValueBlock::Float(vec![1.0]).fits(ColumnType::Numeric));
        assert!(ValueBlock::Float(vec![1.0]).fits(ColumnType::DateTime));
        assert!(!ValueBlock::Float(vec![1.0]).fits(ColumnType::Mixed));
        assert!(ValueBlock::Mixed(vec![MixedCell::Empty]).fits(ColumnType::Mixed));
        assert!(!ValueBlock::Mixed(vec![MixedCell::Empty]).fits(ColumnType::Numeric));
    }

    #[test]
    fn into_mixed_follows_the_same_rules_as_the_type_conversion() {
        assert_eq!(
            ValueBlock::Float(vec![1.0, f64::NAN]).into_mixed(),
            vec![MixedCell::Number(1.0), MixedCell::Empty]
        );
        // Un bloc déjà mixte traverse tel quel, sans réanalyse : un texte
        // numérique délibérément stocké comme texte le reste.
        assert_eq!(
            ValueBlock::Mixed(vec![MixedCell::Text("2".to_string())]).into_mixed(),
            vec![MixedCell::Text("2".to_string())]
        );
    }

    #[test]
    fn a_mixed_cell_never_reparses_the_text_it_stores() {
        assert!(MixedCell::Text("42".to_string()).as_number().is_nan());
        assert_eq!(MixedCell::Number(42.0).as_number(), 42.0);
        assert!(MixedCell::Empty.as_number().is_nan());
    }

    #[test]
    fn reading_a_mixed_cell_through_the_common_cell_vocabulary() {
        let column = Column::from_mixed(&[
            MixedCell::Number(1.5),
            MixedCell::Text("a".to_string()),
            MixedCell::Empty,
        ]);
        assert_eq!(column.get(0), Cell::Float(1.5));
        assert_eq!(column.get(1), Cell::Text(Some("a".to_string())));
        assert_eq!(
            column.get(2),
            Cell::Text(None),
            "une cellule vide se lit comme une valeur absente"
        );
    }

    #[test]
    fn number_at_reads_without_allocating_for_every_column_type() {
        let mixed = Column::from_mixed(&[MixedCell::Number(7.0), MixedCell::Text("x".to_string())]);
        assert_eq!(mixed.values().number_at(0), 7.0);
        assert!(mixed.values().number_at(1).is_nan());
        assert!(mixed.values().number_at(99).is_nan());
        assert_eq!(Column::from_f64(&[3.0]).values().number_at(0), 3.0);
        assert!(Column::from_text(&[Some("3".to_string())])
            .values()
            .number_at(0)
            .is_nan());
    }

    #[test]
    fn value_block_helpers_build_missing_and_empty_blocks() {
        match ValueBlock::missing_for(ColumnType::Numeric, 2) {
            ValueBlock::Float(values) => {
                assert_eq!(values.len(), 2);
                assert!(values.iter().all(|value| value.is_nan()));
            }
            ValueBlock::Mixed(_) => panic!("bloc attendu de type Float"),
        }
        assert_eq!(
            ValueBlock::missing_for(ColumnType::Mixed, 2),
            ValueBlock::Mixed(vec![MixedCell::Empty, MixedCell::Empty])
        );
        assert!(ValueBlock::empty_for(ColumnType::Numeric).is_empty());
        assert!(ValueBlock::empty_for(ColumnType::Mixed).is_empty());
    }
}
