//! La surface `pyx` : quatre appels du socle, plus l'import de fichiers.
//!
//! Ce n'est **pas** l'API v1 destinée aux utilisateurs — celle-là est
//! spécifiée ailleurs et reste le contrat pour plus tard. Les quatre appels du socle ne
//! sont pas choisis pour leur utilité mais parce qu'**ensemble ils rendent le
//! critère de fin du socle vérifiable** :
//!
//! | Appel | Ce qu'il éprouve |
//! |---|---|
//! | [`resolve`] | poignées opaques, grammaire de chemins |
//! | [`column`] | vue numpy **sans copie**, `Arc` maintenu vivant |
//! | [`set_column`] | mutation via l'acteur et le bus, GIL relâché pendant l'attente |
//! | [`undo`] | délégation à la pile d'historique depuis Python |
//! | [`import_file`] | import commun au CLI et à l'application, en une macro annulable |
//!
//! # Vocabulaire d'erreurs
//!
//! Quatre exceptions, et seulement quatre : [`UnknownPathError`],
//! [`BrokenReferenceError`], [`InvalidValueError`], [`ConcurrentWriteError`].
//! Les trois premières sont reprises telles quelles de la spécification de
//! l'API v1, pour ne pas avoir à les renommer ensuite ; la quatrième
//! l'anticipe (A2, `guide-de-correction-robustesse.md` §F6) : `set_column`
//! peut désormais retenter une écriture refusée pour cause de concurrence, et
//! doit distinguer cet épuisement de tentatives des autres refus. Toute
//! erreur de l'hôte (`SessionError`, `CommandError`, `CoreError`) est
//! **traduite** en l'une d'elles (cf. [`crate::bridge`]), jamais transportée
//! telle quelle à travers le canal de l'acteur.
//!
//! # Où le GIL est relâché
//!
//! Chaque appel prépare ses
//! arguments en Rust pur, puis traverse `Python::detach` pour attendre
//! l'acteur. Le marqueur `Ungil` interdit à la compilation qu'une valeur liée
//! au GIL entre dans ce bloc ou en ressorte.

use numpy::PyArray1;
use plotyx_core::NodeId;
use plotyx_python::array::{borrowed_readonly_array, copy_array_to_vec};
use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::types::PyModule;
use pyo3::{create_exception, wrap_pyfunction};

use crate::bridge::{self, HostError};
use crate::engine;

create_exception!(
    pyx,
    UnknownPathError,
    PyException,
    "Le chemin donné est mal formé, ou ne désigne aucun objet du projet."
);
create_exception!(
    pyx,
    BrokenReferenceError,
    PyException,
    "La poignée désigne un objet absent du projet courant (supprimé, ou d'un projet rechargé)."
);
create_exception!(
    pyx,
    InvalidValueError,
    PyException,
    "Valeur refusée : mauvais type, mauvaise longueur, ou objet de nature inattendue."
);
create_exception!(
    pyx,
    ConcurrentWriteError,
    PyException,
    "pyx.set_column a été refusée cinq fois de suite par un autre client qui modifiait la \
     même colonne. Ce n'est pas un bug du script ni une valeur refusée : relisez l'état \
     courant, ou retentez plus tard."
);

/// Traduit une erreur de l'hôte, déjà classée, en exception Python.
fn to_py_error(error: HostError) -> PyErr {
    match error {
        HostError::UnknownPath(message) => UnknownPathError::new_err(message),
        HostError::BrokenReference(message) => BrokenReferenceError::new_err(message),
        HostError::InvalidValue(message) => InvalidValueError::new_err(message),
        HostError::ConcurrentWrite(message) => ConcurrentWriteError::new_err(message),
    }
}

/// Poignée **opaque** vers un objet du projet.
///
/// Un script ne peut ni l'inspecter ni la fabriquer : elle n'a pas de
/// constructeur Python et n'expose aucun entier. [`resolve`] et
/// [`import_file`] en produisent.
///
/// Une poignée reste valide après un renommage, après un déplacement, et après
/// une suppression **suivie d'une annulation** : c'est le contrat de
/// l'identité, assuré par le modèle lui-même (identifiants monotones, jamais
/// réutilisés). Entre la suppression et l'annulation, l'utiliser lève
/// `BrokenReferenceError`.
///
/// **Elle ne survit en revanche pas à un rechargement de projet** (A3,
/// `guide-de-correction-robustesse.md` §F2/F6) : `epoch` mémorise l'identité
/// du projet au moment de [`resolve`], et toute fonction qui reçoit une
/// `Handle` la compare à l'époque courante **avant** de chercher `id` dans
/// l'arbre — sans quoi un `NodeId` qui existe par coïncidence dans le
/// nouveau projet (même forme de fichier, mêmes identifiants réattribués)
/// désignerait silencieusement un tout autre objet.
#[pyclass(frozen, module = "pyx", name = "Handle")]
#[derive(Debug)]
pub struct Handle {
    /// Nœud désigné. Champ privé : rien, depuis Python, n'y donne accès.
    id: NodeId,
    /// Époque du projet au moment où [`resolve`] a produit cette poignée.
    epoch: u64,
}

#[pymethods]
impl Handle {
    /// Représentation lisible, par exemple `Handle(nœud 412)`.
    fn __repr__(&self) -> String {
        format!("Handle({})", self.id)
    }
}

/// Résout un chemin d'objet et rend une poignée opaque.
///
/// La grammaire est celle de PlotYx : `[Book1]Sheet1!ColA`, éventuellement
/// préfixée de dossiers (`/Manips/[Book1]Sheet1!ColA`).
///
/// Lève `UnknownPathError` si le chemin est mal formé **ou** s'il ne désigne
/// rien : les deux cas sont pour un script la même impasse.
#[pyfunction]
#[pyo3(text_signature = "(chemin)")]
fn resolve(py: Python<'_>, chemin: &str) -> PyResult<Handle> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let session = context.session();
    let (id, epoch) = py
        .detach(|| bridge::resolve(session, chemin))
        .map_err(to_py_error)?;
    Ok(Handle { id, epoch })
}

/// Rend une vue numpy **en lecture seule et sans copie** sur les valeurs de la
/// colonne.
///
/// Le tableau pointe sur la mémoire réelle du modèle : aucun octet n'est
/// dupliqué, quelle que soit la taille de la colonne.
///
/// **Une vue est un instantané, pas un abonnement.** Une vue prise avant une
/// écriture continue de montrer les anciennes valeurs, silencieusement. Règle
/// simple qui évite tout le problème : ne gardez pas une vue plus longtemps
/// qu'un calcul, et relisez la colonne après toute écriture.
///
/// Lève `BrokenReferenceError` si la poignée ne désigne plus rien, et
/// `InvalidValueError` si elle ne désigne pas une colonne numérique (une
/// feuille, un classeur, une colonne texte).
#[pyfunction]
#[pyo3(text_signature = "(poignee)")]
fn column<'py>(py: Python<'py>, poignee: &Handle) -> PyResult<Bound<'py, PyArray1<f64>>> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let session = context.session();
    let id = poignee.id;
    let epoch = poignee.epoch;
    let buffer = py
        .detach(|| bridge::float_buffer(session, id, epoch))
        .map_err(to_py_error)?;
    borrowed_readonly_array(py, &buffer)
}

/// Écrit `valeurs` dans la colonne, **par le bus de commandes** : l'écriture
/// entre dans l'historique et s'annule par `pyx.undo()` comme par le `Ctrl+Z`
/// de la fenêtre.
///
/// `valeurs` doit être un tableau numpy de `float64` à une dimension ; les
/// tableaux non contigus (`v[::2]`) sont acceptés. Les valeurs sont
/// **copiées** vers Rust : le modèle possède ses buffers et ne dépend jamais
/// de la durée de vie d'un objet Python.
///
/// **Restriction assumée de la surface-témoin** : la longueur doit être
/// **exactement** celle de la colonne. Cette surface n'allonge ni ne tronque —
/// aucune des huit commandes du socle ne tronque une colonne, et en inventer
/// une pour un banc d'essai serait un mauvais échange. La surface v1 fera
/// mieux. Toute autre longueur lève `InvalidValueError`, **sans envoyer la
/// moindre commande** : la révision du projet ne bouge pas.
#[pyfunction]
#[pyo3(text_signature = "(poignee, valeurs)")]
fn set_column(py: Python<'_>, poignee: &Handle, valeurs: &Bound<'_, PyAny>) -> PyResult<()> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let id = poignee.id;
    let epoch = poignee.epoch;
    // La copie se fait GIL en main ; le `Vec<f64>` obtenu est du Rust pur, il
    // peut traverser l'attente de l'acteur et se détruire sur n'importe quel
    // fil.
    let values = copy_array_to_vec(valeurs)
        .map_err(|error| InvalidValueError::new_err(error.to_string()))?;
    let session = context.session();
    py.detach(move || bridge::set_column(session, id, epoch, values))
        .map_err(to_py_error)
}

/// Annule un cran de la pile d'historique de la session.
///
/// Rend `True` si quelque chose a été annulé, **`False` s'il n'y avait rien** —
/// jamais une exception : une pile vide n'est pas une erreur, c'est le cas que
/// l'interface traduit par un bouton grisé. C'est aussi ce qui s'écrit
/// naturellement dans une boucle « annuler tant qu'il y a quelque chose ».
///
/// Il n'y a **qu'une seule pile** dans la session : `pyx.undo()` annule aussi
/// bien une commande venue de l'interface qu'une commande venue du script.
#[pyfunction]
#[pyo3(text_signature = "()")]
fn undo(py: Python<'_>) -> PyResult<bool> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let session = context.session();
    py.detach(|| bridge::undo(session)).map_err(to_py_error)
}

/// Importe un fichier dans un classeur neuf et rend les poignées de ses
/// feuilles. `recipe` nomme un modèle enregistré ou fourni ; sans ce nom,
/// le modèle est choisi automatiquement, sinon la recette permissive.
/// Colonnes, rôles, métadonnées et source sont ceux de l'assistant.
/// Un seul Ctrl+Z (ou `pyx.undo()`) annule tout l'import. Un fichier illisible
/// ou un modèle inconnu lève `InvalidValueError` sans toucher au projet.
#[pyfunction]
#[pyo3(signature = (path, recipe=None))]
fn import_file(
    py: Python<'_>,
    path: std::path::PathBuf,
    recipe: Option<String>,
) -> PyResult<Vec<Handle>> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let session = context.session();
    let sheets = py
        .detach(|| bridge::import_file(session, &path, recipe.as_deref()))
        .map_err(to_py_error)?;
    Ok(sheets
        .into_iter()
        .map(|(id, epoch)| Handle { id, epoch })
        .collect())
}

/// Documentation du module telle qu'un script la voit (`help(pyx)`).
const MODULE_DOC: &str = "\
Surface de script de PlotYx : lecture, écriture, annulation et import.

    pyx.resolve(chemin)            -> Handle
    pyx.column(poignee)            -> numpy.ndarray (lecture seule, sans copie)
    pyx.set_column(poignee, tab)   -> None (passe par l'historique)
    pyx.undo()                     -> bool (False si rien à annuler)
    pyx.import_file(path, recipe=None) -> list[Handle] (feuilles d'un nouveau classeur)

Erreurs : UnknownPathError, BrokenReferenceError, InvalidValueError,
          ConcurrentWriteError.

Limites connues de cette surface :
  - la sortie (print) n'est rendue qu'à la fin du script, pas au fil de l'eau ;
  - set_column exige la longueur exacte de la colonne, et retente jusqu'à
    cinq fois si un autre client modifie la colonne pendant l'écriture
    (ConcurrentWriteError au-delà) ;
  - une Handle ne survit pas à un rechargement de projet (BrokenReferenceError
    à la prochaine utilisation) ;
  - atexit ne s'exécute jamais (l'interpréteur n'est pas finalisé) : tout ce
    qui doit être écrit sur disque doit l'être explicitement ;
  - une interruption ne coupe pas un appel natif long (une opération numpy sur
    dix millions de points ira à son terme).
";

/// Construit le module `pyx` et l'enregistre dans `sys.modules`.
///
/// Le module est **sans état** : les fonctions retrouvent la session par
/// le contexte du fil courant (cf. [`crate::engine`]). Deux moteurs de script
/// vivant dans le même processus peuvent donc partager le même module sans se
/// gêner, et les quatre types d'exception sont des singletons du processus —
/// un script attrape le même `pyx.UnknownPathError` quel que soit le moteur.
pub(crate) fn install(py: Python<'_>) -> PyResult<()> {
    let module = PyModule::new(py, "pyx")?;
    module.add("__doc__", MODULE_DOC)?;
    module.add_class::<Handle>()?;
    module.add_function(wrap_pyfunction!(resolve, &module)?)?;
    module.add_function(wrap_pyfunction!(column, &module)?)?;
    module.add_function(wrap_pyfunction!(set_column, &module)?)?;
    module.add_function(wrap_pyfunction!(undo, &module)?)?;
    module.add_function(wrap_pyfunction!(import_file, &module)?)?;
    module.add("UnknownPathError", py.get_type::<UnknownPathError>())?;
    module.add(
        "BrokenReferenceError",
        py.get_type::<BrokenReferenceError>(),
    )?;
    module.add("InvalidValueError", py.get_type::<InvalidValueError>())?;
    module.add(
        "ConcurrentWriteError",
        py.get_type::<ConcurrentWriteError>(),
    )?;
    py.import("sys")?
        .getattr("modules")?
        .set_item("pyx", &module)?;
    Ok(())
}
