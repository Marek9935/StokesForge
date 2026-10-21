//! La surface `pyst` : navigation et lecture du projet, puis les écritures du socle.
//!
//! Le chantier Python (#65, lots #66 et #67) étend l'existant ; l'ancienne API v1
//! archivée n'est plus une cible à recopier. Les poignées et les lectures
//! fonctionnent sans état de vue, aussi bien pour la console que pour une
//! session sans interface.
//!
//! Deux lectures distinctes évitent une conversion implicite : `column`
//! conserve son vrai tableau numpy numérique sans copie ; `values` copie les
//! cellules en un tuple Python, sans perdre les textes ni confondre `None`,
//! une chaîne vide et un nombre NaN. Les métadonnées sont des dictionnaires
//! détachés du modèle : les modifier ne constitue jamais une écriture.
//!
//! # Vocabulaire d'erreurs
//!
//! Quatre exceptions, et seulement quatre : [`UnknownPathError`],
//! [`BrokenReferenceError`], [`InvalidValueError`], [`ConcurrentWriteError`].
//! Les trois premières sont reprises telles quelles de la spécification de
//! l'API v1, pour ne pas avoir à les renommer ensuite ; la quatrième
//! l'anticipe (A2, `guide-de-correction-robustesse.md` §F6, puis #312) :
//! `set_column` et `set_values` refusent aussitôt une écriture dont la colonne
//! a changé depuis sa lecture, et doivent distinguer ce conflit des autres refus. Toute
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
use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule, PyTuple};
use pyo3::IntoPyObjectExt;
use pyo3::{create_exception, wrap_pyfunction};
use stokesforge_commands::Command;
use stokesforge_core::{
    ColumnRole, ColumnType, ColumnValues, MixedCell, NodeId, NodeKind, ValueBlock,
};
use stokesforge_python::array::{borrowed_readonly_array, copy_array_to_vec};

use crate::bridge::{self, HostError};
use crate::engine;

create_exception!(
    pyst,
    UnknownPathError,
    PyException,
    "The given path is malformed, or does not designate any object of the project."
);
create_exception!(
    pyst,
    BrokenReferenceError,
    PyException,
    "The handle designates an object absent from the current project (deleted, or from a reloaded project)."
);
create_exception!(
    pyst,
    InvalidValueError,
    PyException,
    "Value refused: wrong type, wrong length, or object of an unexpected kind."
);
create_exception!(
    pyst,
    ConcurrentWriteError,
    PyException,
    "pyst.set_column or pyst.set_values was refused because another client modified the \
     column after the script read it. This is neither a script bug nor a refused value: \
     read the column again, then write."
);

/// Traduit une erreur de l'hôte, déjà classée, en exception Python.
pub(crate) fn to_py_error(error: HostError) -> PyErr {
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
/// constructeur Python et n'expose aucun entier. [`resolve`], [`root`],
/// [`children`], [`info`] et [`import_file`] en produisent.
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
#[pyclass(frozen, module = "pyst", name = "Handle")]
#[derive(Debug)]
pub struct Handle {
    /// Nœud désigné. Champ privé : rien, depuis Python, n'y donne accès.
    pub(crate) id: NodeId,
    /// Époque du projet au moment où [`resolve`] a produit cette poignée.
    pub(crate) epoch: u64,
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
/// La grammaire est celle de StokesForge : `[Book1]Sheet1!ColA`, éventuellement
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

/// Rend la poignée du dossier racine du projet courant.
#[pyfunction]
fn root(py: Python<'_>) -> PyResult<Handle> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let (id, epoch) = py
        .detach(|| bridge::root(context.session()))
        .map_err(to_py_error)?;
    Ok(Handle { id, epoch })
}

/// Rend les enfants directs dans leur ordre : dossiers et fenêtres d'un
/// dossier, feuilles d'un classeur, colonnes d'une feuille. Sans poignée,
/// part du dossier racine. Une colonne ou une autre feuille de l'arbre rend
/// une liste vide. Une poignée périmée lève `BrokenReferenceError`.
#[pyfunction]
#[pyo3(signature = (handle=None))]
fn children(py: Python<'_>, handle: Option<&Handle>) -> PyResult<Vec<Handle>> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let handle = handle.map(|handle| (handle.id, handle.epoch));
    let children = py
        .detach(|| bridge::children(context.session(), handle))
        .map_err(to_py_error)?;
    Ok(children
        .into_iter()
        .map(|(id, epoch)| Handle { id, epoch })
        .collect())
}

/// Décrit un nœud par un dictionnaire indépendant du modèle : `name`, `kind`,
/// `parent` (poignée ou None), `row_count` (feuille/colonne, sinon None),
/// `column` (métadonnées d'une colonne, sinon None). Le nombre de lignes d'une
/// feuille est le maximum des longueurs de ses colonnes, zéro si elle est vide.
/// Les métadonnées comprennent `type`, `role`, `long_name`, `unit`, `comment`, `x_prime`,
/// `display_format`, `formula` (en lettres) et `version`. Une poignée périmée lève
/// `BrokenReferenceError` ; aucune lecture ne change l'historique.
#[pyfunction]
fn info<'py>(py: Python<'py>, handle: &Handle) -> PyResult<Bound<'py, PyDict>> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let id = handle.id;
    let epoch = handle.epoch;
    let (node, row_count, formula) = py
        .detach(|| bridge::info(context.session(), id, epoch))
        .map_err(to_py_error)?;
    let result = PyDict::new(py);
    result.set_item("name", node.name())?;
    result.set_item(
        "kind",
        match node.kind() {
            NodeKind::Folder { .. } => "folder",
            NodeKind::Workbook { .. } => "workbook",
            NodeKind::Worksheet { .. } => "worksheet",
            NodeKind::Column(_) => "column",
            NodeKind::Graph { .. } => "graph",
            NodeKind::Matrix { .. } => "matrix",
            NodeKind::Note { .. } => "note",
            NodeKind::Unknown(_) => "unknown",
        },
    )?;
    result.set_item("parent", node.parent().map(|id| Handle { id, epoch }))?;
    result.set_item("row_count", row_count)?;
    if let Some(column) = node.as_column() {
        let meta = column.meta();
        let description = PyDict::new(py);
        description.set_item(
            "type",
            match column.column_type() {
                ColumnType::Numeric => "numeric",
                ColumnType::DateTime => "datetime",
                ColumnType::Mixed => "mixed",
            },
        )?;
        description.set_item(
            "role",
            match meta.role {
                ColumnRole::X => "x",
                ColumnRole::Y => "y",
                ColumnRole::X2 => "x2",
                ColumnRole::Y2 => "y2",
                ColumnRole::Z => "z",
                ColumnRole::XError => "x_error",
                ColumnRole::YError => "y_error",
                ColumnRole::Label => "label",
                ColumnRole::None => "none",
            },
        )?;
        description.set_item("long_name", meta.long_name.as_str())?;
        description.set_item("unit", meta.unit.as_str())?;
        description.set_item("comment", meta.comment.as_str())?;
        description.set_item("x_prime", meta.x_prime)?;
        description.set_item("display_format", meta.display_format.as_str())?;
        // En lettres : celles d'aujourd'hui, `#REF` pour une colonne lue qui
        // n'est plus dans la feuille (une formule `#REF` ne se réécrit pas).
        description.set_item("formula", formula.as_deref())?;
        description.set_item("version", column.version())?;
        result.set_item("column", description)?;
    } else {
        result.set_item("column", py.None())?;
    }
    Ok(result)
}

/// Copie les cellules stockées d'une colonne en un tuple Python : `float`,
/// `str` ou `None`. Ne réanalyse jamais un texte numérique et conserve les
/// distinctions entre cellule vide, chaîne vide et nombre NaN. Les dates
/// restent des timestamps POSIX en secondes. Le tuple est un instantané ;
/// pour une lecture numérique sans copie, utiliser `column`.
/// Lève `InvalidValueError` sur un autre nœud, `BrokenReferenceError` sur une
/// poignée périmée.
#[pyfunction]
fn values<'py>(py: Python<'py>, handle: &Handle) -> PyResult<Bound<'py, PyTuple>> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let id = handle.id;
    let epoch = handle.epoch;
    let (values, version) = py
        .detach(|| bridge::column_values(context.session(), id, epoch))
        .map_err(to_py_error)?;
    context.remember_version(id, epoch, version);
    let cells: Vec<Bound<'py, PyAny>> = match values {
        ColumnValues::Float(values) => values
            .iter()
            .map(|value| value.into_bound_py_any(py))
            .collect::<PyResult<_>>()?,
        ColumnValues::Mixed(values) => values
            .iter()
            .map(|cell| match cell {
                MixedCell::Empty => Ok(py.None().into_bound(py)),
                MixedCell::Number(value) => value.into_bound_py_any(py),
                MixedCell::Text(text) => text.into_bound_py_any(py),
            })
            .collect::<PyResult<_>>()?,
    };
    PyTuple::new(py, cells)
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
    let (buffer, version) = py
        .detach(|| bridge::float_buffer(session, id, epoch))
        .map_err(to_py_error)?;
    context.remember_version(id, epoch, version);
    borrowed_readonly_array(py, &buffer)
}

/// Écrit `valeurs` dans la colonne, **par le bus de commandes** : l'écriture
/// entre dans l'historique et s'annule par `pyst.undo()` comme par le `Ctrl+Z`
/// de la fenêtre.
///
/// `valeurs` doit être un tableau numpy de `float64` à une dimension ; les
/// tableaux non contigus (`v[::2]`) sont acceptés. Les valeurs sont
/// **copiées** vers Rust : le modèle possède ses buffers et ne dépend jamais
/// de la durée de vie d'un objet Python.
///
/// La longueur doit rester exactement celle de la colonne. Une autre
/// longueur lève `InvalidValueError` avant toute commande. Pour remplacer la
/// longueur et écrire du texte, utiliser `set_values`.
///
/// Écriture protégée : la colonne doit être inchangée depuis la dernière
/// lecture du script (`column`/`values`), sinon `ConcurrentWriteError`
/// aussitôt, sans retentative ni écrasement. `expected_version` (facultatif,
/// la `version` rendue par `info`) remplace cette version de lecture. Une
/// colonne jamais lue par le script n'est pas protégée. `undo()` oublie les
/// versions lues.
#[pyfunction]
#[pyo3(signature = (poignee, valeurs, *, expected_version=None))]
fn set_column(
    py: Python<'_>,
    poignee: &Handle,
    valeurs: &Bound<'_, PyAny>,
    expected_version: Option<u64>,
) -> PyResult<()> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let id = poignee.id;
    let epoch = poignee.epoch;
    // La copie se fait GIL en main ; le `Vec<f64>` obtenu est du Rust pur, il
    // peut traverser l'attente de l'acteur et se détruire sur n'importe quel
    // fil.
    if let Ok(array) = valeurs.cast::<numpy::PyUntypedArray>() {
        use numpy::PyUntypedArrayMethods;
        check_value_count(array.len())?;
    }
    let values = copy_array_to_vec(valeurs)
        .map_err(|error| InvalidValueError::new_err(error.to_string()))?;
    py.detach(move || bridge::set_column(&context, id, epoch, values, expected_version))
        .map_err(to_py_error)
}

/// Annule un cran de la pile d'historique de la session.
///
/// Rend `True` si quelque chose a été annulé, **`False` s'il n'y avait rien** —
/// jamais une exception : une pile vide n'est pas une erreur, c'est le cas que
/// l'interface traduit par un bouton grisé. C'est aussi ce qui s'écrit
/// naturellement dans une boucle « annuler tant qu'il y a quelque chose ».
///
/// Il n'y a **qu'une seule pile** dans la session : `pyst.undo()` annule aussi
/// bien une commande venue de l'interface qu'une commande venue du script.
#[pyfunction]
#[pyo3(text_signature = "()")]
fn undo(py: Python<'_>) -> PyResult<bool> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    py.detach(|| bridge::undo(&context)).map_err(to_py_error)
}

/// Importe un fichier dans un classeur neuf et rend les poignées de ses
/// feuilles. `recipe` nomme un modèle enregistré ou fourni ; sans ce nom,
/// le modèle est choisi automatiquement, sinon la recette permissive.
/// Colonnes, rôles, métadonnées et source sont ceux de l'assistant.
/// Un seul Ctrl+Z (ou `pyst.undo()`) annule tout l'import. Un fichier illisible
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
    let sheets = py
        .detach(|| bridge::import_file(&context, &path, recipe.as_deref()))
        .map_err(to_py_error)?;
    Ok(sheets
        .into_iter()
        .map(|(id, epoch)| Handle { id, epoch })
        .collect())
}

/// Crée un classeur vide dans `parent` (dossier), ou dans la racine.
/// Sans nom, utilise le premier BookN libre. Rend une poignée stable.
#[pyfunction]
#[pyo3(signature = (name=None, parent=None))]
fn create_workbook(
    py: Python<'_>,
    name: Option<String>,
    parent: Option<&Handle>,
) -> PyResult<Handle> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let parent = parent.map(|h| (h.id, h.epoch));
    let (id, epoch) = py
        .detach(|| {
            bridge::create_node(&context, parent, |parent| Command::AddEmptyWorkbook {
                parent,
                name,
            })
        })
        .map_err(to_py_error)?;
    Ok(Handle { id, epoch })
}

/// Crée une feuille vide en fin de classeur. Sans nom, utilise SheetN.
#[pyfunction]
#[pyo3(signature = (workbook, name=None))]
fn create_worksheet(py: Python<'_>, workbook: &Handle, name: Option<String>) -> PyResult<Handle> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let parent = (workbook.id, workbook.epoch);
    let (id, epoch) = py
        .detach(|| {
            bridge::create_node(&context, Some(parent), |book| Command::AddWorksheet {
                book,
                name,
            })
        })
        .map_err(to_py_error)?;
    Ok(Handle { id, epoch })
}

/// Crée une colonne vide en fin de feuille, de type numeric, datetime ou
/// mixed. Les colonnes texte utilisent mixed. Sans nom, utilise A, B, C…
#[pyfunction]
#[pyo3(signature = (worksheet, name=None, column_type="numeric"))]
fn create_column(
    py: Python<'_>,
    worksheet: &Handle,
    name: Option<String>,
    column_type: &str,
) -> PyResult<Handle> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let column_type = match column_type {
        "numeric" => ColumnType::Numeric,
        "datetime" => ColumnType::DateTime,
        "mixed" => ColumnType::Mixed,
        _ => {
            return Err(InvalidValueError::new_err(
                "unknown column type: use numeric, datetime or mixed",
            ))
        }
    };
    let parent = (worksheet.id, worksheet.epoch);
    let (id, epoch) = py
        .detach(|| {
            bridge::create_node(&context, Some(parent), |sheet| Command::AddColumn {
                sheet,
                index: None,
                name,
                column: stokesforge_core::Column::new(column_type),
            })
        })
        .map_err(to_py_error)?;
    Ok(Handle { id, epoch })
}

/// Renomme un objet sans changer sa poignée. Les règles de nom du modèle
/// s'appliquent : nom non vide, au plus 255 caractères, unique dans le parent.
#[pyfunction]
fn rename(py: Python<'_>, handle: &Handle, name: String) -> PyResult<()> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let (id, epoch) = (handle.id, handle.epoch);
    py.detach(|| bridge::mutate_node(&context, id, epoch, Command::Rename { id, name }))
        .map_err(to_py_error)
}

/// Supprime l'objet et ses descendants. La poignée redevient valide après
/// annulation. La racine du projet ne peut pas être supprimée.
#[pyfunction]
fn remove(py: Python<'_>, handle: &Handle) -> PyResult<()> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let (id, epoch) = (handle.id, handle.epoch);
    py.detach(|| bridge::mutate_node(&context, id, epoch, Command::RemoveNode { id }))
        .map_err(to_py_error)
}

/// Copie explicite des entrées avant toute attente d'acteur. La voie numpy
/// garde les contrôles existants : float64, une dimension, masque refusé.
/// La voie iterable conserve les textes tels quels, même "12" et "".
fn copy_cells(values: &Bound<'_, PyAny>) -> PyResult<ValueBlock> {
    let invalid = |message: String| InvalidValueError::new_err(message);
    if let Ok(array) = values.cast::<numpy::PyUntypedArray>() {
        use numpy::PyUntypedArrayMethods;
        check_value_count(array.len())?;
        return copy_array_to_vec(values)
            .map(ValueBlock::Float)
            .map_err(|err| invalid(err.to_string()));
    }
    if values.is_instance_of::<pyo3::types::PyString>() {
        return Err(invalid(
            "expected an iterable of cells, not a single string".into(),
        ));
    }
    // Ne pas croire __len__ pour borner l'itération : un générateur n'en a
    // pas, et un objet utilisateur peut mentir. Le comptage protège les deux.
    if let Ok(length) = values.len() {
        check_value_count(length)?;
    }
    let mut cells = Vec::new();
    for value in values
        .try_iter()
        .map_err(|_| invalid("expected an iterable of float, int, str or None cells".into()))?
    {
        let value = value?;
        check_value_count(cells.len() + 1)?;
        let cell = if value.is_none() {
            MixedCell::Empty
        } else if value.is_instance_of::<pyo3::types::PyString>() {
            MixedCell::Text(value.extract::<String>()?)
        } else if value.is_instance_of::<pyo3::types::PyBool>() {
            // bool est une sous-classe de int : le tester avant, sinon
            // True/False seraient écrits en silence comme 1.0/0.0.
            return Err(invalid(
                "bool is not a valid cell: convert it explicitly to int or float".into(),
            ));
        } else if value.is_instance_of::<pyo3::types::PyInt>()
            || value.is_instance_of::<pyo3::types::PyFloat>()
        {
            let number = value
                .extract::<f64>()
                .map_err(|err| invalid(err.to_string()))?;
            if value.is_instance_of::<pyo3::types::PyInt>()
                && value
                    .extract::<i64>()
                    .map_or(true, |n| n.unsigned_abs() > 9_007_199_254_740_992)
            {
                return Err(invalid(
                    "integer outside the exact float64 range: convert it explicitly before writing"
                        .into(),
                ));
            }
            MixedCell::Number(number)
        } else {
            return Err(invalid(
                "expected a float, int, str or None cell; convert other values explicitly".into(),
            ));
        };
        cells.push(cell);
    }
    Ok(ValueBlock::Mixed(cells))
}

/// Refuse avant de copier les gros tableaux ou de traverser l'acteur. Le
/// plafond est celui du modèle, pas une seconde valeur à garder en accord.
fn check_value_count(count: usize) -> PyResult<()> {
    let limit = stokesforge_core::column::MAX_COLUMN_ROWS;
    if count > limit {
        return Err(InvalidValueError::new_err(format!(
            "column length {count} exceeds the limit of {limit} rows"
        )));
    }
    Ok(())
}

/// Remplace toutes les cellules, en allongeant ou tronquant cette seule
/// colonne. Accepte numpy float64 1D ou un iterable de float/int/str/None.
/// Numeric/datetime refusent les textes et traduisent None en NaN ; mixed
/// conserve None, texte vide, texte numérique, NaN et zéro signé distincts.
/// Copie les entrées ; le type et les métadonnées restent inchangés. `True`
/// et `False` sont refusés (`InvalidValueError`, colonne inchangée). L'écriture
/// et la troncature de la queue forment une seule opération atomique. La
/// colonne doit être inchangée depuis la dernière lecture du script
/// (`column`/`values`), ou depuis `expected_version` si fourni (facultatif) :
/// sinon `ConcurrentWriteError` aussitôt, sans écrasement. Une colonne jamais
/// lue n'est pas protégée.
#[pyfunction]
#[pyo3(signature = (handle, values, *, expected_version=None))]
fn set_values(
    py: Python<'_>,
    handle: &Handle,
    values: &Bound<'_, PyAny>,
    expected_version: Option<u64>,
) -> PyResult<()> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let cells = copy_cells(values)?;
    context.check_interrupt()?;
    let (id, epoch) = (handle.id, handle.epoch);
    py.detach(|| bridge::set_values(&context, id, epoch, cells, expected_version))
        .map_err(to_py_error)
}

/// Modifie les seuls champs nommés : role (x, y, x2, y2, z, x_error, y_error,
/// label, none), long_name, unit, comment, display_format, formula.
/// Les valeurs sont des str ; formula=None retire la formule. La formule se
/// saisit en lettres : une lettre qui ne désigne aucune colonne de la feuille,
/// la colonne elle-même ou `#REF` lèvent `InvalidValueError`. Type, version
/// et provenance sont préservés. Une colonne-formule refuse les écritures.
#[pyfunction]
#[pyo3(signature = (handle, **metadata))]
fn set_column_meta(
    py: Python<'_>,
    handle: &Handle,
    metadata: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let mut changes = Vec::new();
    if let Some(metadata) = metadata {
        for (key, value) in metadata.iter() {
            let key = key.extract::<String>()?;
            if !matches!(
                key.as_str(),
                "role" | "long_name" | "unit" | "comment" | "display_format" | "formula"
            ) {
                return Err(InvalidValueError::new_err(format!(
                    "unknown column metadata field: {key}"
                )));
            }
            let value =
                if key == "formula" && value.is_none() {
                    None
                } else {
                    Some(value.extract::<String>().map_err(|_| {
                        InvalidValueError::new_err(format!("{key} must be a string"))
                    })?)
                };
            changes.push((key, value));
        }
    }
    let (id, epoch) = (handle.id, handle.epoch);
    py.detach(|| bridge::set_column_meta(&context, id, epoch, changes))
        .map_err(to_py_error)
}

/// Pose un objet graphique sur un graphe et rend son rang dans le panneau.
///
/// `kind` : `"rectangle"`, `"ellipse"`, `"line"` ou `"arrow"` (pointe en
/// `(x2, y2)`). Les points sont en **coordonnées de données** du panneau
/// `panel = (ligne, colonne)` et de l'ordonnée `axis` (`"left"` ou
/// `"right"`) : l'objet suit le zoom. Couleurs `"#RRGGBB"` ou `"#RRGGBBAA"` ;
/// `line_color=""` = automatique, `fill_color=""` = aucun remplissage.
/// L'objet est posé au-dessus des autres. Les écritures d'une exécution sont
/// groupées : un `pyst.undo()` ou Ctrl+Z les annule ensemble.
#[pyfunction]
#[pyo3(signature = (graph, kind, x1, y1, x2, y2, *, panel=(0, 0), axis="left", line_color="", line_width=1.5, fill_color=""))]
#[allow(clippy::too_many_arguments)]
fn add_shape(
    py: Python<'_>,
    graph: &Handle,
    kind: &str,
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    panel: (usize, usize),
    axis: &str,
    line_color: &str,
    line_width: f64,
    fill_color: &str,
) -> PyResult<usize> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let args = bridge::ShapeArgs {
        kind: kind.to_owned(),
        points: (x1, y1, x2, y2),
        panel,
        axis: axis.to_owned(),
        line_color: line_color.to_owned(),
        line_width,
        fill_color: fill_color.to_owned(),
    };
    let (id, epoch) = (graph.id, graph.epoch);
    py.detach(|| bridge::add_shape(&context, id, epoch, &args))
        .map_err(to_py_error)
}

/// Retire l'objet graphique de rang `index` du panneau `panel`. Les objets
/// au-dessus de lui descendent d'un rang. Annulable par `pyst.undo()`.
#[pyfunction]
#[pyo3(signature = (graph, index, *, panel=(0, 0)))]
fn remove_shape(
    py: Python<'_>,
    graph: &Handle,
    index: usize,
    panel: (usize, usize),
) -> PyResult<()> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let (id, epoch) = (graph.id, graph.epoch);
    py.detach(|| bridge::remove_shape(&context, id, epoch, panel, index))
        .map_err(to_py_error)
}

/// Documentation du module telle qu'un script la voit (`help(pyst)`).
const MODULE_DOC: &str = "\
Surface de script de StokesForge : navigation, lecture, écriture, annulation et import.

    pyst.root()                     -> Handle (dossier racine)
    pyst.children(handle=None)      -> list[Handle] (enfants directs ordonnés)
    pyst.info(handle)               -> dict (nom, nature, parent, lignes, métadonnées)
    pyst.values(handle)             -> tuple (copie des cellules float/str/None)
    pyst.resolve(chemin)            -> Handle
    pyst.column(poignee)            -> numpy.ndarray (lecture seule, sans copie)
    pyst.set_column(poignee, tab)   -> None (passe par l'historique)
    pyst.create_workbook(name=None, parent=None) -> Handle (classeur vide)
    pyst.create_worksheet(workbook, name=None) -> Handle
    pyst.create_column(worksheet, name=None, column_type=\"numeric\") -> Handle
    pyst.rename(handle, name)       -> None
    pyst.remove(handle)             -> None
    pyst.set_values(handle, values) -> None (remplacement, longueur comprise)
    pyst.set_column_meta(handle, **metadata) -> None
    pyst.plot(y, x=None, *, graph=None, name=None, parent=None, **options) -> Handle
                  (y : une colonne, ou une liste de colonnes = un groupe de séries ;
                   label=[...] un nom par série, group_name='...')
    pyst.series(graph, *, panel=(0, 0)) -> list[dict] (index, name, group, color, visible, y)
    pyst.series_groups(graph, *, panel=(0, 0)) -> list[dict] (id, name, members)
    pyst.group_series(graph, indices, *, name=None, panel=(0, 0)) -> int (numéro du groupe)
    pyst.set_series_group(graph, index, group, *, panel=(0, 0)) -> int (nouveau rang ;
                  group=None sort la série de son groupe)
    pyst.rename_group(graph, group, name, *, panel=(0, 0)) -> None ('' = nom automatique)
    pyst.ungroup(graph, group, *, panel=(0, 0)) -> None (les séries restent)
    pyst.remove_group(graph, group, *, panel=(0, 0)) -> None (supprime aussi les séries)
    pyst.set_group_style(graph, group, *, panel=(0, 0), line_style=None, line_width=None,
                  symbol=None, symbol_size=None) -> None
    pyst.apply_gradient(graph, group, gradient, *, panel=(0, 0)) -> None (nom d'un nuancier
                  fourni ou enregistré dans le projet, ou liste d'arrêts '#RRGGBB')
    pyst.add_shape(graph, kind, x1, y1, x2, y2, *, panel=(0, 0), axis='left',
                  line_color='', line_width=1.5, fill_color='') -> int
    pyst.remove_shape(graph, index, *, panel=(0, 0)) -> None
    pyst.undo()                     -> bool (False si rien à annuler)
    pyst.import_file(path, recipe=None) -> list[Handle] (feuilles d'un nouveau classeur)
    pyst.analysis.baseline / correct / integrate / fit / peaks / reconvolve /
        deconvolve(y, x=None, ...) -> dict (voir help(pyst.analysis))

Erreurs : UnknownPathError, BrokenReferenceError, InvalidValueError,
          ConcurrentWriteError.

Limites connues de cette surface :
  - la sortie (print) n'est rendue qu'à la fin du script, pas au fil de l'eau ;
  - un seul Ctrl+Z annule les écritures du script ;
    une erreur non rattrapée ou un arrêt demandé annule le groupe courant ;
  - undo() termine puis annule le groupe courant ;
  - set_column exige la longueur exacte ; set_values remplace aussi la longueur ;
  - les autres clients ne peuvent pas écrire pendant le groupe du script ;
  - une Handle ne survit pas à un rechargement de projet (BrokenReferenceError
    à la prochaine utilisation) ;
  - atexit ne s'exécute jamais (l'interpréteur n'est pas finalisé) : tout ce
    qui doit être écrit sur disque doit l'être explicitement ;
  - une interruption ne coupe pas un appel natif long (une opération numpy sur
    dix millions de points ira à son terme).
";

/// Construit le module `pyst` et l'enregistre dans `sys.modules`.
///
/// Le module est **sans état** : les fonctions retrouvent la session par
/// le contexte du fil courant (cf. [`crate::engine`]). Deux moteurs de script
/// vivant dans le même processus peuvent donc partager le même module sans se
/// gêner, et les quatre types d'exception sont des singletons du processus —
/// un script attrape le même `pyst.UnknownPathError` quel que soit le moteur.
pub(crate) fn install(py: Python<'_>) -> PyResult<()> {
    let module = PyModule::new(py, "pyst")?;
    module.add("__doc__", MODULE_DOC)?;
    module.add_class::<Handle>()?;
    module.add_function(wrap_pyfunction!(resolve, &module)?)?;
    module.add_function(wrap_pyfunction!(root, &module)?)?;
    module.add_function(wrap_pyfunction!(children, &module)?)?;
    module.add_function(wrap_pyfunction!(info, &module)?)?;
    module.add_function(wrap_pyfunction!(values, &module)?)?;
    module.add_function(wrap_pyfunction!(column, &module)?)?;
    module.add_function(wrap_pyfunction!(set_column, &module)?)?;
    module.add_function(wrap_pyfunction!(create_workbook, &module)?)?;
    module.add_function(wrap_pyfunction!(create_worksheet, &module)?)?;
    module.add_function(wrap_pyfunction!(create_column, &module)?)?;
    module.add_function(wrap_pyfunction!(rename, &module)?)?;
    module.add_function(wrap_pyfunction!(remove, &module)?)?;
    module.add_function(wrap_pyfunction!(set_values, &module)?)?;
    module.add_function(wrap_pyfunction!(set_column_meta, &module)?)?;
    module.add_function(wrap_pyfunction!(crate::graph::plot, &module)?)?;
    module.add_function(wrap_pyfunction!(add_shape, &module)?)?;
    module.add_function(wrap_pyfunction!(remove_shape, &module)?)?;
    for function in [
        wrap_pyfunction!(crate::groups::series, &module)?,
        wrap_pyfunction!(crate::groups::series_groups, &module)?,
        wrap_pyfunction!(crate::groups::group_series, &module)?,
        wrap_pyfunction!(crate::groups::set_series_group, &module)?,
        wrap_pyfunction!(crate::groups::rename_group, &module)?,
        wrap_pyfunction!(crate::groups::ungroup, &module)?,
        wrap_pyfunction!(crate::groups::remove_group, &module)?,
        wrap_pyfunction!(crate::groups::set_group_style, &module)?,
        wrap_pyfunction!(crate::groups::apply_gradient, &module)?,
    ] {
        module.add_function(function)?;
    }
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
    crate::analysis::install(py, &module)?;
    py.import("sys")?
        .getattr("modules")?
        .set_item("pyst", &module)?;
    Ok(())
}
