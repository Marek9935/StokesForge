"""Complète la couverture des chemins peu fréquents de lecture/écriture :
métadonnées non vides sur un nœud fenêtre, attribut binaire, type de nœud
inconnu dans le fichier."""

from __future__ import annotations

import h5py
import numpy as np
import pytest

from plotyx.core import Project
from plotyx.persistence import CorruptedProjectFileError, load_project, save_project
from plotyx.persistence.reader import _attr_str
from plotyx.persistence.schema import ATTR_ORDER, ATTR_TYPE


def test_window_node_metadata_round_trips(tmp_path):
    project = Project("P")
    graph = project.add_graph("Graph1")
    graph.metadata["source"] = "spectromètre A"
    graph.metadata["gain"] = 12
    graph.label = "Vue d'ensemble"
    graph.comment = "commentaire libre"

    path = tmp_path / "meta.plx"
    save_project(project, path)
    reloaded = load_project(path)

    reloaded_graph = reloaded.find_window("Graph1")
    assert reloaded_graph.label == "Vue d'ensemble"
    assert reloaded_graph.comment == "commentaire libre"
    assert reloaded_graph.metadata == {"source": "spectromètre A", "gain": 12}


def test_attr_str_decodes_bytes_attributes():
    class _Fake:
        attrs = {"x": b"caf\xc3\xa9"}

    assert _attr_str(_Fake(), "x") == "café"


def test_attr_str_falls_back_to_default_when_missing():
    class _Fake:
        attrs = {}

    assert _attr_str(_Fake(), "missing", "défaut") == "défaut"


def test_unknown_node_type_in_file_raises_cleanly(tmp_path):
    project = Project("P")
    project.add_folder("F")
    path = tmp_path / "bad_type.plx"
    save_project(project, path)

    with h5py.File(path, "r+") as file:
        file["project"]["F"].attrs[ATTR_TYPE] = "unicorn"

    with pytest.raises(CorruptedProjectFileError, match="unicorn"):
        load_project(path)


def test_children_order_round_trips_empty_list(tmp_path):
    project = Project("P")
    path = tmp_path / "empty_folder.plx"
    save_project(project, path)
    with h5py.File(path, "r") as file:
        order = file["project"].attrs[ATTR_ORDER]
        assert list(order) == []
    reloaded = load_project(path)
    assert reloaded.root.children() == ()


def test_datetime_column_round_trips(tmp_path):
    from plotyx.core import Column

    project = Project("P")
    sheet = project.add_workbook("Book1").sheet("Sheet1")
    sheet.insert_column(
        0, Column("T", [0.0, 86400.0, np.nan], column_type="datetime")
    )
    path = tmp_path / "datetime.plx"
    save_project(project, path)
    reloaded = load_project(path)
    column = reloaded.resolve_path("[Book1]Sheet1!T")
    assert str(column.column_type) == "datetime"
    np.testing.assert_array_equal(column.values, np.array([0.0, 86400.0, np.nan]))
