"""Écriture d'un `Project` vers un fichier `.plx` (HDF5).

Voir `plotyx.persistence.schema` pour l'arborescence complète. Deux points
notables ici :

* **écriture atomique** — on écrit dans un fichier temporaire à côté de la
  cible puis on le renomme dessus (`Path.replace`, atomique sur un même
  système de fichiers) : un plantage ou une coupure pendant l'écriture ne
  laisse jamais le fichier de destination à moitié écrit ;
* **compression mesurée, pas devinée** — `gzip` niveau 4 avec l'option
  `shuffle` (voir le rapport de la phase 4) : sur un signal spectroscopique
  synthétique (baseline lente + pics + bruit), gain d'environ 40% sur la
  taille de fichier pour un surcoût d'écriture de quelques dizaines de
  millisecondes par colonne de 200k points. `lzf` compresse moins bien sur
  ce type de signal et n'a pas été retenu par défaut, mais reste
  utilisable via les paramètres de `save_project`.
"""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

import h5py
import numpy as np

from plotyx.core.column import Column, ColumnType
from plotyx.core.node import ModelObject
from plotyx.core.nodes import Folder, Graph, Matrix, Note, WindowNode, Workbook
from plotyx.core.project import Project
from plotyx.core.worksheet import Worksheet
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
    ATTR_PLOT_SPEC_JSON,
    ATTR_ROLE,
    ATTR_SCHEMA_VERSION,
    ATTR_TEXT,
    ATTR_TYPE,
    ATTR_UNIT,
    CURRENT_SCHEMA_VERSION,
    DATASET_MISSING,
    DATASET_VALUES,
    FORMAT_MARKER,
    GROUP_PROJECT,
    TYPE_COLUMN,
    TYPE_FOLDER,
    TYPE_GRAPH,
    TYPE_MATRIX,
    TYPE_NOTE,
    TYPE_WORKBOOK,
    TYPE_WORKSHEET,
)

__all__ = ["save_project"]

#: Choix par défaut, justifié dans le docstring du module et le rapport de
#: la phase 4 : mesuré, pas deviné.
DEFAULT_COMPRESSION = "gzip"
DEFAULT_COMPRESSION_OPTS = 4

_STRING_DTYPE = h5py.string_dtype(encoding="utf-8")


def save_project(
    project: Project,
    path: str | Path,
    *,
    compression: str | None = DEFAULT_COMPRESSION,
    compression_opts: int | None = DEFAULT_COMPRESSION_OPTS,
    extra_attrs: dict[str, str] | None = None,
) -> Path:
    """Écrit `project` dans `path`, en remplaçant tout fichier existant.

    `compression=None` désactive la compression (utile pour comparer, ou
    pour un disque déjà lent où le CPU est le goulot). `extra_attrs` est le
    point d'entrée de `plotyx.persistence.autosave` pour marquer un fichier
    comme sauvegarde automatique liée à un chemin source.
    """
    path = Path(path)
    tmp_path = path.with_name(path.name + ".tmp")
    try:
        with h5py.File(tmp_path, "w") as file:
            file.attrs[ATTR_FORMAT] = FORMAT_MARKER
            file.attrs[ATTR_SCHEMA_VERSION] = CURRENT_SCHEMA_VERSION
            for key, value in (extra_attrs or {}).items():
                file.attrs[key] = value
            root_group = file.create_group(GROUP_PROJECT)
            _write_node_common(root_group, project.root, TYPE_FOLDER)
            _write_folder_children(
                root_group, project.root, compression, compression_opts
            )
        tmp_path.replace(path)
    finally:
        tmp_path.unlink(missing_ok=True)
    return path


# -- nœuds de structure -------------------------------------------------------


def _write_node_common(group: h5py.Group, node: ModelObject, type_name: str) -> None:
    group.attrs[ATTR_KEY] = node.key
    group.attrs[ATTR_NAME] = node.name
    group.attrs[ATTR_TYPE] = type_name


def _write_window_common(group: h5py.Group, node: WindowNode) -> None:
    group.attrs[ATTR_LABEL] = node.label
    group.attrs[ATTR_COMMENT] = node.comment
    if node.metadata:
        group.attrs[ATTR_METADATA_JSON] = json.dumps(node.metadata)


def _write_order(group: h5py.Group, names: list[str]) -> None:
    group.attrs.create(ATTR_ORDER, data=np.array(names, dtype=object), dtype=_STRING_DTYPE)


def _write_folder_children(
    group: h5py.Group,
    folder: Folder,
    compression: str | None,
    compression_opts: int | None,
) -> None:
    names: list[str] = []
    for child in folder.children():
        names.append(child.name)
        subgroup = group.create_group(child.name)
        if isinstance(child, Folder):
            _write_node_common(subgroup, child, TYPE_FOLDER)
            _write_folder_children(subgroup, child, compression, compression_opts)
        elif isinstance(child, Workbook):
            _write_workbook(subgroup, child, compression, compression_opts)
        elif isinstance(child, Graph):
            _write_node_common(subgroup, child, TYPE_GRAPH)
            _write_window_common(subgroup, child)
            _write_plot_spec(subgroup, child)
        elif isinstance(child, Matrix):
            _write_node_common(subgroup, child, TYPE_MATRIX)
            _write_window_common(subgroup, child)
        elif isinstance(child, Note):
            _write_node_common(subgroup, child, TYPE_NOTE)
            _write_window_common(subgroup, child)
            subgroup.attrs[ATTR_TEXT] = child.text
        else:  # pragma: no cover - jeu de types de l'arbre fermé (phase 1)
            raise TypeError(f"nœud non pris en charge par la persistance : {child!r}")
    _write_order(group, names)


def _write_workbook(
    group: h5py.Group,
    workbook: Workbook,
    compression: str | None,
    compression_opts: int | None,
) -> None:
    _write_node_common(group, workbook, TYPE_WORKBOOK)
    _write_window_common(group, workbook)
    names: list[str] = []
    for sheet in workbook.sheets:
        names.append(sheet.name)
        subgroup = group.create_group(sheet.name)
        _write_worksheet(subgroup, sheet, compression, compression_opts)
    _write_order(group, names)


def _write_worksheet(
    group: h5py.Group,
    sheet: Worksheet,
    compression: str | None,
    compression_opts: int | None,
) -> None:
    _write_node_common(group, sheet, TYPE_WORKSHEET)
    names: list[str] = []
    for column in sheet.columns:
        names.append(column.name)
        _write_column(group, column, compression, compression_opts)
    _write_order(group, names)


def _write_plot_spec(group: h5py.Group, graph: Graph) -> None:
    """Sérialise le `PlotSpec` d'un graphe (schéma 2), s'il en a un.

    Un graphe vide (`plot_spec is None`) n'écrit rien du tout : l'absence
    d'attribut est la représentation d'un graphe sans contenu, à
    l'identique de ce qu'écrivait le schéma 1. C'est ce qui rend la
    migration 1 → 2 sans réécriture.
    """
    spec = graph.plot_spec
    if spec is None:
        return
    group.attrs[ATTR_PLOT_SPEC_JSON] = json.dumps(spec.to_dict(), ensure_ascii=False)


# -- colonnes -----------------------------------------------------------------


def _dataset_kwargs(
    length: int, compression: str | None, compression_opts: int | None
) -> dict[str, Any]:
    """`chunks`/`compression` n'ont de sens que sur un dataset non vide."""
    if length == 0 or compression is None:
        return {}
    kwargs: dict[str, Any] = {"chunks": True, "shuffle": True, "compression": compression}
    if compression == "gzip" and compression_opts is not None:
        kwargs["compression_opts"] = compression_opts
    return kwargs


def _write_column_attrs(target: h5py.Group | h5py.Dataset, column: Column) -> None:
    target.attrs[ATTR_KEY] = column.key
    target.attrs[ATTR_NAME] = column.name
    target.attrs[ATTR_TYPE] = TYPE_COLUMN
    target.attrs[ATTR_COLUMN_TYPE] = str(column.column_type)
    target.attrs[ATTR_ROLE] = str(column.role)
    target.attrs[ATTR_UNIT] = column.unit
    target.attrs[ATTR_LONG_NAME] = column.long_name
    target.attrs[ATTR_COMMENT] = column.comment
    target.attrs[ATTR_DISPLAY_FORMAT] = column.display_format


def _write_column(
    group: h5py.Group,
    column: Column,
    compression: str | None,
    compression_opts: int | None,
) -> None:
    length = len(column)
    if column.column_type is ColumnType.TEXT:
        subgroup = group.create_group(column.name)
        _write_column_attrs(subgroup, column)
        values = column.values
        raw = np.array(
            ["" if v is None else v for v in values], dtype=object
        )
        missing = np.array([v is None for v in values], dtype=bool)
        subgroup.create_dataset(
            DATASET_VALUES,
            data=raw,
            dtype=_STRING_DTYPE,
            **_dataset_kwargs(length, compression, compression_opts),
        )
        subgroup.create_dataset(
            DATASET_MISSING,
            data=missing,
            **_dataset_kwargs(length, compression, compression_opts),
        )
    else:
        dataset = group.create_dataset(
            column.name,
            data=np.asarray(column.values, dtype=np.float64),
            **_dataset_kwargs(length, compression, compression_opts),
        )
        _write_column_attrs(dataset, column)
