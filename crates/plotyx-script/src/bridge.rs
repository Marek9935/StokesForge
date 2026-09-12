//! Le côté **hôte** de la surface `pyx` : résolution de chemins, lecture et
//! écriture de colonnes, annulation — le tout **sans jamais toucher au GIL**.
//!
//! Tout ce module s'exécute à l'intérieur d'un `Python::detach`, c'est-à-dire
//! GIL relâché. Rien ici ne nomme un type PyO3 : c'est ce qui permet au
//! marqueur `Ungil` de vérifier à la compilation qu'aucune valeur liée au GIL
//! ne traverse l'attente de l'acteur.
//!
//! # La résolution de chemin n'est plus réécrite ici
//!
//! Ce module a longtemps porté une copie de `Project::resolve` : la résolution
//! vivait dans le `Project`, que **l'acteur** possède, alors qu'un client n'a
//! qu'un `Arc<Tree>` via [`plotyx_session::Snapshot`]. Le doublon était assumé
//! et documenté, faute d'avoir le droit de toucher au socle.
//!
//! Il a été supprimé : `plotyx-core` expose désormais [`Tree::resolve`], seule
//! implémentation de la grammaire `[Book1]Sheet1!ColA` du dépôt, et le pont
//! l'appelle sur l'arbre de son instantané. Il n'y a plus rien à garder
//! synchronisé ici.

use std::sync::Arc;

use plotyx_commands::{Command, CommandError};
use plotyx_core::{CoreError, NodeId, ValueBlock};
use plotyx_session::{SessionError, SessionHandle};

/// A2 (`guide-de-correction-robustesse.md` §F6, §4/§7 de
/// `cahier-des-charges-api-script.md`) — nombre de tentatives que
/// [`set_column`] s'accorde avant de renoncer. Cinq, comme le cahier des
/// charges le fixe : ni une variable d'environnement ni un paramètre, pour
/// que le contrat lu par un auteur de script soit celui que le code applique.
const MAX_SET_COLUMN_ATTEMPTS: u32 = 5;

/// Erreur de l'hôte **déjà traduite** dans le vocabulaire des quatre
/// exceptions de la surface `pyx`.
///
/// La traduction se fait ici, côté Rust pur ; c'est le fil de script qui la
/// convertit en exception Python, GIL en main. Aucune erreur de l'hôte
/// (`SessionError`, `CommandError`, `CoreError`) n'est transportée telle
/// quelle jusqu'à Python.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HostError {
    /// Chemin d'objet mal formé, ou qui ne désigne rien dans le projet.
    UnknownPath(String),
    /// La poignée désigne un nœud absent de l'arbre courant — supprimé, ou
    /// d'un projet rechargé depuis (A3 : l'époque mémorisée par la poignée
    /// ne coïncide plus avec celle de l'instantané courant).
    BrokenReference(String),
    /// Valeur refusée : mauvais type, mauvaise longueur, mauvaise nature
    /// d'objet.
    InvalidValue(String),
    /// A2 — [`set_column`] a essuyé [`MAX_SET_COLUMN_ATTEMPTS`] refus
    /// consécutifs pour cause d'écriture concurrente sur la même colonne.
    ConcurrentWrite(String),
}

impl HostError {
    /// Traduit une erreur du modèle. Le `match` est **exhaustif** à dessein :
    /// une nouvelle variante de `CoreError` forcera une décision de traduction
    /// plutôt que de tomber dans un fourre-tout.
    fn from_core(error: CoreError) -> Self {
        match error {
            CoreError::Path(message) => HostError::UnknownPath(message),
            CoreError::UnknownNode(id) => {
                HostError::BrokenReference(format!("{id} n'existe plus dans le projet"))
            }
            CoreError::InvalidName(message)
            | CoreError::DuplicateName(message)
            | CoreError::Structure(message)
            | CoreError::IndexOutOfBounds(message)
            | CoreError::TypeMismatch(message) => HostError::InvalidValue(message),
            // B1 (`guide-de-correction-robustesse.md` §F2/F3/F4) — plus aucun
            // nœud ne peut être créé : ce n'est ni un chemin, ni une
            // référence cassée, mais la commande qui l'aurait créé qui doit
            // être refusée.
            CoreError::IdentifiersExhausted => {
                HostError::InvalidValue(CoreError::IdentifiersExhausted.to_string())
            }
            // A2 — n'atteint normalement jamais cette traduction générique :
            // [`set_column`] intercepte cette variante avant tout appel à
            // `from_session`/`from_core`, pour retenter plutôt que traduire.
            // Couverte ici pour l'exhaustivité du `match`, avec le même
            // vocabulaire que la traduction que `set_column` construit
            // elle-même après la cinquième tentative.
            CoreError::ConcurrentModification { .. } => {
                HostError::ConcurrentWrite(error.to_string())
            }
        }
    }

    /// Traduit une erreur de la session.
    ///
    /// `Disconnected` devient une **référence cassée** : quand la session est
    /// fermée, plus aucune poignée du script ne désigne quoi que ce soit. Le
    /// message le dit explicitement pour que la cause ne se confonde pas avec
    /// une colonne supprimée.
    fn from_session(error: SessionError) -> Self {
        match error {
            SessionError::Disconnected => HostError::BrokenReference(
                "la session PlotYx est fermée : plus aucune poignée n'est valide".to_owned(),
            ),
            SessionError::Persistence(error) => HostError::InvalidValue(error.to_string()),
            SessionError::Command(CommandError::Core(error)) => Self::from_core(error),
            SessionError::Command(error) => HostError::InvalidValue(error.to_string()),
        }
    }
}

/// `pyx.resolve` : chemin d'objet → identifiant de nœud, et l'époque du
/// projet au moment de la résolution (A3, `guide-de-correction-robustesse.md`
/// §F2/F6) — c'est cette paire que [`crate::surface::Handle`] mémorise, pour
/// pouvoir refuser plus tard si le projet a changé d'identité entre-temps
/// (un `Load`).
pub(crate) fn resolve(session: &SessionHandle, path: &str) -> Result<(NodeId, u64), HostError> {
    let snapshot = session.snapshot().map_err(HostError::from_session)?;
    // Toute erreur de résolution — syntaxe comprise — est un chemin qu'on ne
    // sait pas suivre : c'est `UnknownPathError` dans les deux cas, et le
    // message d'origine dit lequel des deux.
    let id = snapshot
        .tree
        .resolve(path)
        .map_err(|error| HostError::UnknownPath(error.to_string()))?;
    Ok((id, snapshot.epoch))
}

/// Refuse (`BrokenReference`) quand l'époque mémorisée par une poignée ne
/// coïncide plus avec celle de l'instantané courant (A3) : un `Load` a eu
/// lieu depuis que la poignée a été produite par `resolve`, et `id`, même
/// s'il existe encore dans le nouvel arbre, peut y désigner un tout autre
/// objet — exactement le cas de la reproduction
/// `b6_poignee_survit_a_un_load_et_designe_un_autre_objet`. Vérifié **avant**
/// toute recherche de `id` dans l'arbre, pas après : chercher d'abord
/// laisserait passer la coïncidence que cette vérification existe pour
/// écarter.
fn check_epoch(id: NodeId, expected_epoch: u64, current_epoch: u64) -> Result<(), HostError> {
    if expected_epoch == current_epoch {
        return Ok(());
    }
    Err(HostError::BrokenReference(format!(
        "{id} appartient à un projet qui n'existe plus : un chargement a eu lieu depuis que \
         cette poignée a été obtenue"
    )))
}

/// `pyx.column` : le buffer `f64` partagé de la colonne, prêt à être emprunté
/// sans copie par numpy.
///
/// Rend un **clone de l'`Arc`** : c'est lui qui maintiendra le buffer vivant
/// sous la vue numpy, et qui forcera toute écriture ultérieure côté modèle à
/// passer par `Arc::make_mut`, donc à allouer un buffer neuf.
pub(crate) fn float_buffer(
    session: &SessionHandle,
    id: NodeId,
    epoch: u64,
) -> Result<Arc<Vec<f64>>, HostError> {
    let snapshot = session.snapshot().map_err(HostError::from_session)?;
    check_epoch(id, epoch, snapshot.epoch)?;
    let node = snapshot
        .tree
        .node(id)
        .ok_or_else(|| HostError::BrokenReference(format!("{id} n'existe plus dans le projet")))?;
    let column = node.as_column().ok_or_else(|| {
        HostError::InvalidValue(format!(
            "{id} est {} et non une colonne",
            node.kind().with_article()
        ))
    })?;
    column.float_values().cloned().ok_or_else(|| {
        HostError::InvalidValue(format!(
            "la colonne « {} » est de type texte : pyx.column ne rend que des colonnes numériques",
            node.name()
        ))
    })
}

/// `pyx.set_column` : écrit `values` dans la colonne, **par une commande**,
/// donc par le bus, donc par Ctrl+Z.
///
/// Refuse toute longueur différente de celle de la colonne (voir la doc de la
/// surface pour la raison de cette restriction), **avant** d'envoyer quoi que
/// ce soit à l'acteur : un refus ne fait pas bouger la révision.
///
/// # Concurrence optimiste (A2)
///
/// Entre l'instantané lu ici et l'instant où l'acteur applique la commande,
/// un autre client peut avoir modifié la même colonne — c'est exactement le
/// scénario de `b6_toctou_entre_instantane_et_commande`. La commande porte
/// donc `expected_version`, vérifié par `History::apply` au moment
/// d'appliquer, jamais avant : si la version a bougé, l'acteur refuse sans
/// toucher au modèle, et cette fonction relit un instantané frais et
/// retente — jusqu'à [`MAX_SET_COLUMN_ATTEMPTS`] tentatives, exactement la
/// politique documentée dans `cahier-des-charges-api-script.md` (§4, §7).
/// « Recalculer » pour cette surface-témoin, qui n'écrit qu'une colonne
/// entière de longueur fixe, veut dire : rejeter proprement
/// (`InvalidValueError`) si la longueur attendue a changé entre-temps, ou
/// renvoyer la même valeur contre la version fraîchement lue sinon. Au-delà
/// de [`MAX_SET_COLUMN_ATTEMPTS`] échecs, `ConcurrentWriteError` (§7)
/// plutôt que de retenter indéfiniment.
pub(crate) fn set_column(
    session: &SessionHandle,
    id: NodeId,
    epoch: u64,
    values: Vec<f64>,
) -> Result<(), HostError> {
    for _ in 0..MAX_SET_COLUMN_ATTEMPTS {
        let snapshot = session.snapshot().map_err(HostError::from_session)?;
        check_epoch(id, epoch, snapshot.epoch)?;
        let node = snapshot.tree.node(id).ok_or_else(|| {
            HostError::BrokenReference(format!("{id} n'existe plus dans le projet"))
        })?;
        let column = node.as_column().ok_or_else(|| {
            HostError::InvalidValue(format!(
                "{id} est {} et non une colonne",
                node.kind().with_article()
            ))
        })?;
        if column.float_values().is_none() {
            return Err(HostError::InvalidValue(format!(
                "la colonne « {} » est de type texte : pyx.set_column n'écrit que des valeurs \
                 numériques",
                node.name()
            )));
        }
        if values.len() != column.len() {
            return Err(HostError::InvalidValue(format!(
                "longueur {} attendue pour la colonne « {} », {} reçue : la surface-témoin \
                 n'écrit qu'une colonne entière, elle ne l'allonge ni ne la tronque",
                column.len(),
                node.name(),
                values.len()
            )));
        }
        let expected_version = column.version();
        match session.execute(Command::SetCells {
            column: id,
            start: 0,
            values: ValueBlock::Float(values.clone()),
            expected_version: Some(expected_version),
        }) {
            Ok(_) => return Ok(()),
            // A2 — un autre client a modifié la colonne entre l'instantané
            // lu ci-dessus et l'application de cette commande : on relit un
            // instantané frais et on retente, plutôt que de refuser sec ou
            // d'écraser silencieusement son travail.
            Err(SessionError::Command(CommandError::Core(CoreError::ConcurrentModification {
                ..
            }))) => continue,
            Err(other) => return Err(HostError::from_session(other)),
        }
    }
    Err(HostError::ConcurrentWrite(format!(
        "l'écriture de la colonne {id} a été refusée {MAX_SET_COLUMN_ATTEMPTS} fois de suite : \
         un autre client la modifie en continu ; relisez l'état courant ou retentez plus tard"
    )))
}

/// `pyx.undo` : un cran de la pile d'historique de l'hôte.
///
/// Rend `false` quand il n'y a rien à annuler — une pile vide n'est pas une
/// erreur, c'est le cas que l'interface traduit par un bouton grisé.
pub(crate) fn undo(session: &SessionHandle) -> Result<bool, HostError> {
    session
        .undo()
        .map(|revision| revision.is_some())
        .map_err(HostError::from_session)
}
