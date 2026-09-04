"""Lecture d'un fichier `.plx` (HDF5) vers un `Project`.

**Hors bus, à dessein.** Comme `_default_project()` dans `ui/main_window.py`,
construire un projet chargé depuis un fichier ne mute rien qu'un `undo`
pourrait vouloir défaire : il n'y a rien avant. C'est à l'appelant (l'UI)
de remplacer le projet courant et de repartir d'une pile d'undo neuve —
`load_project` ne fait qu'un projet, jamais plus.

**Identité restaurée, pas régénérée.** Chaque nœud reprend, via
`ModelObject._restore_key`, la clé UUID qu'il portait à l'écriture (voir
`plotyx.persistence.writer`). La construction se fait toujours **détachée**
— une colonne, une feuille, un classeur entier sont bâtis avant d'être
insérés dans leur parent déjà vivant dans l'arbre — de sorte qu'aucune
inscription au registre n'a lieu avant que la clé finale soit en place :
`attach()` (appelé par les primitives `insert_*`) est le seul point
d'inscription, sous la clé courante à cet instant précis.
"""

from __future__ import annotations

from pathlib import Path

import h5py
import numpy as np

from plotyx.core.column import Column, ColumnRole, ColumnType
from plotyx.core.node import ModelObject
from plotyx.core.nodes import Folder, Graph, Matrix, Note, WindowNode, Workbook
from plotyx.core.project import Project
from plotyx.core.worksheet import Worksheet
from plotyx.persistence.errors import CorruptedProjectFileError
from plotyx.persistence.migration import check_version, migrate
from plotyx.persistence.schema import (
    ATTR_COLUMN_TYPE,
    ATTR_COMMENT,
    ATTR_DISPLAY_FORMAT,
    ATTR_FORMAT,
    ATTR_KEY,
    ATTR_LABEL,
    ATTR_LONG_NAME,
    ATTR_METADATA_JSON,
    ATTR_NAME,
    ATTR_ORDER,
    ATTR_ROLE,
    ATTR_TEXT,
    ATTR_TYPE,
    ATTR_UNIT,
    DATASET_MISSING,
    DATASET_VALUES,
    FORMAT_MARKER,
    GROUP_PROJECT,
    TYPE_FOLDER,
    TYPE_GRAPH,
    TYPE_MATRIX,
    TYPE_NOTE,
    TYPE_WORKBOOK,
)

__all__ = ["load_project"]


def load_project(path: str | Path) -> Project:
    path = Path(path)
    with h5py.File(path, "r") as file:
        _check_format_marker(file, path)
        version = check_version(file)
        migrate(file, version)
        root_group = file[GROUP_PROJECT]
        project = Project(_attr_str(root_group, ATTR_NAME))
        project._restore_root_key(_attr_str(root_group, ATTR_KEY))
        for name in _read_order(root_group):
            child = _read_folder_child(root_group[name], name)
            project.root.insert_child(len(project.root.children()), child)
    return project


def _check_format_marker(file: h5py.File, path: Path) -> None:
    marker = file.attrs.get(ATTR_FORMAT)
    if marker != FORMAT_MARKER or GROUP_PROJECT not in file:
        raise CorruptedProjectFileError(
            f"{path} n'est pas reconnu comme un fichier projet PlotYx (.plx)."
        )


# -- utilitaires de lecture d'attributs ---------------------------------------


def _attr_str(obj: h5py.Group | h5py.Dataset, name: str, default: str = "") -> str:
    value = obj.attrs.get(name, default)
    if isinstance(value, bytes):
        return value.decode("utf-8")
    return str(value)


def _read_order(group: h5py.Group) -> list[str]:
    raw = group.attrs.get(ATTR_ORDER, [])
    return [v.decode("utf-8") if isinstance(v, bytes) else str(v) for v in raw]


def _apply_window_common(group: h5py.Group, node: WindowNode) -> None:
    node.label = _attr_str(group, ATTR_LABEL)
    node.comment = _attr_str(group, ATTR_COMMENT)
    raw_metadata = group.attrs.get(ATTR_METADATA_JSON)
    if raw_metadata:
        import json

        node.metadata.update(json.loads(raw_metadata))


# -- structure ------------------------------------------------------------


def _read_folder_child(group: h5py.Group, name: str) -> ModelObject:
    node_type = _attr_str(group, ATTR_TYPE)
    if node_type == TYPE_FOLDER:
        return _read_folder(group, name)
    if node_type == TYPE_WORKBOOK:
        return _read_workbook(group, name)
    if node_type in (TYPE_GRAPH, TYPE_MATRIX, TYPE_NOTE):
        return _read_minimal_window(group, name, node_type)
    raise CorruptedProjectFileError(f"type de nœud inconnu dans le fichier : {node_type!r}")


def _read_folder(group: h5py.Group, name: str) -> Folder:
    folder = Folder(name)
    folder._restore_key(_attr_str(group, ATTR_KEY))
    for child_name in _read_order(group):
        child = _read_folder_child(group[child_name], child_name)
        folder.insert_child(len(folder.children()), child)
    return folder


def _read_workbook(group: h5py.Group, name: str) -> Workbook:
    book = Workbook(name, sheets=[])
    book._restore_key(_attr_str(group, ATTR_KEY))
    _apply_window_common(group, book)
    for sheet_name in _read_order(group):
        sheet = _read_worksheet(group[sheet_name], sheet_name)
        book.insert_sheet(len(book.sheets), sheet)
    return book


def _read_worksheet(group: h5py.Group, name: str) -> Worksheet:
    sheet = Worksheet(name)
    sheet._restore_key(_attr_str(group, ATTR_KEY))
    for column_name in _read_order(group):
        column = _read_column(group[column_name], column_name)
        sheet.insert_column(len(sheet.columns), column)
    return sheet


def _read_minimal_window(group: h5py.Group, name: str, node_type: str) -> WindowNode:
    if node_type == TYPE_NOTE:
        node: WindowNode = Note(name, _attr_str(group, ATTR_TEXT))
    elif node_type == TYPE_GRAPH:
        node = Graph(name)
    else:
        node = Matrix(name)
    node._restore_key(_attr_str(group, ATTR_KEY))
    _apply_window_common(group, node)
    return node


# -- colonnes -----------------------------------------------------------------


def _read_column(obj: h5py.Group | h5py.Dataset, name: str) -> Column:
    column_type = ColumnType(_attr_str(obj, ATTR_COLUMN_TYPE))
    if isinstance(obj, h5py.Group):
        raw_values = obj[DATASET_VALUES][()]
        missing = obj[DATASET_MISSING][()]
        values: list[str | None] = []
        for value, is_missing in zip(raw_values, missing, strict=True):
            if is_missing:
                values.append(None)
            else:
                values.append(value.decode("utf-8") if isinstance(value, bytes) else value)
    else:
        values = np.asarray(obj[()], dtype=np.float64)

    column = Column(
        name,
        values,
        column_type=column_type,
        role=ColumnRole(_attr_str(obj, ATTR_ROLE)),
        unit=_attr_str(obj, ATTR_UNIT),
        long_name=_attr_str(obj, ATTR_LONG_NAME),
        comment=_attr_str(obj, ATTR_COMMENT),
        display_format=_attr_str(obj, ATTR_DISPLAY_FORMAT),
    )
    column._restore_key(_attr_str(obj, ATTR_KEY))
    return column
