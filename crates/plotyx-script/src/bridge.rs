//! Le côté **hôte** de la surface `pyx` : résolution de chemins, lecture et
//! écriture de colonnes, annulation — le tout **sans jamais toucher au GIL**.
//!
//! Tout ce module s'exécute à l'intérieur d'un `Python::detach`, c'est-à-dire
//! GIL relâché. Rien ici ne nomme un type PyO3 : c'est ce qui permet au
//! marqueur `Ungil` de vérifier à la compilation qu'aucune valeur liée au GIL
//! ne traverse l'attente de l'acteur.
//!
//! # Pourquoi une résolution de chemin est réécrite ici
//!
//! `Project::resolve_path` est une méthode du `Project`, que **l'acteur**
//! possède. Un client n'a qu'un `Arc<Tree>` via [`plotyx_session::Snapshot`].
//! [`resolve_in_tree`] refait donc la même résolution contre un `Tree` seul,
//! avec l'API publique de `plotyx-core` (`parse_path`, `Tree::find_window`,
//! `Tree::child_by_name`, `Tree::ancestors`) — même grammaire
//! `[Book1]Sheet1!ColA`, mêmes règles de dossiers. C'est un doublon assumé et
//! visible ; le supprimer demanderait une `Tree::resolve` dans `plotyx-core`,
//! ce que cette tâche n'a pas le droit de faire.

use std::sync::Arc;

use plotyx_commands::{Command, CommandError};
use plotyx_core::{parse_path, CoreError, CoreResult, NodeId, NodeKind, Tree, ValueBlock};
use plotyx_session::{SessionError, SessionHandle};

/// Erreur de l'hôte **déjà traduite** dans le vocabulaire des trois exceptions
/// de la surface `pyx`.
///
/// La traduction se fait ici, côté Rust pur ; c'est le fil de script qui la
/// convertit en exception Python, GIL en main. Aucune erreur de l'hôte
/// (`SessionError`, `CommandError`, `CoreError`) n'est transportée telle
/// quelle jusqu'à Python.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HostError {
    /// Chemin d'objet mal formé, ou qui ne désigne rien dans le projet.
    UnknownPath(String),
    /// La poignée désigne un nœud absent de l'arbre courant.
    BrokenReference(String),
    /// Valeur refusée : mauvais type, mauvaise longueur, mauvaise nature
    /// d'objet.
    InvalidValue(String),
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

/// Résout un chemin d'objet contre un `Tree` seul.
///
/// Réplique fidèle de `Project::resolve` : mêmes règles de dossiers, même
/// unicité globale des noms de fenêtre, mêmes messages. Toute erreur est un
/// [`CoreError::Path`], que la syntaxe soit fautive ou que l'objet manque —
/// la surface `pyx` les rend toutes deux comme `UnknownPathError`.
pub(crate) fn resolve_in_tree(tree: &Tree, text: &str) -> CoreResult<NodeId> {
    let path = parse_path(text)?;

    let Some(window_name) = path.window.as_deref() else {
        let mut current = tree.root();
        for segment in &path.folders {
            let child = tree
                .child_by_name(current, segment)?
                .filter(|id| tree.node(*id).is_some_and(|node| node.kind().is_folder()))
                .ok_or_else(|| CoreError::Path(format!("dossier introuvable : {segment}")))?;
            current = child;
        }
        return Ok(current);
    };

    let window = tree
        .find_window(window_name)
        .ok_or_else(|| CoreError::Path(format!("fenêtre introuvable : [{window_name}]")))?;

    if !path.folders.is_empty() {
        let actual = folder_names_above(tree, window)?;
        if actual != path.folders {
            return Err(CoreError::Path(format!(
                "[{window_name}] n'est pas dans le dossier {} mais dans {}",
                display_folders(&path.folders),
                display_folders(&actual)
            )));
        }
    }

    let Some(sheet_name) = path.sheet.as_deref() else {
        return Ok(window);
    };
    if !matches!(tree.get(window)?.kind(), NodeKind::Workbook { .. }) {
        return Err(CoreError::Path(format!(
            "[{window_name}] n'est pas un classeur, il n'a pas de feuille"
        )));
    }
    let sheet = tree.child_by_name(window, sheet_name)?.ok_or_else(|| {
        CoreError::Path(format!("feuille introuvable : [{window_name}]{sheet_name}"))
    })?;

    let Some(column_name) = path.column.as_deref() else {
        return Ok(sheet);
    };
    tree.child_by_name(sheet, column_name)?
        .ok_or_else(|| CoreError::Path(format!("colonne introuvable : {text}")))
}

/// Noms des dossiers au-dessus de `id`, de la racine (exclue) vers `id`.
fn folder_names_above(tree: &Tree, id: NodeId) -> CoreResult<Vec<String>> {
    let root = tree.root();
    let mut names: Vec<String> = tree
        .ancestors(id)?
        .into_iter()
        .filter(|ancestor| *ancestor != root)
        .filter_map(|ancestor| tree.node(ancestor))
        .filter(|node| node.kind().is_folder())
        .map(|node| node.name().to_string())
        .collect();
    names.reverse();
    Ok(names)
}

/// Forme lisible d'une suite de dossiers, `/` pour la racine.
fn display_folders(folders: &[String]) -> String {
    if folders.is_empty() {
        "/".to_owned()
    } else {
        folders.join("/")
    }
}

/// `pyx.resolve` : chemin d'objet → identifiant de nœud.
pub(crate) fn resolve(session: &SessionHandle, path: &str) -> Result<NodeId, HostError> {
    let snapshot = session.snapshot().map_err(HostError::from_session)?;
    // Toute erreur de résolution — syntaxe comprise — est un chemin qu'on ne
    // sait pas suivre : c'est `UnknownPathError` dans les deux cas, et le
    // message d'origine dit lequel des deux.
    resolve_in_tree(&snapshot.tree, path).map_err(|error| HostError::UnknownPath(error.to_string()))
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
) -> Result<Arc<Vec<f64>>, HostError> {
    let snapshot = session.snapshot().map_err(HostError::from_session)?;
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
pub(crate) fn set_column(
    session: &SessionHandle,
    id: NodeId,
    values: Vec<f64>,
) -> Result<(), HostError> {
    let snapshot = session.snapshot().map_err(HostError::from_session)?;
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
    if column.float_values().is_none() {
        return Err(HostError::InvalidValue(format!(
            "la colonne « {} » est de type texte : pyx.set_column n'écrit que des valeurs numériques",
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
    session
        .execute(Command::SetCells {
            column: id,
            start: 0,
            values: ValueBlock::Float(values),
        })
        .map(|_| ())
        .map_err(HostError::from_session)
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
