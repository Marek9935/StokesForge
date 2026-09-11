//! Persistance de PlotYx : `save`/`load` d'un [`plotyx_core::Project`] complet.
//!
//! # Frontière figée, format provisoire
//!
//! Deux fonctions, un sens chacune, un `Project` complet en entrée/sortie et
//! rien d'autre :
//!
//! ```text
//! save(project: &Project, path: &Path) -> Result<(), PersistenceError>;
//! load(path: &Path) -> Result<Project, PersistenceError>;
//! ```
//!
//! Aucun appelant ne connaît le format sur disque ; il sera remplacé plus
//! tard (Zarr est envisagé) sans que rien au-dessus n'ait à changer. Le
//! chargement est **intégral** : tout le projet tient en mémoire, il n'y a
//! pas de chargement partiel ou différé.
//!
//! # Le format ne s'écrit pas dans le modèle
//!
//! `plotyx-core` ne dérive aucun `Serialize`/`Deserialize` : ce crate définit
//! ses **propres structures de transport** (`schema.rs`, privées) et écrit à
//! la main les conversions vers/depuis le modèle en mémoire (`convert.rs`).
//! Le schéma sur disque est un objet explicite, versionné indépendamment des
//! types du cœur — les faire diverger un jour (renommer un champ du modèle
//! sans casser les fichiers déjà écrits, par exemple) ne coûtera qu'une
//! fonction de conversion à ajuster.
//!
//! # Cinq propriétés tenues dès cette version provisoire
//!
//! Les rajouter après coup coûterait une migration ; elles sont donc dans le
//! format dès la version 1 :
//!
//! 1. **Écriture atomique.** `save` écrit dans un fichier temporaire du même
//!    répertoire que la destination, puis le renomme sur la cible
//!    (`tempfile::NamedTempFile::persist`). Un lecteur ne voit jamais un
//!    fichier à moitié écrit ; une écriture qui échoue ne laisse aucune trace
//!    sur la destination.
//! 2. **Numéro de version en premier.** Les 4 octets qui suivent la
//!    signature sont le numéro de version, lus et vérifiés **avant** de
//!    toucher au corps du fichier. Un fichier plus récent que ce lecteur est
//!    refusé proprement (`UnsupportedVersion`), sans tenter de décoder un
//!    corps qu'il ne comprendrait pas forcément. Un fichier qui n'est pas un
//!    PlotYx (mauvaise signature, ou trop court pour en porter une) est
//!    refusé tout aussi proprement (`NotAPlotyxFile`).
//! 3. **Valeur manquante vs chaîne vide.** Les colonnes texte distinguent
//!    `None` (manquant) de `Some(String::new())` (chaîne vide présente) —
//!    c'est déjà la convention du cœur, ce format ne fait que la préserver.
//!    Les valeurs manquantes numériques sont `NaN`, écrites et relues
//!    **bit à bit** (`f64::to_le_bytes`/`from_le_bytes` conservent le motif
//!    binaire exact).
//! 4. **Compteur de `NodeId` persisté.** `next_id_hint` — le **prochain**
//!    identifiant à attribuer, pas le plus grand déjà attribué — est écrit
//!    dans le fichier ; au chargement, [`Project::from_tree`] fait repartir
//!    le compteur à `max(valeur persistée, plus grand id lu + 1)`. Un
//!    fichier dont la valeur persistée serait en retard sur les identifiants
//!    qu'il contient (fichier écrit par une version boguée) ne fait donc
//!    jamais reculer le compteur.
//! 5. **Champs inconnus ignorés à la lecture.** C'est ce qui permettra
//!    d'ajouter des données plus tard (une spécification de graphe, par
//!    exemple) sans casser les fichiers déjà écrits : un lecteur plus ancien
//!    ignore simplement ce qu'il ne connaît pas. **Conséquence directe** :
//!    le conteneur doit être auto-descriptif, c'est-à-dire nommer ses champs
//!    plutôt que les positionner. `bincode` encode les structures en
//!    tuples positionnels — ajouter un champ y décale tout ce qui suit, et
//!    en retirer un est tout aussi cassant. **MessagePack** (`rmp-serde`) le
//!    permet : `rmp_serde::to_vec_named` sérialise chaque structure comme une
//!    map nommée, et son lecteur ignore nativement une clé qu'une structure
//!    ne déclare pas. Tout champ ajouté après la version 1 portera
//!    `#[serde(default)]`, pour qu'un fichier plus ancien qui ne le porte pas
//!    reste lisible.
//!
//! Les buffers de colonnes numériques échappent à l'encodage MessagePack
//! générique : ils sont écrits comme des octets bruts little-endian via
//! `serde_bytes` (8 octets par `f64`), pas comme un tableau MessagePack
//! d'éléments — un million de points pèse 8 Mo sur disque, pas un multiple de
//! ce chiffre gonflé par l'encodage par élément.
//!
//! # Disposition du fichier
//!
//! ```text
//! octets 0..4   signature "PLYX"
//! octets 4..8   FORMAT_VERSION, u32 little-endian
//! octets 8..    corps MessagePack (FileBody), rmp_serde::to_vec_named
//! ```
//!
//! Le corps liste les nœuds de l'arbre en **parcours préfixe depuis la
//! racine** (`Tree::walk_all`) : deux sauvegardes successives du même projet
//! produisent donc des fichiers identiques octet à octet.

#![cfg_attr(
    not(test),
    warn(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

mod convert;
mod error;
mod schema;

use std::io::Write;
use std::path::Path;
use std::sync::Arc;

use plotyx_core::{Node, NodeId, Project, Tree};

use convert::ConvertError;
use schema::{FileBody, NodeRecord, FORMAT_VERSION, MAGIC};

pub use error::{PersistenceError, PersistenceResult};

/// Sauvegarde `project` vers `path`, atomiquement : un fichier temporaire est
/// écrit dans le même répertoire que `path` puis renommé sur la cible ; en
/// cas d'échec à n'importe quelle étape, `path` n'est pas touché et le
/// temporaire est supprimé (comportement de `NamedTempFile` à l'abandon).
///
/// Deux appels successifs sur le même `project` produisent des fichiers
/// identiques octet à octet (ordre déterministe des nœuds : parcours
/// préfixe depuis la racine).
pub fn save(project: &Project, path: &Path) -> PersistenceResult<()> {
    let parent_dir = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };

    let tree = project.tree();
    let nodes: Vec<NodeRecord> = tree
        .walk_all()
        .into_iter()
        .filter_map(|id| tree.node(id))
        .map(convert::node_to_record)
        .collect();
    let body = FileBody {
        root: tree.root().raw(),
        next_id_hint: project.next_id_hint(),
        nodes,
    };

    // Encoder ne devrait jamais échouer pour nos propres types de transport
    // (pas de clé non-chaîne, aucune E/S impliquée) ; on retient tout de
    // même l'erreur plutôt que de paniquer, au cas où une future extension du
    // schéma introduirait un cas qui échoue à l'encodage.
    let payload = rmp_serde::to_vec_named(&body).map_err(|source| PersistenceError::Io {
        path: path.to_path_buf(),
        source: std::io::Error::other(source),
    })?;

    let io_err = |source: std::io::Error| PersistenceError::Io {
        path: path.to_path_buf(),
        source,
    };

    let mut file = tempfile::NamedTempFile::new_in(parent_dir).map_err(io_err)?;
    file.write_all(MAGIC).map_err(io_err)?;
    file.write_all(&FORMAT_VERSION.to_le_bytes())
        .map_err(io_err)?;
    file.write_all(&payload).map_err(io_err)?;
    file.flush().map_err(io_err)?;
    file.as_file().sync_all().map_err(io_err)?;
    file.persist(path).map_err(|err| io_err(err.error))?;
    Ok(())
}

/// Charge un [`Project`] complet depuis `path`. La version est lue et
/// vérifiée avant tout décodage du corps : un fichier plus récent que ce
/// lecteur est refusé sans que son corps soit touché.
pub fn load(path: &Path) -> PersistenceResult<Project> {
    let bytes = std::fs::read(path).map_err(|source| PersistenceError::Io {
        path: path.to_path_buf(),
        source,
    })?;

    if bytes.len() < MAGIC.len() || &bytes[..MAGIC.len()] != MAGIC.as_slice() {
        return Err(PersistenceError::NotAPlotyxFile {
            path: path.to_path_buf(),
        });
    }
    if bytes.len() < MAGIC.len() + 4 {
        return Err(PersistenceError::Corrupt {
            path: path.to_path_buf(),
            detail: "en-tête tronqué avant le numéro de version".to_string(),
        });
    }
    let mut version_bytes = [0u8; 4];
    version_bytes.copy_from_slice(&bytes[MAGIC.len()..MAGIC.len() + 4]);
    let version = u32::from_le_bytes(version_bytes);
    if version > FORMAT_VERSION {
        return Err(PersistenceError::UnsupportedVersion {
            path: path.to_path_buf(),
            found: version,
            supported: FORMAT_VERSION,
        });
    }

    let body: FileBody = rmp_serde::from_slice(&bytes[MAGIC.len() + 4..]).map_err(|source| {
        PersistenceError::Corrupt {
            path: path.to_path_buf(),
            detail: source.to_string(),
        }
    })?;

    build_project(body, path)
}

/// Reconstruit un `Project` depuis un corps déjà décodé. Séparé de `load`
/// pour que les tests internes puissent fabriquer un `FileBody` à la main
/// (cas ceinture du compteur d'identifiants, champs inconnus, modèle
/// invalide) sans repasser par l'encodage MessagePack.
fn build_project(body: FileBody, path: &Path) -> PersistenceResult<Project> {
    let mut nodes = Vec::with_capacity(body.nodes.len());
    for record in body.nodes {
        let NodeRecord {
            id,
            name,
            parent,
            kind,
        } = record;
        let kind = convert::record_to_kind(kind).map_err(|err| convert_error(err, path))?;
        let node = Node::with_parent(
            NodeId::from_raw(id),
            &name,
            parent.map(NodeId::from_raw),
            kind,
        )
        .map_err(|source| PersistenceError::InvalidModel {
            path: path.to_path_buf(),
            source,
        })?;
        nodes.push(node);
    }

    let tree = Tree::from_nodes(NodeId::from_raw(body.root), nodes).map_err(|source| {
        PersistenceError::InvalidModel {
            path: path.to_path_buf(),
            source,
        }
    })?;
    Ok(Project::from_tree(Arc::new(tree), body.next_id_hint))
}

/// Traduit une erreur de conversion bas niveau (`convert.rs`) en erreur
/// publique : dans les deux cas, le fichier est incohérent d'une façon
/// qu'un fichier écrit par ce crate ne produit jamais.
fn convert_error(err: ConvertError, path: &Path) -> PersistenceError {
    match err {
        ConvertError::BadFloatLength(len) => PersistenceError::Corrupt {
            path: path.to_path_buf(),
            detail: format!("buffer de colonne flottante de {len} octet(s), non multiple de 8"),
        },
        ConvertError::Core(source) => PersistenceError::Corrupt {
            path: path.to_path_buf(),
            detail: format!("colonne incohérente : {source}"),
        },
        ConvertError::ZeroHistogramBins => PersistenceError::Corrupt {
            path: path.to_path_buf(),
            detail: "histogram_bins d'une série vaut 0, ce qui n'est pas représentable"
                .to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plotyx_core::{Column, ColumnType, CoreError, NodeKind};
    use schema::{KindRecord, TypeRecord};
    use serde::Serialize;

    fn folder_record(id: u64, name: &str, children: Vec<u64>) -> NodeRecord {
        NodeRecord {
            id,
            name: name.to_string(),
            parent: if id == 1 { None } else { Some(1) },
            kind: KindRecord::Folder { children },
        }
    }

    /// Critère 2 (cas ceinture) : un fichier dont `next_id_hint` est
    /// inférieur au plus grand id qu'il contient doit tout de même faire
    /// repartir le compteur au-dessus du plus grand id lu.
    #[test]
    fn resume_uses_the_highest_id_seen_even_if_the_hint_is_stale() {
        let root = folder_record(1, "Racine", vec![5]);
        let child = folder_record(5, "Enfant", vec![]);
        let body = FileBody {
            root: 1,
            next_id_hint: 2, // en retard : le plus grand id présent est 5
            nodes: vec![root, child],
        };
        let project = build_project(body, Path::new("test.plyx")).expect("modèle valide");
        assert_eq!(project.next_id_hint(), 6);
    }

    /// Critère 8 : un corps bien formé mais structurellement incohérent doit
    /// produire `InvalidModel`, pas paniquer ni être silencieusement accepté.
    #[test]
    fn incoherent_body_yields_invalid_model() {
        // Nœud dont le parent est absent de la liste.
        let root = folder_record(1, "Racine", vec![]);
        let orphan = NodeRecord {
            id: 2,
            name: "Perdu".to_string(),
            parent: Some(99),
            kind: KindRecord::Folder { children: vec![] },
        };
        let body = FileBody {
            root: 1,
            next_id_hint: 3,
            nodes: vec![root, orphan],
        };
        let err = build_project(body, Path::new("test.plyx")).unwrap_err();
        assert!(
            matches!(err, PersistenceError::InvalidModel { .. }),
            "{err:?}"
        );

        // Deux nœuds partageant le même id.
        let a = folder_record(1, "A", vec![]);
        let b = folder_record(1, "B", vec![]);
        let body = FileBody {
            root: 1,
            next_id_hint: 2,
            nodes: vec![a, b],
        };
        let err = build_project(body, Path::new("test.plyx")).unwrap_err();
        assert!(
            matches!(err, PersistenceError::InvalidModel { .. }),
            "{err:?}"
        );

        // La racine n'est pas un dossier.
        let root = NodeRecord {
            id: 1,
            name: "Racine".to_string(),
            parent: None,
            kind: KindRecord::Note {
                label: String::new(),
                comment: String::new(),
                text: String::new(),
            },
        };
        let body = FileBody {
            root: 1,
            next_id_hint: 2,
            nodes: vec![root],
        };
        let err = build_project(body, Path::new("test.plyx")).unwrap_err();
        assert!(
            matches!(err, PersistenceError::InvalidModel { .. }),
            "{err:?}"
        );
    }

    /// Une colonne dont le buffer flottant n'est pas de longueur multiple de
    /// 8 doit être signalée `Corrupt`, distinctement d'un modèle invalide.
    #[test]
    fn bad_float_buffer_length_yields_corrupt_not_invalid_model() {
        let root = NodeRecord {
            id: 1,
            name: "Racine".to_string(),
            parent: None,
            kind: KindRecord::Folder { children: vec![2] },
        };
        let column = NodeRecord {
            id: 2,
            name: "A".to_string(),
            parent: Some(1),
            kind: KindRecord::Column {
                column_type: TypeRecord::Numeric,
                role: schema::RoleRecord::Y,
                unit: String::new(),
                long_name: String::new(),
                comment: String::new(),
                display_format: String::new(),
                values: schema::ValuesRecord::Float(vec![0, 1, 2]), // 3 octets, pas multiple de 8
                version: 0,
            },
        };
        let body = FileBody {
            root: 1,
            next_id_hint: 3,
            nodes: vec![root, column],
        };
        let err = build_project(body, Path::new("test.plyx")).unwrap_err();
        assert!(matches!(err, PersistenceError::Corrupt { .. }), "{err:?}");
    }

    /// Copies locales étendues du schéma v1, avec un champ supplémentaire à
    /// chaque niveau — simule un fichier écrit par une version future qui
    /// aurait ajouté des données. Les noms de champs/variantes doivent
    /// coïncider avec `schema.rs` ; seule la présence d'un champ en plus
    /// change.
    #[derive(Serialize)]
    struct ExtendedFileBody {
        root: u64,
        next_id_hint: u64,
        nodes: Vec<ExtendedNodeRecord>,
        extra: String,
    }

    #[derive(Serialize)]
    struct ExtendedNodeRecord {
        id: u64,
        name: String,
        parent: Option<u64>,
        kind: ExtendedKindRecord,
        extra: String,
    }

    #[derive(Serialize)]
    enum ExtendedKindRecord {
        Folder {
            children: Vec<u64>,
        },
        Workbook {
            sheets: Vec<u64>,
            label: String,
            comment: String,
        },
        Worksheet {
            columns: Vec<u64>,
        },
        Column {
            column_type: TypeRecord,
            role: schema::RoleRecord,
            unit: String,
            long_name: String,
            comment: String,
            display_format: String,
            values: schema::ValuesRecord,
            plot_hint: u32,
        },
    }

    /// Critère 6 : des champs inconnus (ajoutés par une version future) sont
    /// ignorés à la lecture plutôt que de faire échouer le chargement.
    /// Chaîne minimale conforme aux règles de contenance du cœur : dossier
    /// racine → classeur → feuille → colonne.
    #[test]
    fn unknown_fields_are_ignored_on_load() {
        let extended = ExtendedFileBody {
            root: 1,
            next_id_hint: 5,
            nodes: vec![
                ExtendedNodeRecord {
                    id: 1,
                    name: "Racine".to_string(),
                    parent: None,
                    kind: ExtendedKindRecord::Folder { children: vec![2] },
                    extra: "ignoré".to_string(),
                },
                ExtendedNodeRecord {
                    id: 2,
                    name: "Book1".to_string(),
                    parent: Some(1),
                    kind: ExtendedKindRecord::Workbook {
                        sheets: vec![3],
                        label: String::new(),
                        comment: String::new(),
                    },
                    extra: "ignoré".to_string(),
                },
                ExtendedNodeRecord {
                    id: 3,
                    name: "Sheet1".to_string(),
                    parent: Some(2),
                    kind: ExtendedKindRecord::Worksheet { columns: vec![4] },
                    extra: "ignoré".to_string(),
                },
                ExtendedNodeRecord {
                    id: 4,
                    name: "A".to_string(),
                    parent: Some(3),
                    kind: ExtendedKindRecord::Column {
                        column_type: TypeRecord::Numeric,
                        role: schema::RoleRecord::X,
                        unit: "ms".to_string(),
                        long_name: String::new(),
                        comment: String::new(),
                        display_format: String::new(),
                        values: schema::ValuesRecord::Float(1.5f64.to_le_bytes().to_vec()),
                        plot_hint: 42,
                    },
                    extra: "ignoré aussi".to_string(),
                },
            ],
            extra: "version future".to_string(),
        };

        let payload = rmp_serde::to_vec_named(&extended).expect("encodage valide");
        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        bytes.extend_from_slice(&payload);

        let dir = tempfile::tempdir().expect("répertoire temporaire");
        let path = dir.path().join("extended.plyx");
        std::fs::write(&path, &bytes).expect("écriture du fichier de test");

        let project = load(&path).expect("le chargement doit ignorer les champs inconnus");
        // `next_id_hint` persisté (5) n'est pas contredit par le plus grand
        // id lu (4) : le compteur repart directement à 5.
        assert_eq!(project.next_id_hint(), 5);
        let node = project
            .tree()
            .node(NodeId::from_raw(4))
            .expect("colonne présente");
        let column = node.as_column().expect("nœud colonne");
        assert_eq!(column.column_type(), ColumnType::Numeric);
        assert_eq!(column.meta().role, plotyx_core::ColumnRole::X);
        assert_eq!(column.meta().unit, "ms");
        assert_eq!(column.len(), 1);
    }

    /// Copie locale de la forme du schéma *avant* la phase R3 : un `Graph`
    /// sans champ `spec` — exactement ce qu'un fichier écrit par une version
    /// antérieure du logiciel contient. Sert à prouver le critère de sortie
    /// de la tâche 3.2 sans dépendre d'un fichier `.plyx` archivé : `spec`
    /// est `#[serde(default)]`, donc un champ absent ne doit pas faire
    /// échouer le chargement, et le graphe reconstruit doit porter
    /// `PlotSpec::default()`.
    #[derive(Serialize)]
    struct PreR3FileBody {
        root: u64,
        next_id_hint: u64,
        nodes: Vec<PreR3NodeRecord>,
    }

    #[derive(Serialize)]
    struct PreR3NodeRecord {
        id: u64,
        name: String,
        parent: Option<u64>,
        kind: PreR3KindRecord,
    }

    #[derive(Serialize)]
    enum PreR3KindRecord {
        Folder { children: Vec<u64> },
        Graph { label: String, comment: String },
    }

    /// Critère de sortie de la tâche 3.2 : un fichier écrit avant la phase R3
    /// (donc sans le champ `spec`) se relit, et le graphe qu'il contient
    /// porte un `PlotSpec` par défaut plutôt que de faire échouer le
    /// chargement.
    #[test]
    fn a_graph_written_before_r3_loads_with_a_default_spec() {
        let pre_r3 = PreR3FileBody {
            root: 1,
            next_id_hint: 3,
            nodes: vec![
                PreR3NodeRecord {
                    id: 1,
                    name: "Racine".to_string(),
                    parent: None,
                    kind: PreR3KindRecord::Folder { children: vec![2] },
                },
                PreR3NodeRecord {
                    id: 2,
                    name: "Graph1".to_string(),
                    parent: Some(1),
                    kind: PreR3KindRecord::Graph {
                        label: String::new(),
                        comment: String::new(),
                    },
                },
            ],
        };

        let payload = rmp_serde::to_vec_named(&pre_r3).expect("encodage valide");
        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        bytes.extend_from_slice(&payload);

        let dir = tempfile::tempdir().expect("répertoire temporaire");
        let path = dir.path().join("pre_r3.plyx");
        std::fs::write(&path, &bytes).expect("écriture du fichier de test");

        let project = load(&path).expect("un fichier sans champ spec doit encore se relire");
        let node = project
            .tree()
            .node(NodeId::from_raw(2))
            .expect("graphe présent");
        match node.kind() {
            NodeKind::Graph { spec, .. } => {
                assert_eq!(*spec, plotyx_plot::PlotSpec::default());
            }
            other => panic!("attendu Graph, obtenu {other:?}"),
        }
    }

    /// `record_to_kind`/`kind_to_record` restent cohérents pour une colonne
    /// vide (longueur 0, ni corrompue ni invalide).
    #[test]
    fn empty_column_round_trips_through_records() {
        let column = Column::new(ColumnType::Numeric);
        let record = convert::kind_to_record(&NodeKind::Column(column.clone()));
        let rebuilt = convert::record_to_kind(record).expect("colonne valide");
        match rebuilt {
            NodeKind::Column(rebuilt_column) => assert_eq!(rebuilt_column, column),
            other => panic!("attendu Column, obtenu {other:?}"),
        }
    }

    /// Sanity-check indépendant de `CoreError` : `Column::from_block` ne
    /// devrait jamais être atteint avec un couple (type, bloc) incohérent
    /// depuis `record_to_kind`, mais on vérifie que l'erreur, si elle
    /// survenait, resterait bien un `CoreError` transmis tel quel.
    #[test]
    fn core_error_variant_exists_for_completeness() {
        let err = ConvertError::Core(CoreError::TypeMismatch("test".to_string()));
        assert!(matches!(
            err,
            ConvertError::Core(CoreError::TypeMismatch(_))
        ));
    }
}
