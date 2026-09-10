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
