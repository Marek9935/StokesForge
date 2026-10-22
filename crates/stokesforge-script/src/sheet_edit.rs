//! Édition de classeur depuis pyst (#331, lot 2, S5) : lignes, type de colonne,
//! déplacement, duplication, texte d'une note.
//!
//! Aucune règle ici : chaque appel envoie la commande annulable que
//! l'interface emploie aussi (`InsertRows`, `DeleteRows`, `SetColumnType`,
//! `Move`, `DuplicateWorksheet`/`InsertCopy`, `SetNoteText`), dans le groupe
//! annulable du script. Les poignées sont revalidées sous la réservation.
//! Une écriture de colonne par le script met à jour la version qu'il avait lue
//! de cette colonne : ses écritures suivantes ne se refusent pas elles-mêmes.

use pyo3::prelude::*;
use stokesforge_commands::Command;
use stokesforge_core::{NodeCopy, NodeId};

use crate::bridge::{self, HostError};
use crate::engine::{self, ScriptContext};
use crate::graph::invalid;
use crate::surface::{to_py_error, Handle};

/// Les colonnes de la feuille `sheet` (celles que des lignes touchent).
fn sheet_columns(
    context: &ScriptContext,
    sheet: NodeId,
    epoch: u64,
) -> Result<Vec<NodeId>, HostError> {
    let snapshot = context
        .session()
        .snapshot()
        .map_err(HostError::from_session)?;
    Ok(bridge::checked_node(&snapshot, sheet, epoch)?
        .children()
        .iter()
        .copied()
        .filter(|id| {
            snapshot
                .tree
                .node(*id)
                .is_some_and(|node| node.as_column().is_some())
        })
        .collect())
}

/// Insère (`insert`) ou supprime des lignes au niveau de la feuille.
fn edit_rows(
    context: &ScriptContext,
    sheet: NodeId,
    epoch: u64,
    at: usize,
    count: usize,
    insert: bool,
) -> Result<(), HostError> {
    let snapshot = bridge::write_snapshot(context, sheet, epoch)?;
    if !bridge::checked_node(&snapshot, sheet, epoch)?
        .kind()
        .is_worksheet()
    {
        return Err(HostError::InvalidValue(format!(
            "{sheet} is not a worksheet"
        )));
    }
    // Le modèle ignore en silence une plage hors de toutes les colonnes ; pour
    // un script, c'est une faute de frappe qu'il vaut mieux dire.
    let rows = sheet_columns(context, sheet, epoch)?
        .into_iter()
        .filter_map(|id| snapshot.tree.node(id)?.as_column().map(|c| c.len()))
        .max()
        .unwrap_or(0);
    if (insert && at > rows) || (!insert && at >= rows) {
        return Err(HostError::InvalidValue(format!(
            "row {at} is outside the worksheet, which has {rows} rows"
        )));
    }
    let command = if insert {
        Command::InsertRows { sheet, at, count }
    } else {
        Command::DeleteRows { sheet, at, count }
    };
    context
        .session()
        .execute(command)
        .map_err(HostError::from_session)?;
    for column in sheet_columns(context, sheet, epoch)? {
        bridge::retain_written_version(context, column, epoch)?;
    }
    Ok(())
}

fn check_row_count(count: usize) -> PyResult<()> {
    let limit = stokesforge_core::column::MAX_COLUMN_ROWS;
    if count > limit {
        return Err(invalid(format!(
            "row count {count} exceeds the limit of {limit} rows"
        )));
    }
    Ok(())
}

/// Insère `count` lignes vides (une par défaut) à la ligne `at` (0 = en tête)
/// de la feuille : toutes ses colonnes d'au moins `at` lignes s'allongent, les
/// cellules suivantes descendent. `at` égal au nombre de lignes de la feuille ajoute
/// en fin ; une colonne plus courte que `at` n'est pas touchée. Annulable par
/// `pyst.undo()`.
///
/// Lève `InvalidValueError` pour une poignée qui n'est pas une feuille, un
/// `at` au-delà de la dernière ligne ou un total au-dessus du plafond du modèle ;
/// `BrokenReferenceError` pour une poignée périmée.
#[pyfunction]
#[pyo3(signature = (worksheet, at, count=1))]
pub(crate) fn insert_rows(
    py: Python<'_>,
    worksheet: &Handle,
    at: usize,
    count: usize,
) -> PyResult<()> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    check_row_count(count)?;
    let (id, epoch) = (worksheet.id, worksheet.epoch);
    py.detach(|| edit_rows(&context, id, epoch, at, count, true))
        .map_err(to_py_error)
}

/// Supprime `count` lignes (une par défaut) à partir de la ligne `at` (0 = la
/// première) dans les colonnes de la feuille qui ont une ligne `at`, et remonte
/// ce qui suit. Annulable par `pyst.undo()`, qui rend les cellules retirées.
///
/// Lève `InvalidValueError` pour une poignée qui n'est pas une feuille ou un
/// `at` au-delà de la dernière ligne ; `BrokenReferenceError` pour une poignée périmée.
#[pyfunction]
#[pyo3(signature = (worksheet, at, count=1))]
pub(crate) fn delete_rows(
    py: Python<'_>,
    worksheet: &Handle,
    at: usize,
    count: usize,
) -> PyResult<()> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let (id, epoch) = (worksheet.id, worksheet.epoch);
    py.detach(|| edit_rows(&context, id, epoch, at, count, false))
        .map_err(to_py_error)
}

/// Change le type d'une colonne : `'numeric'`, `'datetime'` ou `'mixed'` (les
/// colonnes texte sont `'mixed'`). **La conversion peut perdre des valeurs** (un
/// texte vers `'numeric'` ne se relit pas) ; `pyst.undo()` rend type et valeurs.
/// Rôle, unité et nom restent. Une colonne-formule refuse la conversion.
///
/// Lève `InvalidValueError` pour un type inconnu ou une poignée qui n'est pas
/// une colonne ; `BrokenReferenceError` pour une poignée périmée.
#[pyfunction]
#[pyo3(signature = (column, column_type))]
pub(crate) fn set_column_type(py: Python<'_>, column: &Handle, column_type: &str) -> PyResult<()> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let column_type = crate::surface::parse_column_type(column_type)?;
    let (id, epoch) = (column.id, column.epoch);
    py.detach(|| -> Result<(), HostError> {
        let snapshot = bridge::write_snapshot(&context, id, epoch)?;
        if bridge::checked_node(&snapshot, id, epoch)?
            .as_column()
            .is_none()
        {
            return Err(HostError::InvalidValue(format!("{id} is not a column")));
        }
        context
            .session()
            .execute(Command::SetColumnType {
                column: id,
                column_type,
            })
            .map_err(HostError::from_session)?;
        bridge::retain_written_version(&context, id, epoch)
    })
    .map_err(to_py_error)
}

/// Déplace un objet sous `parent` (dossier, classeur ou feuille selon sa
/// nature) à la position `index` parmi les autres enfants de `parent` ; sans
/// `index`, en dernier. Avec le même parent, c'est un simple réordonnancement.
/// La poignée reste valide. Annulable par `pyst.undo()`.
///
/// Lève `InvalidValueError` si `parent` n'accepte pas ce genre d'objet, si le
/// déplacement est circulaire, si le nom est déjà pris dans `parent`, si `index`
/// est hors limites ou si l'objet est la racine ; `BrokenReferenceError` pour
/// une poignée périmée.
#[pyfunction]
#[pyo3(name = "move", signature = (handle, parent, index=None))]
pub(crate) fn move_node(
    py: Python<'_>,
    handle: &Handle,
    parent: &Handle,
    index: Option<usize>,
) -> PyResult<()> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let (id, epoch) = (handle.id, handle.epoch);
    let (parent_id, parent_epoch) = (parent.id, parent.epoch);
    py.detach(|| -> Result<(), HostError> {
        let snapshot = bridge::write_snapshot(&context, id, epoch)?;
        let target = bridge::checked_node(&snapshot, parent_id, parent_epoch)?;
        let node = bridge::checked_node(&snapshot, id, epoch)?;
        let others = target.children().len() - usize::from(node.parent() == Some(parent_id));
        context
            .session()
            .execute(Command::Move {
                id,
                new_parent: parent_id,
                index: index.unwrap_or(others),
            })
            .map(|_| ())
            .map_err(HostError::from_session)
    })
    .map_err(to_py_error)
}

/// Le premier « <nom> copy N » libre parmi les enfants de `parent`.
fn free_copy_name(
    snapshot: &stokesforge_session::Snapshot,
    parent: NodeId,
    origin: &str,
) -> String {
    let taken = |name: &str| {
        snapshot
            .tree
            .children(parent)
            .unwrap_or_default()
            .iter()
            .any(|id| {
                snapshot
                    .tree
                    .node(*id)
                    .is_some_and(|node| node.name() == name)
            })
    };
    (1u64..)
        .map(|number| format!("{origin} copy {number}"))
        .find(|name| !taken(name))
        .unwrap_or_else(|| origin.to_owned())
}

/// Duplique un objet (et ses descendants, identifiants neufs) dans le même
/// parent et rend la poignée de la copie. Une feuille est copiée juste après
/// l'original, les autres objets en fin de parent. `name` nomme la copie ; sans
/// lui, le premier « <nom> copy N » libre. Annulable par `pyst.undo()`.
///
/// Lève `InvalidValueError` pour la racine, un nom refusé ou déjà pris, ou un
/// objet que le modèle ne copie pas ; `BrokenReferenceError` pour une poignée
/// périmée.
#[pyfunction]
#[pyo3(signature = (handle, name=None))]
pub(crate) fn duplicate(py: Python<'_>, handle: &Handle, name: Option<String>) -> PyResult<Handle> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let (id, epoch) = (handle.id, handle.epoch);
    let created = py
        .detach(|| {
            let snapshot = bridge::write_snapshot(&context, id, epoch)?;
            let node = bridge::checked_node(&snapshot, id, epoch)?;
            let parent = node.parent().ok_or_else(|| {
                HostError::InvalidValue("the project root cannot be duplicated".into())
            })?;
            let name = name.unwrap_or_else(|| free_copy_name(&snapshot, parent, node.name()));
            let command = if node.kind().is_worksheet() {
                Command::DuplicateWorksheet { sheet: id, name }
            } else {
                Command::InsertCopy {
                    parent,
                    copy: NodeCopy::capture(&snapshot.tree, id).map_err(HostError::from_core)?,
                    name,
                }
            };
            let before = snapshot.tree.children(parent).unwrap_or_default().to_vec();
            context
                .session()
                .execute(command)
                .map_err(HostError::from_session)?;
            // Sous la macro du script, personne d'autre n'écrit : l'enfant
            // apparu est la copie.
            let after = context
                .session()
                .snapshot()
                .map_err(HostError::from_session)?;
            after
                .tree
                .children(parent)
                .unwrap_or_default()
                .iter()
                .copied()
                .find(|child| !before.contains(child))
                .ok_or_else(|| HostError::InvalidValue("the copy was not created".into()))
        })
        .map_err(to_py_error)?;
    Ok(Handle { id: created, epoch })
}

/// Remplace le texte d'une note (`text` est le texte entier, sauts de ligne
/// compris). Annulable par `pyst.undo()`.
///
/// Lève `InvalidValueError` si la poignée n'est pas une note ;
/// `BrokenReferenceError` pour une poignée périmée.
#[pyfunction]
#[pyo3(signature = (note, text))]
pub(crate) fn set_note(py: Python<'_>, note: &Handle, text: String) -> PyResult<()> {
    let context = engine::current_context()?;
    context.check_interrupt()?;
    let (id, epoch) = (note.id, note.epoch);
    py.detach(|| bridge::mutate_node(&context, id, epoch, Command::SetNoteText { id, text }))
        .map_err(to_py_error)
}
