//! Persistance de StokesForge : `save`/`load` d'un [`stokesforge_core::Project`] complet.
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
//! `stokesforge-core` ne dérive aucun `Serialize`/`Deserialize` : ce crate définit
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
//!    StokesForge (mauvaise signature, ou trop court pour en porter une) est
//!    refusé tout aussi proprement (`NotAStokesForgeFile`).
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
//! octets 0..4   signature "STKF"
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

mod calculations;
pub mod clipboard;
mod convert;
mod error;
mod schema;
mod unknown;

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, Weak};

use serde::{Deserialize, Serialize};
use stokesforge_core::{Node, NodeId, Project, Tree};

use convert::ConvertError;
use schema::{FileBody, NodeRecord, FORMAT_VERSION, MAGIC};

pub use error::{PersistenceError, PersistenceResult};

/// Un calcul que la lecture n'a pas pu garder : son enregistrement est
/// illisible (opération inconnue ou invalide, identifiant hors borne). Donnée
/// neutre : la mise en phrase est l'affaire de l'application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscardedCalculation {
    /// Identifiant du calcul dans le fichier.
    pub id: u64,
    /// Famille d'opération (`"baseline"`…) quand l'enregistrement la porte
    /// encore ; `None` si même cela est illisible.
    pub kind: Option<String>,
}

/// Ce que la lecture d'un `.stf` a dégradé sans échouer (invariant 10) :
/// l'appelant le montre à l'utilisateur au lieu de perdre en silence. Les
/// éléments écartés ne sont **pas conservés** — un enregistrement illisible ne
/// peut pas être reconstruit — et disparaissent donc à la prochaine
/// sauvegarde.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LoadReport {
    /// Calculs d'analyse écartés (#278).
    pub discarded_calculations: Vec<DiscardedCalculation>,
}

impl LoadReport {
    /// Rien n'a été écarté.
    pub fn is_clean(&self) -> bool {
        self.discarded_calculations.is_empty()
    }
}

/// Position et taille d'une fenêtre de l'UI (classeur, graphe, matrice,
/// note), en points logiques — même unité que le reste d'egui.
///
/// # Pourquoi ce type vit ici et pas dans `stokesforge-session`
///
/// La géométrie des fenêtres vit hors de l'arbre du modèle, côté acteur de
/// `stokesforge-session` (voir la doc de tête de ce crate) : ce n'est ni un champ
/// de `WindowMeta`, ni quoi que ce soit de persistant dans `stokesforge-core`. Par
/// défaut ce type aurait donc dû être défini dans `stokesforge-session`. La
/// raison structurelle qui l'en écarte : ce crate (`stokesforge-persistence`) est
/// **en dessous** de `stokesforge-session` dans le graphe de dépendances
/// (`stokesforge-session` dépend de `stokesforge-persistence`, jamais l'inverse), et
/// [`save_with_geometries`]/[`load_with_geometries`] ont besoin de nommer ce
/// type dans leur signature publique. Le définir ici, où `stokesforge-session`
/// vient simplement le réutiliser (`pub use stokesforge_persistence::WindowGeometry`),
/// évite un type dupliqué des deux côtés d'une conversion qui n'aurait rien
/// à faire. Contrairement au reste du schéma (`schema.rs`), il n'existe pas
/// de type "modèle" séparé dont il faudrait rester indépendant (la doctrine
/// "le format ne s'écrit pas dans le modèle" concerne `stokesforge-core`, qui ne
/// porte justement pas ce champ) : ce type public sert donc directement de
/// représentation sur le disque, sans `Record` privé qui le mirorerait.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WindowGeometry {
    /// Abscisse du coin supérieur gauche.
    pub x: f64,
    /// Ordonnée du coin supérieur gauche.
    pub y: f64,
    /// Largeur.
    pub width: f64,
    /// Hauteur.
    pub height: f64,
    /// État de la fenêtre (ouverte, maximisée, fermée, repliée, rang
    /// d'empilement). `None` : jamais enregistré (fichier antérieur, ou
    /// fenêtre jamais vue) — l'application applique alors sa disposition
    /// initiale par défaut. Ne change jamais `x`/`y`/`width`/`height`, qui
    /// restent la géométrie **flottante** quoi qu'il arrive à la fenêtre
    /// (issue #16).
    #[serde(default)]
    pub view: Option<WindowView>,
}

/// État d'affichage d'une fenêtre de contenu, retenu avec sa
/// [`WindowGeometry`] flottante. Une fenêtre détachée est enregistrée
/// `Open` : le détachement ne se mémorise pas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowView {
    /// Ouverte, maximisée ou fermée.
    pub mode: WindowMode,
    /// Fenêtre repliée sur sa barre de titre.
    #[serde(default)]
    pub collapsed: bool,
    /// Rang d'empilement parmi les fenêtres ouvertes : 0 = tout en dessous,
    /// le plus grand rang est au premier plan. Un rang relatif, pas une
    /// valeur de calque egui.
    #[serde(default)]
    pub stack: u32,
}

/// Mode d'une fenêtre de contenu. Une valeur inconnue (fichier d'une version
/// plus récente) se lit `Closed` : dégradation gracieuse, jamais un échec de
/// chargement (invariant 10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WindowMode {
    /// Fenêtre flottante ouverte.
    Open,
    /// Fenêtre maximisée dans l'espace de travail.
    Maximized,
    /// Fenêtre fermée (sa géométrie flottante est conservée).
    #[serde(other)]
    Closed,
}

/// Borne haute d'un identifiant de nœud ou de `next_id_hint` lu d'un fichier
/// (B1, `guide-de-correction-robustesse.md` §F3) : largement au-dessus de
/// tout projet réel, largement en dessous de `u64::MAX` — l'endroit où
/// `checked_add` de `IdAllocator::next` (mécanique, déjà en place côté
/// `stokesforge-core`) refuse de toute façon, mais trop tard pour qu'un projet
/// chargé reste utilisable.
const MAX_NODE_ID: u64 = 1_000_000_000_000;

/// Sauvegarde `project` vers `path`, atomiquement : un fichier temporaire est
/// écrit dans le même répertoire que `path` puis renommé sur la cible ; en
/// cas d'échec à n'importe quelle étape, `path` n'est pas touché et le
/// temporaire est supprimé (comportement de `NamedTempFile` à l'abandon).
///
/// Deux appels successifs sur le même `project` produisent des fichiers
/// identiques octet à octet (ordre déterministe des nœuds : parcours
/// préfixe depuis la racine).
///
/// # Détection d'un écrivain concurrent (B8)
///
/// Avant d'écrire, `save` compare le compteur de sauvegarde actuellement sur
/// le disque (s'il existe déjà un fichier à `path`) à celui que **cette
/// instance de `project`, dans ce processus**, a vu la dernière fois qu'elle
/// a été chargée ou sauvegardée vers ce même chemin. S'ils diffèrent, un
/// autre écrivain a modifié le fichier entre-temps : `save` refuse
/// (`PersistenceError::ConcurrentModification`) plutôt que d'écraser son
/// travail en silence. Ce suivi est un registre en mémoire, propre au
/// processus (voir [`tracked_counter`]) — rien de tout cela ne traverse le
/// disque à part le compteur lui-même (`FileBody::save_counter`), conforme à
/// la doctrine des estampilles (§F2 du guide) : seule la valeur qui permet la
/// comparaison est persistée, jamais l'identité de qui l'a vue.
///
/// Un `project` que ce processus n'a **jamais** chargé ni sauvegardé vers ce
/// chemin (un projet neuf, ou une sauvegarde vers une nouvelle destination)
/// n'a pas de baseline à comparer : `save` procède sans contrôle, comme
/// avant B8 — il ne peut détecter qu'un changement depuis qu'IL a vu le
/// fichier, pas depuis l'origine des temps.
///
/// Équivalent à [`save_with_geometries`] avec une table de géométries vide :
/// n'écrit aucun bloc de géométrie de fenêtre. C'est la fonction qu'utilisent
/// les appelants qui ne connaissent pas cette notion (`stokesforge-app`) ;
/// `stokesforge-session`, qui possède la table `NodeId -> WindowGeometry` à côté
/// du `Project`, appelle `save_with_geometries` pour que le fichier retrouve
/// la disposition des fenêtres à la réouverture.
pub fn save(project: &Project, path: &Path) -> PersistenceResult<()> {
    save_with_geometries(project, &HashMap::new(), path)
}

/// Comme [`save`], mais écrit en plus `geometries` dans un bloc **séparé de
/// l'arbre annulable** (`FileBody::window_geometries`) — jamais mêlé aux
/// `NodeRecord` de `nodes`, jamais passé par `convert.rs` (qui ne convertit
/// que des `Node`). C'est la fonction qu'appelle `stokesforge-session::actor`
/// pour que la position des fenêtres survive à un rechargement du projet.
pub fn save_with_geometries(
    project: &Project,
    geometries: &HashMap<NodeId, WindowGeometry>,
    path: &Path,
) -> PersistenceResult<()> {
    let disk_counter = peek_save_counter(path)?;
    if let Some(found) = disk_counter {
        if let Some(expected) = tracked_counter(path, project) {
            if expected != found {
                return Err(PersistenceError::ConcurrentModification {
                    path: path.to_path_buf(),
                    expected,
                    found,
                });
            }
        }
    }
    let save_counter = disk_counter.unwrap_or(0) + 1;

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
    let window_geometries = geometries
        .iter()
        .map(|(id, geometry)| (id.raw(), *geometry))
        .collect();
    let body = FileBody {
        root: tree.root().raw(),
        next_id_hint: project.next_id_hint(),
        nodes,
        save_counter,
        saved_colors: convert::saved_colors_to_record(tree.saved_colors()),
        calculations: calculations::calculations_to_records(tree),
        correction_traces: calculations::corrections_to_records(tree),
        window_geometries,
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
    track_counter(path, project, save_counter);
    Ok(())
}

/// Charge un [`Project`] complet depuis `path`. La version est lue et
/// vérifiée avant tout décodage du corps : un fichier plus récent que ce
/// lecteur est refusé sans que son corps soit touché. Une version `0` — un
/// numéro qui n'a jamais existé, la première version réelle étant 1 — est
/// refusée elle aussi, explicitement, plutôt que d'être lue comme si
/// c'était la version 1.
///
/// Équivalent à [`load_with_geometries`] dont le bloc de géométries de
/// fenêtre est jeté : c'est la fonction qu'utilisent les appelants qui ne
/// connaissent pas cette notion (`stokesforge-app`, aujourd'hui).
pub fn load(path: &Path) -> PersistenceResult<Project> {
    load_with_geometries(path).map(|(project, _geometries)| project)
}

/// Comme [`load`], mais rend aussi le bloc de géométries de fenêtre lu dans
/// le fichier (`FileBody::window_geometries`), clé par `NodeId`. Une entrée
/// dont la clé ne correspond à aucun nœud du projet reconstruit (fichier
/// corrompu, ou géométrie orpheline laissée par un outil tiers) est
/// silencieusement écartée plutôt que de faire échouer le chargement —
/// invariant 10 : un défaut de donnée dégrade, il ne plante pas. C'est la
/// fonction qu'appelle `stokesforge-session::actor` pour retrouver la disposition
/// des fenêtres à la réouverture d'un projet.
pub fn load_with_geometries(
    path: &Path,
) -> PersistenceResult<(Project, HashMap<NodeId, WindowGeometry>)> {
    load_with_report(path).map(|(project, geometries, _report)| (project, geometries))
}

/// Comme [`load_with_geometries`], mais rend aussi le [`LoadReport`] de ce que
/// la lecture a écarté (calculs illisibles, #278). C'est la fonction qu'appelle
/// la session, pour que l'application le signale.
pub fn load_with_report(
    path: &Path,
) -> PersistenceResult<(Project, HashMap<NodeId, WindowGeometry>, LoadReport)> {
    let bytes = std::fs::read(path).map_err(|source| PersistenceError::Io {
        path: path.to_path_buf(),
        source,
    })?;

    if bytes.len() < MAGIC.len() || &bytes[..MAGIC.len()] != MAGIC.as_slice() {
        return Err(PersistenceError::NotAStokesForgeFile {
            path: path.to_path_buf(),
        });
    }
    if bytes.len() < MAGIC.len() + 4 {
        return Err(PersistenceError::Corrupt {
            path: path.to_path_buf(),
            detail: "header truncated before the version number".to_string(),
        });
    }
    let mut version_bytes = [0u8; 4];
    version_bytes.copy_from_slice(&bytes[MAGIC.len()..MAGIC.len() + 4]);
    let version = u32::from_le_bytes(version_bytes);
    if version == 0 {
        return Err(PersistenceError::Corrupt {
            path: path.to_path_buf(),
            detail: "version number 0 in the header: it never existed, \
                     the first real format version is 1"
                .to_string(),
        });
    }
    if version > FORMAT_VERSION {
        return Err(PersistenceError::UnsupportedVersion {
            path: path.to_path_buf(),
            found: version,
            supported: FORMAT_VERSION,
        });
    }

    let mut body: FileBody = rmp_serde::from_slice(&bytes[MAGIC.len() + 4..])
        .map_err(|source| classify_body_error(source, path))?;

    let save_counter = body.save_counter;
    let raw_geometries = std::mem::take(&mut body.window_geometries);
    for &id in raw_geometries.keys() {
        check_node_id(id, path)?;
    }
    let (project, report) = build_project_reported(body, path)?;
    track_counter(path, &project, save_counter);

    // Une clé sans nœud correspondant (fichier corrompu, ou géométrie
    // orpheline laissée par un outil tiers qui n'aurait pas purgé la sienne)
    // est écartée en silence plutôt que de faire échouer le chargement —
    // invariant 10. `stokesforge-session::actor` purge lui-même, à chaque
    // mutation, les entrées orphelines créées pendant la vie du processus ;
    // ce filtre couvre le cas où le fichier en portait déjà avant même
    // d'être ouvert.
    let tree = project.tree();
    let geometries = raw_geometries
        .into_iter()
        .filter_map(|(id, geometry)| {
            let node = NodeId::from_raw(id);
            tree.node(node).is_some().then_some((node, geometry))
        })
        .collect();

    Ok((project, geometries, report))
}

/// Classe une erreur de décodage du corps : une **variante d'énumération**
/// inconnue (C5, niveau 1) n'est pas la même chose qu'un fichier vraiment
/// corrompu — `rmp_serde`/`serde` nomment ce cas dans le message d'erreur
/// (« unknown variant … »), ce qui suffit à le distinguer sans avoir besoin
/// d'inspecter les octets nous-mêmes.
fn classify_body_error(source: rmp_serde::decode::Error, path: &Path) -> PersistenceError {
    let detail = source.to_string();
    if detail.contains("unknown variant") {
        PersistenceError::UnknownVariant {
            path: path.to_path_buf(),
            detail,
        }
    } else {
        PersistenceError::Corrupt {
            path: path.to_path_buf(),
            detail,
        }
    }
}

/// Reconstruit un `Project` depuis un corps déjà décodé. Séparé de `load`
/// pour que les tests internes puissent fabriquer un `FileBody` à la main
/// (cas ceinture du compteur d'identifiants, champs inconnus, modèle
/// invalide) sans repasser par l'encodage MessagePack.
fn build_project(body: FileBody, path: &Path) -> PersistenceResult<Project> {
    build_project_reported(body, path).map(|(project, _report)| project)
}

/// [`build_project`], avec le rapport de ce que la reconstruction a écarté.
fn build_project_reported(body: FileBody, path: &Path) -> PersistenceResult<(Project, LoadReport)> {
    check_node_id(body.root, path)?;
    check_node_id(body.next_id_hint, path)?;
    let mut nodes = Vec::with_capacity(body.nodes.len());
    for record in body.nodes {
        let NodeRecord {
            id,
            name,
            parent,
            kind,
        } = record;
        check_node_id(id, path)?;
        if let Some(parent_id) = parent {
            check_node_id(parent_id, path)?;
        }
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

    let (kept, discarded_calculations) =
        calculations::records_to_calculations(body.calculations, MAX_NODE_ID);
    let tree = Tree::from_nodes(NodeId::from_raw(body.root), nodes)
        .map_err(|source| PersistenceError::InvalidModel {
            path: path.to_path_buf(),
            source,
        })?
        .with_saved_colors(convert::record_to_saved_colors(body.saved_colors))
        .with_correction_traces(calculations::records_to_corrections(
            body.correction_traces,
            MAX_NODE_ID,
        ))
        .with_calculations(kept);
    Ok((
        Project::from_tree(Arc::new(tree), body.next_id_hint),
        LoadReport {
            discarded_calculations,
        },
    ))
}

/// Refuse un identifiant de nœud (ou `next_id_hint`) au-delà de
/// [`MAX_NODE_ID`] (B1) : bien au-dessus de tout projet réel, bien en
/// dessous de `u64::MAX` où `IdAllocator::next` refuserait de toute façon —
/// mais un projet dont un nœud porte déjà un identifiant proche de cette
/// frontière resterait inutilisable dès la première création suivante.
/// Borner à la lecture ferme ce chemin d'entrée plutôt que de compter sur le
/// `checked_add` mécanique de `stokesforge-core`, qui protège l'allocateur mais
/// pas le fichier qui l'a fait démarrer si près du bord.
fn check_node_id(id: u64, path: &Path) -> PersistenceResult<()> {
    if id > MAX_NODE_ID {
        Err(PersistenceError::Corrupt {
            path: path.to_path_buf(),
            detail: format!(
                "node identifier {id} exceeds the limit accepted when reading \
                 ({MAX_NODE_ID})"
            ),
        })
    } else {
        Ok(())
    }
}

/// Traduit une erreur de conversion bas niveau (`convert.rs`) en erreur
/// publique : dans les deux cas, le fichier est incohérent d'une façon
/// qu'un fichier écrit par ce crate ne produit jamais.
fn convert_error(err: ConvertError, path: &Path) -> PersistenceError {
    match err {
        ConvertError::BadFloatLength(len) => PersistenceError::Corrupt {
            path: path.to_path_buf(),
            detail: format!("float column buffer of {len} bytes, not a multiple of 8"),
        },
        ConvertError::BadMixedTag(tag) => PersistenceError::Corrupt {
            path: path.to_path_buf(),
            detail: format!(
                "unknown mixed cell tag ({tag}): expected 0 (empty), \
                 1 (number) or 2 (text)"
            ),
        },
        ConvertError::BadMixedLength {
            expected,
            found,
            what,
        } => PersistenceError::Corrupt {
            path: path.to_path_buf(),
            detail: format!(
                "inconsistent mixed column: the tags announce {expected} {what}, \
                 the array holds {found}"
            ),
        },
        ConvertError::Core(source) => PersistenceError::Corrupt {
            path: path.to_path_buf(),
            detail: format!("inconsistent column: {source}"),
        },
        ConvertError::ZeroHistogramBins => PersistenceError::Corrupt {
            path: path.to_path_buf(),
            detail: "a series has histogram_bins of 0, which is not representable".to_string(),
        },
        ConvertError::HistogramBinsTooLarge(bins) => PersistenceError::Corrupt {
            path: path.to_path_buf(),
            detail: format!(
                "a series has histogram_bins of {bins}, beyond the accepted limit \
                 ({})",
                convert::MAX_HISTOGRAM_BINS
            ),
        },
        ConvertError::PanelPositionOutOfBounds { row, column } => PersistenceError::Corrupt {
            path: path.to_path_buf(),
            detail: format!(
                "panel at position (row={row}, column={column}), beyond the accepted \
                 limit ({})",
                convert::MAX_PANEL_POSITION
            ),
        },
        ConvertError::UnsupportedSpecVersion { found, supported } => PersistenceError::Corrupt {
            path: path.to_path_buf(),
            detail: format!(
                "a PlotSpec has format_version {found}, this reader only knows up to \
                 {supported}"
            ),
        },
    }
}

// -- B8 : détection d'un écrivain concurrent -------------------------------
//
// Registre en mémoire, propre à ce processus, associant (chemin canonique,
// identité d'un `Project`) au dernier compteur de sauvegarde observé pour ce
// couple. Ne persiste jamais sur disque — seul le compteur qu'il décrit
// traverse le disque (`FileBody::save_counter`), conforme à la doctrine des
// estampilles (§F2 du guide de correction).
//
// La clé n'est pas le chemin seul : deux `Project` distincts chargés depuis
// le même fichier (deux fenêtres, un script et l'interface, …) ne doivent
// pas se confondre entre eux — c'est précisément le cas que B8 doit
// détecter. On retient une référence **faible** sur `Project::identity()` —
// PAS sur l'arbre courant (`project.tree()`) : une première version de ce
// registre clefait sur l'identité de l'arbre, en s'appuyant sur le fait que
// `Arc::make_mut` réutilise la même allocation tant que personne d'autre ne
// l'a clonée. C'est faux dès qu'un `undo`/`redo` entre en jeu :
// `Project::replace_tree` réaffecte à l'instance un `Arc<Tree>` DÉJÀ VU à un
// instant antérieur de CETTE MÊME instance (restauré depuis la pile
// d'historique) — la même adresse redevient donc « courante » alors que le
// compteur de sauvegarde a avancé entre-temps, et le registre y voyait à
// tort un compteur périmé, refusant une sauvegarde parfaitement légitime
// (trouvé par `b14_laller_retour_disque_tient_a_chaque_pas`, un balayage
// aléatoire d'opérations incluant `Annuler`/`Rétablir`). `Project::identity
// ()` est stable sur toute la vie d'une instance, quel que soit l'arbre
// qu'elle affiche à un instant donné, et c'est exactement l'identité qu'il
// faut ici — voir sa documentation dans `stokesforge-core`. `Weak::upgrade`
// échoue proprement pour un `Project` abandonné depuis longtemps plutôt que
// de risquer une collision avec une adresse mémoire réutilisée par une
// valeur sans rapport (tant qu'une `Weak` existe, l'allocation qu'elle vise
// reste en vie — deux entrées encore « upgradables » en même temps ne
// peuvent donc jamais partager une adresse).

struct TrackedSave {
    path: PathBuf,
    identity: Weak<()>,
    counter: u64,
}

fn registry() -> &'static Mutex<Vec<TrackedSave>> {
    static REGISTRY: OnceLock<Mutex<Vec<TrackedSave>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(Vec::new()))
}

/// Chemin canonique si le fichier existe déjà ; sinon le chemin tel quel —
/// une destination qui n'existe pas encore n'a pas de forme canonique.
fn canonical_or_given(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn matches(entry: &TrackedSave, canon: &Path, project: &Project) -> bool {
    entry.path == canon
        && entry.identity.upgrade().is_some()
        && Weak::ptr_eq(&entry.identity, &project.identity())
}

/// Compteur que CETTE instance de `project` (au sens de l'identité de son
/// arbre) a vu la dernière fois qu'elle a été chargée ou sauvegardée vers
/// `path`, dans ce processus. `None` si ce couple (chemin, instance) est
/// inconnu du registre : soit ce `Project` n'est jamais passé par
/// `load`/`save` pour ce chemin, soit son arbre a depuis été remplacé
/// (`replace_tree`, restauration d'annulation) — dans les deux cas, il n'y a
/// rien à comparer, et `save` procède sans contrôle plutôt que de refuser à
/// tort.
fn tracked_counter(path: &Path, project: &Project) -> Option<u64> {
    let canon = canonical_or_given(path);
    let mut guard = match registry().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    guard.retain(|entry| entry.identity.strong_count() > 0);
    guard
        .iter()
        .find(|entry| matches(entry, &canon, project))
        .map(|entry| entry.counter)
}

/// Enregistre (ou met à jour) le dernier compteur connu pour ce couple
/// (chemin, instance) — appelé après chaque `load`/`save` réussi.
fn track_counter(path: &Path, project: &Project, counter: u64) {
    let canon = canonical_or_given(path);
    let mut guard = match registry().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    guard.retain(|entry| entry.identity.strong_count() > 0);
    if let Some(entry) = guard
        .iter_mut()
        .find(|entry| matches(entry, &canon, project))
    {
        entry.counter = counter;
    } else {
        guard.push(TrackedSave {
            path: canon,
            identity: project.identity(),
            counter,
        });
    }
}

/// Lit uniquement le compteur de sauvegarde d'un fichier déjà écrit par ce
/// format, sans construire de `Project` : c'est le contrôle bon marché que
/// `save` fait avant d'écraser une cible existante.
///
/// `Ok(None)` si le fichier n'existe pas encore (rien à comparer : la
/// sauvegarde est la première) ou s'il n'est pas reconnaissable comme un
/// fichier de ce format (comportement historique conservé pour une cible qui
/// n'est pas un `.stf` — B8 ne couvre que la collision entre deux écritures
/// de ce format-ci, pas le remplacement d'un fichier étranger). Un fichier
/// plus récent que ce lecteur (`version > FORMAT_VERSION`) reste refusé,
/// comme au chargement : mieux vaut ne pas écraser à l'aveugle un fichier
/// qu'on ne sait pas lire.
fn peek_save_counter(path: &Path) -> PersistenceResult<Option<u64>> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(PersistenceError::Io {
                path: path.to_path_buf(),
                source,
            })
        }
    };
    if bytes.len() < MAGIC.len() || &bytes[..MAGIC.len()] != MAGIC.as_slice() {
        return Ok(None);
    }
    if bytes.len() < MAGIC.len() + 4 {
        return Ok(None);
    }
    let mut version_bytes = [0u8; 4];
    version_bytes.copy_from_slice(&bytes[MAGIC.len()..MAGIC.len() + 4]);
    let version = u32::from_le_bytes(version_bytes);
    if version == 0 {
        return Ok(None);
    }
    if version > FORMAT_VERSION {
        return Err(PersistenceError::UnsupportedVersion {
            path: path.to_path_buf(),
            found: version,
            supported: FORMAT_VERSION,
        });
    }
    match rmp_serde::from_slice::<FileBody>(&bytes[MAGIC.len() + 4..]) {
        Ok(body) => Ok(Some(body.save_counter)),
        // Le corps ne se décode pas : `save` n'est pas l'endroit qui
        // diagnostique une corruption (c'est le rôle de `load`) — on traite
        // ce cas comme « rien à comparer » plutôt que de bloquer l'écriture.
        Err(_) => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use schema::{KindRecord, TypeRecord};
    use serde::Serialize;
    use stokesforge_core::{Column, ColumnType, CoreError, NodeKind};

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
            save_counter: 0,
            saved_colors: Default::default(),
            calculations: Default::default(),
            correction_traces: Default::default(),
            window_geometries: std::collections::HashMap::new(),
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
            save_counter: 0,
            saved_colors: Default::default(),
            calculations: Default::default(),
            correction_traces: Default::default(),
            window_geometries: std::collections::HashMap::new(),
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
            save_counter: 0,
            saved_colors: Default::default(),
            calculations: Default::default(),
            correction_traces: Default::default(),
            window_geometries: std::collections::HashMap::new(),
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
                hidden: false,
                label: String::new(),
                comment: String::new(),
                text: String::new(),
            },
        };
        let body = FileBody {
            root: 1,
            next_id_hint: 2,
            nodes: vec![root],
            save_counter: 0,
            saved_colors: Default::default(),
            calculations: Default::default(),
            correction_traces: Default::default(),
            window_geometries: std::collections::HashMap::new(),
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
                formula: None,
                file_metadata: Vec::new(),
                import_source: None,
                x_prime: None,
            },
        };
        let body = FileBody {
            root: 1,
            next_id_hint: 3,
            nodes: vec![root, column],
            save_counter: 0,
            saved_colors: Default::default(),
            calculations: Default::default(),
            correction_traces: Default::default(),
            window_geometries: std::collections::HashMap::new(),
        };
        let err = build_project(body, Path::new("test.plyx")).unwrap_err();
        assert!(matches!(err, PersistenceError::Corrupt { .. }), "{err:?}");
    }

    /// Migration de compatibilité : un fichier écrit avant le retrait du
    /// type `Text` homogène (`ColumnType`) porte encore `TypeRecord::Text` /
    /// `ValuesRecord::Text` au disque. Le charger doit produire une colonne
    /// `Mixed`, où chaque `Option<String>` devient la cellule mixte
    /// équivalente **telle quelle** : `None → Empty`, `Some(s) → Text(s)`,
    /// jamais réanalysée en nombre — pas de corruption, pas de refus de
    /// charger le fichier.
    #[test]
    fn a_legacy_text_column_migrates_to_mixed_on_load() {
        let root = NodeRecord {
            id: 1,
            name: "Racine".to_string(),
            parent: None,
            kind: KindRecord::Folder { children: vec![2] },
        };
        let book = NodeRecord {
            id: 2,
            name: "Book1".to_string(),
            parent: Some(1),
            kind: KindRecord::Workbook {
                hidden: false,
                sheets: vec![3],
                label: String::new(),
                comment: String::new(),
            },
        };
        let sheet = NodeRecord {
            id: 3,
            name: "Sheet1".to_string(),
            parent: Some(2),
            kind: KindRecord::Worksheet { columns: vec![4] },
        };
        let column = NodeRecord {
            id: 4,
            name: "A".to_string(),
            parent: Some(3),
            kind: KindRecord::Column {
                column_type: TypeRecord::Text,
                role: schema::RoleRecord::Y,
                unit: String::new(),
                long_name: String::new(),
                comment: String::new(),
                display_format: String::new(),
                values: schema::ValuesRecord::Text(vec![
                    Some(String::new()),
                    None,
                    Some("1.5".to_string()),
                ]),
                version: 3,
                formula: None,
                file_metadata: Vec::new(),
                import_source: None,
                x_prime: None,
            },
        };
        let body = FileBody {
            root: 1,
            next_id_hint: 5,
            nodes: vec![root, book, sheet, column],
            save_counter: 0,
            saved_colors: Default::default(),
            calculations: Default::default(),
            correction_traces: Default::default(),
            window_geometries: std::collections::HashMap::new(),
        };
        let project = build_project(body, Path::new("legacy.plyx")).expect("modèle valide");
        let loaded = project
            .tree()
            .node(stokesforge_core::NodeId::from_raw(4))
            .expect("colonne présente")
            .as_column()
            .expect("nœud colonne");

        assert_eq!(loaded.column_type(), ColumnType::Mixed);
        assert_eq!(
            loaded.cell(0),
            stokesforge_core::MixedCell::Text(String::new())
        );
        assert_eq!(loaded.cell(1), stokesforge_core::MixedCell::Empty);
        // Pas réanalysée en nombre malgré l'allure numérique du texte : la
        // migration copie la cellule, elle ne la réinterprète pas (même
        // règle que `MixedCell::from_text`/`Column::set_column_type`
        // documentent pour toute conversion délibérée, mais ici il n'y a
        // même pas de conversion à faire, juste une relecture).
        assert_eq!(
            loaded.cell(2),
            stokesforge_core::MixedCell::Text("1.5".to_string())
        );
        assert_eq!(
            loaded.version(),
            3,
            "le compteur de version traverse le disque"
        );
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
        assert_eq!(column.meta().role, stokesforge_core::ColumnRole::X);
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
                assert_eq!(*spec, stokesforge_plot::PlotSpec::default());
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

    /// Géométrie des fenêtres (tâche « géométrie des fenêtres ») : une entrée
    /// posée via `save_with_geometries` se relit identique via
    /// `load_with_geometries`, associée au bon `NodeId`.
    #[test]
    fn geometries_round_trip_through_save_and_load() {
        let mut project = stokesforge_core::Project::new("Demo").expect("projet valide");
        let root = project.root();
        let book = project
            .add_empty_workbook(root, Some("Book1"))
            .expect("classeur créé");

        let mut geometries = HashMap::new();
        geometries.insert(
            book,
            crate::WindowGeometry {
                x: 12.0,
                y: 34.0,
                width: 640.0,
                height: 480.0,
                view: Some(crate::WindowView {
                    mode: crate::WindowMode::Maximized,
                    collapsed: true,
                    stack: 3,
                }),
            },
        );

        let dir = tempfile::tempdir().expect("répertoire temporaire");
        let path = dir.path().join("geometries.plyx");
        save_with_geometries(&project, &geometries, &path).expect("sauvegarde réussie");

        let (loaded, loaded_geometries) = load_with_geometries(&path).expect("chargement réussi");
        assert_eq!(loaded.tree().len(), project.tree().len());
        assert_eq!(loaded_geometries, geometries);
    }

    /// Rétrocompatibilité : un fichier écrit avant l'ajout du bloc de
    /// géométries (`FileBody` sans `window_geometries`) se recharge sans
    /// erreur, avec une table de géométries vide — même patron que
    /// `unknown_fields_are_ignored_on_load`/`a_graph_written_before_r3_loads_with_a_default_spec`
    /// ci-dessus, qui simulent déjà un fichier plus ancien pour d'autres
    /// champs `#[serde(default)]`.
    #[derive(Serialize)]
    struct PreGeometryFileBody {
        root: u64,
        next_id_hint: u64,
        nodes: Vec<NodeRecord>,
        save_counter: u64,
    }

    #[test]
    fn a_file_without_the_geometry_block_loads_with_an_empty_geometry_table() {
        let root = folder_record(1, "Racine", vec![]);
        let pre_geometry = PreGeometryFileBody {
            root: 1,
            next_id_hint: 2,
            nodes: vec![root],
            save_counter: 3,
        };

        let payload = rmp_serde::to_vec_named(&pre_geometry).expect("encodage valide");
        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        bytes.extend_from_slice(&payload);

        let dir = tempfile::tempdir().expect("répertoire temporaire");
        let path = dir.path().join("pre_geometry.plyx");
        std::fs::write(&path, &bytes).expect("écriture du fichier de test");

        let (project, geometries) =
            load_with_geometries(&path).expect("un fichier sans bloc de géométries doit se relire");
        assert!(geometries.is_empty());
        assert_eq!(project.next_id_hint(), 2);
    }
}
