//! Structures de transport (« schéma sur disque »), privées au crate.
//!
//! Ce module ne connaît **pas** `stokesforge-core` au sens inverse : c'est
//! `convert.rs` qui sait construire un `NodeRecord` depuis un `Node` et
//! réciproquement, jamais l'inverse. Le schéma est volontairement
//! indépendant du modèle en mémoire (voir la doc de crate) : il peut évoluer
//! (ajout de champs `#[serde(default)]`) sans jamais dériver `Serialize` sur
//! les types de `stokesforge-core`.
//!
//! Toutes ces structures sont sérialisées **comme des maps nommées**
//! (`rmp_serde::to_vec_named`), ce qui rend le fichier auto-descriptif : un
//! champ absent à la lecture prend sa valeur par défaut (`#[serde(default)]`,
//! à ajouter sur tout champ créé après la version 1) et un champ inconnu à la
//! lecture est silencieusement ignoré — c'est le mécanisme qui permettra
//! d'ajouter des données (par exemple une spécification de graphe) sans
//! casser les fichiers déjà écrits.
//!
//! Ce mécanisme ne couvrait, jusqu'ici, que les **champs** — pas les
//! **variantes** des enums externally-tagged (`KindRecord`, `PlotTypeRecord`,
//! …) : une variante inconnue faisait échouer tout l'enum qui la porte, donc
//! tout le nœud, donc tout le fichier (C5, niveau 2,
//! `guide-de-correction-robustesse.md` §F3). `KindRecord` et `PlotTypeRecord`
//! écrivent maintenant `Serialize`/`Deserialize` à la main (voir
//! `unknown.rs`) pour capturer une variante inconnue dans une variante
//! `Unknown` plutôt que de propager l'échec.

use serde::{Deserialize, Serialize};
use stokesforge_plot::RawVariant;

use crate::unknown::{deserialize_with_fallback, serialize_raw_variant};

/// Signature de fichier : les quatre premiers octets, avant même le numéro
/// de version.
pub(crate) const MAGIC: &[u8; 4] = b"STKF";

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
    /// Couleurs enregistrées dans le projet, par usage
    /// (`stokesforge_plot::SavedColors`, portées par l'arbre). Absentes des
    /// fichiers écrits avant leur ajout : `#[serde(default)]` les ramène à
    /// des listes vides. Placé avant `window_geometries` et `save_counter`,
    /// pour la même raison qu'eux (voir ci-dessous).
    #[serde(default)]
    pub(crate) saved_colors: SavedColorsRecord,
    /// Calculs d'analyse (#181). Absents des fichiers antérieurs : liste
    /// vide. Placé avant `window_geometries` et `save_counter`, pour la même
    /// raison que `saved_colors`.
    #[serde(default)]
    pub(crate) calculations: Vec<crate::calculations::CalculationRecord>,
    #[serde(default)]
    pub(crate) correction_traces: Vec<crate::calculations::CalculationRecord>,
    /// Feuilles transposées liées (#253). Absentes des fichiers antérieurs :
    /// liste vide. Placé avant `window_geometries` et `save_counter`, pour la
    /// même raison que `calculations`.
    #[serde(default)]
    pub(crate) transposes: Vec<crate::transposes::TransposeRecord>,
    /// Géométrie des fenêtres (position/taille), par identifiant de nœud —
    /// bloc **séparé de l'arbre annulable** (voir la doc de tête de
    /// `stokesforge-session` : la géométrie ne vit pas dans `Project`, ne passe
    /// jamais par une commande, et n'apparaît jamais dans `nodes` ci-dessus).
    /// Absent des fichiers écrits avant l'ajout de ce bloc :
    /// `#[serde(default)]` le ramène alors à une table vide, ce qui est le
    /// comportement voulu — un ancien fichier ne décrivait aucune position de
    /// fenêtre, il n'y a rien à retrouver. Les clés sont des `NodeId` bruts,
    /// comme partout ailleurs dans ce schéma (`root`, `parent`, …) ; une clé
    /// qui ne correspond à aucun nœud de `nodes` (fichier corrompu, ou nœud
    /// supprimé par un outil tiers sans purger sa géométrie) est filtrée
    /// silencieusement au chargement plutôt que de faire échouer tout le
    /// fichier (invariant 10 : un défaut de donnée dégrade, il ne plante
    /// pas) — voir `load_with_geometries`. Placé **avant** `save_counter`
    /// ci-dessous : `tests/roundtrip.rs` (`saving_over_an_existing_valid_file_replaces_it_atomically`)
    /// tient à ce que seul le dernier octet du fichier change entre deux
    /// sauvegardes successives, ce qui suppose que `save_counter` reste le
    /// tout dernier champ sérialisé.
    #[serde(default)]
    pub(crate) window_geometries: std::collections::HashMap<u64, crate::WindowGeometry>,
    /// Largeur des colonnes d'une feuille (#325), en points logiques, par
    /// identifiant de colonne — même statut que `window_geometries` : bloc
    /// **hors de l'arbre annulable**, absent des fichiers antérieurs
    /// (`#[serde(default)]` : table vide, les colonnes prennent la largeur par
    /// défaut), clés `NodeId` brutes bornées par `check_node_id`. À la lecture,
    /// une clé sans nœud et une valeur qui n'est pas un réel fini strictement
    /// positif sont écartées une à une (invariant 10). Placé **avant**
    /// `save_counter`, qui reste le dernier champ sérialisé (voir ci-dessus).
    #[serde(default)]
    pub(crate) column_widths: std::collections::HashMap<u64, f32>,
    /// Compteur de sauvegarde du fichier (B8,
    /// `guide-de-correction-robustesse.md` §F2) : incrémenté à chaque `save`
    /// réussi. C'est ce qui permet à `save` de détecter qu'un autre écrivain
    /// a modifié le fichier depuis le dernier chargement/sauvegarde de ce
    /// processus, sans confondre deux instances distinctes chargées du même
    /// fichier. Absent des fichiers écrits avant le 2026-09-11 :
    /// `#[serde(default)]` le ramène alors à 0, ce qui est le comportement
    /// voulu — un fichier sans compteur n'a de toute façon aucun lecteur qui
    /// en retienne un.
    #[serde(default)]
    pub(crate) save_counter: u64,
}

/// [`stokesforge_plot::SavedColors`], à plat. Chaque liste est bornée à la
/// lecture (`SavedColors::bounded`), un fichier n'étant pas borné.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub(crate) struct SavedColorsRecord {
    #[serde(default)]
    pub(crate) curves: Vec<String>,
    #[serde(default)]
    pub(crate) text: Vec<String>,
    #[serde(default)]
    pub(crate) backgrounds: Vec<String>,
    /// Nuanciers enregistrés (#178). Absents des fichiers antérieurs.
    #[serde(default)]
    pub(crate) gradients: Vec<SavedGradientRecord>,
}

/// [`stokesforge_plot::SavedGradient`], à plat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SavedGradientRecord {
    pub(crate) name: String,
    pub(crate) stops: Vec<String>,
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
///
/// `Serialize`/`Deserialize` sont écrits à la main, pas dérivés (C5, niveau
/// 2, `guide-de-correction-robustesse.md` §F3 — voir `unknown.rs` pour le
/// mécanisme) : une variante que ce lecteur ne connaît pas devient
/// [`KindRecord::Unknown`] au lieu de faire échouer tout le fichier.
#[derive(Debug, Clone, PartialEq)]
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
        hidden: bool,
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
        /// des fichiers écrits avant le 2026-09-10 : `KindRecordKnown` (plus
        /// bas) ramène ce champ à 0 via `#[serde(default)]`, ce qui est le
        /// comportement voulu — un fichier sans compteur n'a de toute façon
        /// aucun lecteur qui en retienne un.
        version: u64,
        /// Formule de calcul (`ColumnMeta::formula`), `None` pour une colonne
        /// ordinaire : fragments de texte et **identifiants** de colonnes lues
        /// (#321), jamais des lettres de position. Champ `formula_parts`,
        /// absent des fichiers antérieurs : `KindRecordKnown` le ramène à
        /// `None` via `#[serde(default)]`. L'ancien champ `formula` (texte en
        /// lettres, d'avant #321) n'est plus lu — le format n'a pas à rester
        /// compatible en développement : la colonne se charge avec ses valeurs
        /// et sans formule. Les valeurs présentes dans `values` ci-dessus sont
        /// traitées comme un cache : `stokesforge-session::actor` déclenche un
        /// recalcul après tout chargement (voir sa doc de tête), donc un
        /// désaccord entre un fichier écrit par un tiers et ce que la formule
        /// recalculerait ne survit pas au premier `load`.
        formula_parts: Option<Vec<FormulaPartRecord>>,
        /// Métadonnées du fichier d'origine (`ColumnMeta::file_metadata`),
        /// paires clé / valeur dans l'ordre. Absentes des fichiers écrits
        /// avant ce champ : `KindRecordKnown` les ramène à une liste vide.
        file_metadata: Vec<(String, String)>,
        /// Source d'import (`ColumnMeta::import_source`) : chemin, recette en
        /// JSON, index de colonne. Absente des fichiers antérieurs.
        import_source: Option<(String, String, u64, u64)>,
        /// Valeur de la ligne spéciale x' (`ColumnMeta::x_prime`, #164).
        /// Absente des fichiers antérieurs : `None`.
        x_prime: Option<f64>,
    },
    /// Graphe.
    Graph {
        hidden: bool,
        /// Étiquette de fenêtre.
        label: String,
        /// Commentaire de fenêtre.
        comment: String,
        /// Description du graphe (`PlotSpec`). Absente des fichiers écrits
        /// avant la phase R3 : `KindRecordKnown` ramène ce champ à
        /// [`PlotSpecRecord::default`] via `#[serde(default)]`, qui convertit
        /// vers le `PlotSpec::default()` de `stokesforge-plot` (un panneau,
        /// aucune série) — exactement ce qu'un fichier sans spec décrivait
        /// déjà en mémoire avant cette phase.
        spec: PlotSpecRecord,
    },
    /// Matrice.
    Matrix {
        hidden: bool,
        /// Étiquette de fenêtre.
        label: String,
        /// Commentaire de fenêtre.
        comment: String,
    },
    /// Note.
    Note {
        hidden: bool,
        /// Étiquette de fenêtre.
        label: String,
        /// Commentaire de fenêtre.
        comment: String,
        /// Texte de la note.
        text: String,
    },
    /// Nature de nœud qu'aucune des variantes ci-dessus ne nomme : une
    /// version plus récente de StokesForge a écrit ce fichier. Nom et contenu
    /// bruts, conservés pour une ré-écriture fidèle.
    Unknown(RawVariant),
}

/// Miroir strict de [`KindRecord`], sans [`KindRecord::Unknown`] : c'est le
/// schéma qu'un fichier écrit par ce lecteur peut produire. `unknown.rs`
/// l'utilise pour *essayer* le décodage strict avant de se rabattre sur
/// [`KindRecord::Unknown`] — voir la doc de tête de ce module.
#[derive(Debug, Clone, PartialEq, Deserialize)]
enum KindRecordKnown {
    Folder {
        children: Vec<u64>,
    },
    Workbook {
        sheets: Vec<u64>,
        #[serde(default)]
        hidden: bool,
        label: String,
        comment: String,
    },
    Worksheet {
        columns: Vec<u64>,
    },
    Column {
        column_type: TypeRecord,
        role: RoleRecord,
        unit: String,
        long_name: String,
        comment: String,
        display_format: String,
        values: ValuesRecord,
        #[serde(default)]
        version: u64,
        #[serde(default)]
        formula_parts: Option<Vec<FormulaPartRecord>>,
        #[serde(default)]
        file_metadata: Vec<(String, String)>,
        #[serde(default)]
        import_source: Option<(String, String, u64, u64)>,
        #[serde(default)]
        x_prime: Option<f64>,
    },
    Graph {
        #[serde(default)]
        hidden: bool,
        label: String,
        comment: String,
        #[serde(default)]
        spec: PlotSpecRecord,
    },
    Matrix {
        #[serde(default)]
        hidden: bool,
        label: String,
        comment: String,
    },
    Note {
        #[serde(default)]
        hidden: bool,
        label: String,
        comment: String,
        text: String,
    },
}

impl From<KindRecordKnown> for KindRecord {
    fn from(value: KindRecordKnown) -> Self {
        match value {
            KindRecordKnown::Folder { children } => KindRecord::Folder { children },
            KindRecordKnown::Workbook {
                sheets,
                hidden,
                label,
                comment,
            } => KindRecord::Workbook {
                sheets,
                hidden,
                label,
                comment,
            },
            KindRecordKnown::Worksheet { columns } => KindRecord::Worksheet { columns },
            KindRecordKnown::Column {
                column_type,
                role,
                unit,
                long_name,
                comment,
                display_format,
                values,
                version,
                formula_parts,
                file_metadata,
                import_source,
                x_prime,
            } => KindRecord::Column {
                column_type,
                role,
                unit,
                long_name,
                comment,
                display_format,
                values,
                version,
                formula_parts,
                file_metadata,
                import_source,
                x_prime,
            },
            KindRecordKnown::Graph {
                hidden,
                label,
                comment,
                spec,
            } => KindRecord::Graph {
                hidden,
                label,
                comment,
                spec,
            },
            KindRecordKnown::Matrix {
                hidden,
                label,
                comment,
            } => KindRecord::Matrix {
                hidden,
                label,
                comment,
            },
            KindRecordKnown::Note {
                hidden,
                label,
                comment,
                text,
            } => KindRecord::Note {
                hidden,
                label,
                comment,
                text,
            },
        }
    }
}

impl From<RawVariant> for KindRecord {
    fn from(value: RawVariant) -> Self {
        KindRecord::Unknown(value)
    }
}

impl<'de> Deserialize<'de> for KindRecord {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_with_fallback::<D, KindRecordKnown, KindRecord>(deserializer)
    }
}

/// Un fragment d'une formule de colonne (`stokesforge_core::FormulaPart`) :
/// du texte saisi, ou l'identifiant brut de la colonne lue (#321).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) enum FormulaPartRecord {
    /// Opérateurs, nombres, espaces, parenthèses, tels que saisis.
    Text(String),
    /// Identifiant d'une colonne lue ; borné à la lecture (`MAX_NODE_ID`).
    Column(u64),
}

/// Miroir de [`KindRecord`] **par référence** : sérialise une variante
/// connue sans en cloner le contenu — en particulier les octets d'une
/// colonne `Float`, potentiellement gros, que `Serialize` n'a besoin que de
/// lire.
#[derive(Serialize)]
enum KindRecordRef<'a> {
    Folder {
        children: &'a [u64],
    },
    Workbook {
        sheets: &'a [u64],
        hidden: bool,
        label: &'a str,
        comment: &'a str,
    },
    Worksheet {
        columns: &'a [u64],
    },
    Column {
        column_type: TypeRecord,
        role: RoleRecord,
        unit: &'a str,
        long_name: &'a str,
        comment: &'a str,
        display_format: &'a str,
        values: &'a ValuesRecord,
        version: u64,
        formula_parts: &'a Option<Vec<FormulaPartRecord>>,
        file_metadata: &'a Vec<(String, String)>,
        import_source: &'a Option<(String, String, u64, u64)>,
        x_prime: Option<f64>,
    },
    Graph {
        hidden: bool,
        label: &'a str,
        comment: &'a str,
        spec: &'a PlotSpecRecord,
    },
    Matrix {
        hidden: bool,
        label: &'a str,
        comment: &'a str,
    },
    Note {
        hidden: bool,
        label: &'a str,
        comment: &'a str,
        text: &'a str,
    },
}

impl Serialize for KindRecord {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            KindRecord::Folder { children } => {
                KindRecordRef::Folder { children }.serialize(serializer)
            }
            KindRecord::Workbook {
                sheets,
                hidden,
                label,
                comment,
            } => KindRecordRef::Workbook {
                sheets,
                hidden: *hidden,
                label,
                comment,
            }
            .serialize(serializer),
            KindRecord::Worksheet { columns } => {
                KindRecordRef::Worksheet { columns }.serialize(serializer)
            }
            KindRecord::Column {
                column_type,
                role,
                unit,
                long_name,
                comment,
                display_format,
                values,
                version,
                formula_parts,
                file_metadata,
                import_source,
                x_prime,
            } => KindRecordRef::Column {
                column_type: *column_type,
                role: *role,
                unit,
                long_name,
                comment,
                display_format,
                values,
                version: *version,
                formula_parts,
                file_metadata,
                import_source,
                x_prime: *x_prime,
            }
            .serialize(serializer),
            KindRecord::Graph {
                hidden,
                label,
                comment,
                spec,
            } => KindRecordRef::Graph {
                hidden: *hidden,
                label,
                comment,
                spec,
            }
            .serialize(serializer),
            KindRecord::Matrix {
                hidden,
                label,
                comment,
            } => KindRecordRef::Matrix {
                hidden: *hidden,
                label,
                comment,
            }
            .serialize(serializer),
            KindRecord::Note {
                hidden,
                label,
                comment,
                text,
            } => KindRecordRef::Note {
                hidden: *hidden,
                label,
                comment,
                text,
            }
            .serialize(serializer),
            KindRecord::Unknown(raw) => serialize_raw_variant(raw, serializer),
        }
    }
}

/// Type de données d'une colonne, à plat.
///
/// `Mixed` n'existe que depuis les colonnes à types mixtes. Un fichier qui en
/// porte une reste lisible par un lecteur **plus ancien**, mais pas
/// intégralement : ce lecteur-là ne connaît pas le nom de variante, son
/// décodage strict de `KindRecordKnown::Column` échoue, et `unknown.rs`
/// range le nœud en [`KindRecord::Unknown`] — la perte est bornée à cette
/// colonne, le reste du projet s'ouvre, et une ré-écriture la restitue à
/// l'identique (b20). C'est exactement le mécanisme de dégradation décrit en
/// tête de ce module, appliqué ici sans qu'aucun code lui soit propre.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum TypeRecord {
    /// Numérique.
    Numeric,
    /// Date-heure.
    DateTime,
    /// Texte — **legacy, lecture seule**. Le modèle en mémoire n'a plus de
    /// type `Text` homogène depuis son retrait au profit de `Mixed` ; cette
    /// variante reste dans l'enum pour qu'un fichier `.stf` écrit avant ce
    /// changement reste lisible (`convert::record_to_kind` la relit en
    /// [`crate::ColumnType`]`::Mixed`, voir `From<TypeRecord> for ColumnType`
    /// dans `convert.rs`), mais plus rien n'écrit cette étiquette.
    Text,
    /// Types mixtes : le type est porté par la cellule.
    Mixed,
}

/// Rôle de tracé d'une colonne, à plat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum RoleRecord {
    /// Abscisse.
    X,
    /// Ordonnée.
    Y,
    /// Abscisse du second calque (#167).
    X2,
    /// Ordonnée du second calque (#167).
    Y2,
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
    /// `Some(String::new())`) — **legacy, lecture seule**, pendant de
    /// [`TypeRecord::Text`] : plus rien n'écrit cette variante, mais
    /// `convert::record_to_kind` sait encore la relire (chaque `Option<String>`
    /// devient la `MixedCell` équivalente, sans réanalyse).
    Text(Vec<Option<String>>),
    /// Colonne à types mixtes, en **trois tableaux parallèles** plutôt qu'un
    /// tableau d'énumérations.
    ///
    /// La raison est la même que pour `Float` ci-dessus : `to_vec_named`
    /// sérialise une énumération par le **nom** de sa variante, donc un
    /// `Vec<MixedCellRecord>` écrirait la chaîne `"Number"` — sept octets —
    /// devant chacun des huit octets d'une valeur. Un octet d'étiquette par
    /// ligne, puis les seules valeurs qui existent, coûte moins qu'une
    /// colonne texte d'aujourd'hui et garde les `NaN` bit à bit.
    Mixed {
        /// Une étiquette par ligne, dans l'ordre : `0` = cellule vide,
        /// `1` = nombre, `2` = texte. Tout autre octet fait échouer la
        /// lecture de la colonne (`ConvertError::BadMixedTag`) : les trois
        /// tableaux ne se relisent pas si l'on ne sait pas dans lequel
        /// chercher la ligne suivante.
        #[serde(with = "serde_bytes")]
        tags: Vec<u8>,
        /// Les valeurs des lignes étiquetées `1`, dans l'ordre, huit octets
        /// little-endian chacune — même encodage que `Float`.
        #[serde(with = "serde_bytes")]
        numbers: Vec<u8>,
        /// Les valeurs des lignes étiquetées `2`, dans l'ordre.
        texts: Vec<String>,
    },
}

/// Étiquette d'une cellule vide dans [`ValuesRecord::Mixed`].
pub(crate) const MIXED_TAG_EMPTY: u8 = 0;
/// Étiquette d'une cellule numérique dans [`ValuesRecord::Mixed`].
pub(crate) const MIXED_TAG_NUMBER: u8 = 1;
/// Étiquette d'une cellule texte dans [`ValuesRecord::Mixed`].
pub(crate) const MIXED_TAG_TEXT: u8 = 2;

// -- PlotSpec (phase R3) ------------------------------------------------
//
// Même patron que `Column`/`ColumnValues`/`ValuesRecord` ci-dessus, étendu à
// un arbre de structures : un enregistrement par type de `stokesforge-plot`,
// converti à la main par `convert.rs`, jamais dérivé du modèle. Les `NodeId`
// référencés par une série voyagent en `u64`, comme ceux des listes
// d'enfants de `Folder`/`Workbook`/`Worksheet` plus haut — même
// représentation, réutilisée plutôt que réinventée.
//
// Chaque `Default` ci-dessous reproduit *à la main* celui du type
// `stokesforge-plot` correspondant : ce module ne connaît toujours pas
// `stokesforge-plot` (voir la doc de tête du fichier), donc pas de raccourci du
// genre `PlotSpecRecord::from(PlotSpec::default())`. Un test de
// `convert.rs` verrouille que les deux mondes restent d'accord.

/// [`stokesforge_plot::PlotSpec`], à plat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct PlotSpecRecord {
    pub(crate) title: String,
    /// Visibilité du titre, indépendante de son contenu (#85).
    #[serde(default = "visible_by_default")]
    pub(crate) title_visible: bool,
    pub(crate) panels: Vec<PanelSpecRecord>,
    pub(crate) background: String,
    /// Ajouté après coup : absent des fichiers plus anciens, ramené à la
    /// chaîne vide (couleur automatique) par `#[serde(default)]`.
    #[serde(default)]
    pub(crate) title_color: String,
    /// Titre déplacé (issue #53) : absent d'un fichier plus ancien, donc à sa
    /// place par défaut.
    #[serde(default)]
    pub(crate) title_anchor: Option<TextAnchorRecord>,
    /// Police et corps du titre (issue #78) : absent d'un fichier plus
    /// ancien, donc ceux du graphe.
    #[serde(default)]
    pub(crate) title_font: TextFontRecord,
    /// Avertissement des séries pas à jour (#198) : absent d'un fichier plus
    /// ancien, donc actif.
    #[serde(default = "visible_by_default")]
    pub(crate) stale_warning: bool,
    /// Séparateur décimal des graduations (#223) : absent d'un fichier plus
    /// ancien, donc le point.
    #[serde(default)]
    pub(crate) decimal_comma: bool,
    pub(crate) format_version: u32,
}

/// [`stokesforge_plot::TextFont`], à plat.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct TextFontRecord {
    #[serde(default)]
    pub(crate) family: String,
    #[serde(default)]
    pub(crate) size: Option<f64>,
    /// Largeur et alignement de la boîte (#244) ; absents d'un fichier plus ancien.
    #[serde(default)]
    pub(crate) width: Option<f64>,
    #[serde(default)]
    pub(crate) align: TextAlignRecord,
}

/// [`stokesforge_plot::TextAnchor`], à plat.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub(crate) struct TextAnchorRecord {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) axis: SeriesAxisRecord,
}

impl Default for PlotSpecRecord {
    fn default() -> Self {
        Self {
            title: String::new(),
            panels: vec![PanelSpecRecord::default()],
            background: "#ffffff00".to_string(), // stokesforge_plot::TRANSPARENT
            title_color: String::new(),
            title_visible: true,
            title_anchor: None,
            title_font: TextFontRecord::default(),
            stale_warning: true,
            decimal_comma: false,
            format_version: 1, // stokesforge_plot::SPEC_FORMAT_VERSION
        }
    }
}

/// [`stokesforge_plot::PanelSpec`], à plat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct PanelSpecRecord {
    pub(crate) title: String,
    /// Visibilité du titre, indépendante de son contenu (#85).
    #[serde(default = "visible_by_default")]
    pub(crate) title_visible: bool,
    /// Titre déplacé (issue #53), comme `PlotSpecRecord::title_anchor`.
    #[serde(default)]
    pub(crate) title_anchor: Option<TextAnchorRecord>,
    /// Police et corps du titre du panneau (issue #78).
    #[serde(default)]
    pub(crate) title_font: TextFontRecord,
    /// Couleur du titre du panneau (issue #90) ; absente d'un fichier plus
    /// ancien, donc automatique.
    #[serde(default)]
    pub(crate) title_color: String,
    pub(crate) series: Vec<SeriesSpecRecord>,
    pub(crate) bottom: AxisSpecRecord,
    pub(crate) left: AxisSpecRecord,
    pub(crate) right: AxisSpecRecord,
    pub(crate) top: AxisSpecRecord,
    pub(crate) legend: LegendSpecRecord,
    /// Calque superposé (#167) ; absent d'un fichier plus ancien.
    #[serde(default)]
    pub(crate) overlay: Option<OverlaySpecRecord>,
    pub(crate) row: u64,
    pub(crate) column: u64,
    /// Textes libres (issue #35) : absents d'un fichier plus ancien,
    /// `#[serde(default)]` les ramène à une liste vide.
    #[serde(default)]
    pub(crate) annotations: Vec<AnnotationSpecRecord>,
    /// Objets graphiques (issue #76) : absents d'un fichier plus ancien.
    #[serde(default)]
    pub(crate) shapes: Vec<ShapeSpecRecord>,
    /// Noms des groupes de séries (#197) ; absents d'un fichier plus ancien,
    /// donc tous automatiques.
    #[serde(default)]
    pub(crate) group_names: Vec<GroupNameRecord>,
    /// Nuanciers retenus par les groupes de séries (#213) ; absents d'un
    /// fichier plus ancien, donc aucun groupe n'a de nuancier.
    #[serde(default)]
    pub(crate) group_gradients: Vec<GroupGradientRecord>,
}

/// Les arrêts du nuancier d'un groupe de séries (#213), à plat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct GroupGradientRecord {
    pub(crate) group: u32,
    pub(crate) stops: Vec<String>,
}

/// Le nom d'un groupe de séries (#197), à plat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct GroupNameRecord {
    pub(crate) group: u32,
    pub(crate) name: String,
}

/// [`stokesforge_plot::OverlaySpec`], à plat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct OverlaySpecRecord {
    pub(crate) x_top: bool,
    pub(crate) y_right: bool,
}

/// [`stokesforge_plot::ShapeKind`], à plat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum ShapeKindRecord {
    Rectangle,
    Ellipse,
    Line,
    Arrow,
}

/// [`stokesforge_plot::ShapeSpec`], à plat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct ShapeSpecRecord {
    #[serde(default = "visible_by_default")]
    pub(crate) visible: bool,
    pub(crate) kind: ShapeKindRecord,
    pub(crate) x1: f64,
    pub(crate) y1: f64,
    pub(crate) x2: f64,
    pub(crate) y2: f64,
    pub(crate) axis: SeriesAxisRecord,
    #[serde(default)]
    pub(crate) line_color: String,
    pub(crate) line_width: f64,
    #[serde(default)]
    pub(crate) fill_color: String,
}

/// [`stokesforge_plot::AnnotationSpec`], à plat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct AnnotationSpecRecord {
    #[serde(default = "visible_by_default")]
    pub(crate) visible: bool,
    pub(crate) text: String,
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) axis: SeriesAxisRecord,
    pub(crate) color: String,
    pub(crate) size: f64,
    pub(crate) rotation: f64,
    /// Police du texte (issue #78) ; absente d'un fichier plus ancien.
    #[serde(default)]
    pub(crate) font: String,
    /// Largeur en fraction de la figure (#244) ; absente = une ligne.
    #[serde(default)]
    pub(crate) width: Option<f64>,
    /// Alignement dans la boîte (#244) ; absent = centré.
    #[serde(default)]
    pub(crate) align: TextAlignRecord,
}

/// [`stokesforge_plot::TextAlign`], à plat.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum TextAlignRecord {
    Left,
    #[default]
    Center,
    Right,
}

impl Default for PanelSpecRecord {
    fn default() -> Self {
        Self {
            title: String::new(),
            title_visible: true,
            title_anchor: None,
            title_font: TextFontRecord::default(),
            title_color: String::new(),
            series: Vec::new(),
            bottom: AxisSpecRecord::default(),
            left: AxisSpecRecord::default(),
            right: AxisSpecRecord::default(),
            top: AxisSpecRecord::hidden(),
            legend: LegendSpecRecord::default(),
            overlay: None,
            row: 0,
            column: 0,
            annotations: Vec::new(),
            shapes: Vec::new(),
            group_names: Vec::new(),
            group_gradients: Vec::new(),
        }
    }
}

/// [`stokesforge_plot::AxisSpec`], à plat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct AxisSpecRecord {
    pub(crate) label: String,
    /// Visibilité du titre, indépendante de son contenu (#85).
    #[serde(default = "visible_by_default")]
    pub(crate) title_visible: bool,
    pub(crate) unit: String,
    pub(crate) scale: AxisScaleRecord,
    pub(crate) auto_range: bool,
    pub(crate) range_min: Option<f64>,
    pub(crate) range_max: Option<f64>,
    pub(crate) grid: bool,
    pub(crate) visible: bool,
    pub(crate) inverted: bool,
    /// Ajouté après coup, comme `PlotSpecRecord::title_color`.
    #[serde(default)]
    pub(crate) title_color: String,
    /// Titre déplacé (issue #53), comme `PlotSpecRecord::title_anchor`.
    #[serde(default)]
    pub(crate) title_anchor: Option<TextAnchorRecord>,
    /// Police et corps du titre de l'axe (issue #78).
    #[serde(default)]
    pub(crate) title_font: TextFontRecord,
    /// Ajouté après coup, comme `PlotSpecRecord::title_color`.
    #[serde(default)]
    pub(crate) tick_color: String,
    /// Axe miroir (#176).
    #[serde(default)]
    pub(crate) mirror: bool,
    /// Libellés des graduations (#176).
    #[serde(default = "visible_by_default")]
    pub(crate) tick_labels: bool,
}

impl AxisSpecRecord {
    /// Axe masqué — la valeur par défaut de l'axe du haut d'un panneau
    /// (`AxisSpec::hidden` côté `stokesforge-plot`).
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
            title_color: String::new(),
            title_visible: true,
            title_anchor: None,
            title_font: TextFontRecord::default(),
            tick_color: String::new(),
            mirror: false,
            tick_labels: true,
        }
    }
}

/// [`stokesforge_plot::AxisScale`], à plat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum AxisScaleRecord {
    Linear,
    Log,
}

/// [`stokesforge_plot::SeriesSpec`], à plat. Les colonnes référencées voyagent en
/// `u64` brut, comme partout ailleurs dans ce schéma — un `NodeId` que la
/// colonne visée ait ou non survécu au fichier : une référence pendante
/// n'est pas une erreur de chargement, elle se découvre à la résolution
/// (`stokesforge_core::resolve`), jamais avant.
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
    /// `SeriesSpec::row` (#166). Absente des fichiers antérieurs.
    #[serde(default)]
    pub(crate) row: Option<u64>,
    /// `SeriesSpec::group` (#177). Absent des fichiers antérieurs.
    #[serde(default)]
    pub(crate) group: Option<u32>,
}

/// [`stokesforge_plot::PlotType`], à plat.
///
/// `Serialize`/`Deserialize` écrits à la main, comme [`KindRecord`] (C5,
/// niveau 2) : un type de tracé qu'une version future ajouterait devient
/// [`PlotTypeRecord::Unknown`] plutôt que de faire échouer tout le graphe qui
/// le porte — et le fichier entier avec lui, avant ce correctif.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum PlotTypeRecord {
    Scatter,
    Line,
    LineScatter,
    Bar,
    Histogram,
    ErrorBar,
    /// Type de tracé d'une version plus récente de StokesForge.
    Unknown(RawVariant),
}

/// Miroir strict de [`PlotTypeRecord`] (sans `Unknown`) — même rôle que
/// `KindRecordKnown`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum PlotTypeRecordKnown {
    Scatter,
    Line,
    LineScatter,
    Bar,
    Histogram,
    ErrorBar,
}

impl From<PlotTypeRecordKnown> for PlotTypeRecord {
    fn from(value: PlotTypeRecordKnown) -> Self {
        match value {
            PlotTypeRecordKnown::Scatter => PlotTypeRecord::Scatter,
            PlotTypeRecordKnown::Line => PlotTypeRecord::Line,
            PlotTypeRecordKnown::LineScatter => PlotTypeRecord::LineScatter,
            PlotTypeRecordKnown::Bar => PlotTypeRecord::Bar,
            PlotTypeRecordKnown::Histogram => PlotTypeRecord::Histogram,
            PlotTypeRecordKnown::ErrorBar => PlotTypeRecord::ErrorBar,
        }
    }
}

impl From<RawVariant> for PlotTypeRecord {
    fn from(value: RawVariant) -> Self {
        PlotTypeRecord::Unknown(value)
    }
}

impl<'de> Deserialize<'de> for PlotTypeRecord {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_with_fallback::<D, PlotTypeRecordKnown, PlotTypeRecord>(deserializer)
    }
}

impl Serialize for PlotTypeRecord {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let known = match self {
            PlotTypeRecord::Scatter => PlotTypeRecordKnown::Scatter,
            PlotTypeRecord::Line => PlotTypeRecordKnown::Line,
            PlotTypeRecord::LineScatter => PlotTypeRecordKnown::LineScatter,
            PlotTypeRecord::Bar => PlotTypeRecordKnown::Bar,
            PlotTypeRecord::Histogram => PlotTypeRecordKnown::Histogram,
            PlotTypeRecord::ErrorBar => PlotTypeRecordKnown::ErrorBar,
            PlotTypeRecord::Unknown(raw) => return serialize_raw_variant(raw, serializer),
        };
        known.serialize(serializer)
    }
}

/// [`stokesforge_plot::SeriesAxis`], à plat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum SeriesAxisRecord {
    Left,
    Right,
}

/// [`stokesforge_plot::LineSpec`], à plat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct LineSpecRecord {
    pub(crate) color: String,
    pub(crate) style: LineStyleRecord,
    pub(crate) width: f64,
}

/// [`stokesforge_plot::LineStyle`], à plat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum LineStyleRecord {
    None,
    Solid,
    Dash,
    Dot,
    DashDot,
}

/// [`stokesforge_plot::SymbolSpec`], à plat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct SymbolSpecRecord {
    pub(crate) shape: SymbolShapeRecord,
    pub(crate) size: f64,
    pub(crate) color: String,
    pub(crate) edge_color: String,
}

/// [`stokesforge_plot::SymbolShape`], à plat.
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

/// [`stokesforge_plot::LegendSpec`], à plat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct LegendSpecRecord {
    pub(crate) visible: bool,
    pub(crate) position: LegendPositionRecord,
    /// Légende déplacée (issue #53), comme `PlotSpecRecord::title_anchor`.
    #[serde(default)]
    pub(crate) anchor: Option<TextAnchorRecord>,
    /// Police et corps de la légende (issue #77).
    #[serde(default)]
    pub(crate) font: TextFontRecord,
    /// Couleur des noms de série (issue #90) ; absente d'un fichier plus
    /// ancien, donc automatique.
    #[serde(default)]
    pub(crate) color: String,
}

impl Default for LegendSpecRecord {
    fn default() -> Self {
        // `LegendSpec::default()` : visible, ancrée en haut à droite — pas de
        // `#[derive(Default)]` ici précisément parce que `bool::default()`
        // (`false`) contredirait cette valeur par défaut.
        Self {
            visible: true,
            position: LegendPositionRecord::default(),
            anchor: None,
            font: TextFontRecord::default(),
            color: String::new(),
        }
    }
}

/// [`stokesforge_plot::LegendPosition`], à plat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub(crate) enum LegendPositionRecord {
    TopLeft,
    #[default]
    TopRight,
    BottomLeft,
    BottomRight,
}

// Les objets des fichiers antérieurs étaient tous visibles.
fn visible_by_default() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un fichier écrit avant l'ajout des couleurs de texte (titre du
    /// graphe, titre et échelle des axes) se relit : les champs absents
    /// reviennent vides, c'est-à-dire en couleur automatique.
    #[test]
    fn un_spec_anterieur_aux_couleurs_de_texte_se_relit_en_automatique() {
        #[derive(Serialize)]
        struct AncienAxe {
            label: String,
            unit: String,
            scale: AxisScaleRecord,
            auto_range: bool,
            range_min: Option<f64>,
            range_max: Option<f64>,
            grid: bool,
            visible: bool,
            inverted: bool,
        }
        #[derive(Serialize)]
        struct AncienSpec {
            title: String,
            panels: Vec<PanelSpecRecord>,
            background: String,
            format_version: u32,
        }
        let axe = AncienAxe {
            label: "Temps".to_string(),
            unit: "s".to_string(),
            scale: AxisScaleRecord::Linear,
            auto_range: true,
            range_min: None,
            range_max: None,
            grid: false,
            visible: true,
            inverted: false,
        };
        let octets = rmp_serde::to_vec_named(&axe).expect("encodage");
        let relu: AxisSpecRecord = rmp_serde::from_slice(&octets).expect("relecture");
        assert_eq!(relu.label, "Temps");
        assert_eq!(relu.title_color, "");
        assert_eq!(relu.tick_color, "");
        assert!(relu.title_visible, "un ancien axe garde son titre visible");

        let spec = AncienSpec {
            title: "Ancien".to_string(),
            panels: vec![PanelSpecRecord::default()],
            background: "#000000".to_string(),
            format_version: 1,
        };
        let octets = rmp_serde::to_vec_named(&spec).expect("encodage");
        let relu: PlotSpecRecord = rmp_serde::from_slice(&octets).expect("relecture");
        assert_eq!(relu.title, "Ancien");
        assert!(
            relu.title_visible,
            "un ancien graphe garde son titre visible"
        );
        assert_eq!(relu.title_color, "");
        assert!(
            relu.stale_warning,
            "un ancien graphe garde l'avertissement des données pas à jour"
        );
    }
}

#[cfg(test)]
mod visibility_tests {
    use super::*;
    #[test]
    fn objets_anterieurs_sans_champ_visible_se_relisent_visibles() {
        #[derive(Serialize)]
        struct OldText {
            text: String,
            x: f64,
            y: f64,
            axis: SeriesAxisRecord,
            color: String,
            size: f64,
            rotation: f64,
        }
        #[derive(Serialize)]
        struct OldShape {
            kind: ShapeKindRecord,
            x1: f64,
            y1: f64,
            x2: f64,
            y2: f64,
            axis: SeriesAxisRecord,
            line_width: f64,
        }
        let bytes = rmp_serde::to_vec_named(&OldText {
            text: "Pic".into(),
            x: 1.0,
            y: 2.0,
            axis: SeriesAxisRecord::Left,
            color: String::new(),
            size: 16.0,
            rotation: 0.0,
        })
        .unwrap();
        let text: AnnotationSpecRecord = rmp_serde::from_slice(&bytes).unwrap();
        let bytes = rmp_serde::to_vec_named(&OldShape {
            kind: ShapeKindRecord::Rectangle,
            x1: 0.0,
            y1: 0.0,
            x2: 1.0,
            y2: 1.0,
            axis: SeriesAxisRecord::Left,
            line_width: 1.5,
        })
        .unwrap();
        let shape: ShapeSpecRecord = rmp_serde::from_slice(&bytes).unwrap();
        assert!(text.visible);
        assert!(shape.visible);
    }
}

#[cfg(test)]
mod title_visibility_tests {
    use super::*;
    #[test]
    fn titres_masques_et_anciens_champs_absents() {
        let mut record = PlotSpecRecord {
            title_visible: false,
            ..Default::default()
        };
        record.panels[0].title_visible = false;
        record.panels[0].bottom.title_visible = false;
        let bytes = rmp_serde::to_vec_named(&record).unwrap();
        let reloaded: PlotSpecRecord = rmp_serde::from_slice(&bytes).unwrap();
        assert_eq!(reloaded, record);
        let mut value = rmpv::decode::read_value(&mut std::io::Cursor::new(bytes)).unwrap();
        fn remove_field(value: &mut rmpv::Value) {
            match value {
                rmpv::Value::Map(fields) => {
                    fields.retain(|(key, _)| key.as_str() != Some("title_visible"));
                    for (_, value) in fields {
                        remove_field(value);
                    }
                }
                rmpv::Value::Array(values) => {
                    for value in values {
                        remove_field(value);
                    }
                }
                _ => {}
            }
        }
        remove_field(&mut value);
        let mut old = Vec::new();
        rmpv::encode::write_value(&mut old, &value).unwrap();
        let reloaded: PlotSpecRecord = rmp_serde::from_slice(&old).unwrap();
        assert!(reloaded.title_visible);
        assert!(reloaded.panels[0].title_visible);
        for side in [
            &reloaded.panels[0].bottom,
            &reloaded.panels[0].left,
            &reloaded.panels[0].right,
            &reloaded.panels[0].top,
        ] {
            assert!(side.title_visible);
        }
    }
}
