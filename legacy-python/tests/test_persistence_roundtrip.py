"""Round-trip complet du format `.plx` — critère de sortie de la phase 4.

Un projet non trivial (dossiers, classeurs, feuilles, colonnes de longueurs
différentes, métadonnées, NaN, types variés, nœuds `Graph`/`Matrix`/`Note`)
est sauvé, "fermé" (le `Project` Python est simplement abandonné), puis
rouvert. L'égalité est vérifiée avec `tests.model_state`, qui traite `NaN`
comme une valeur comme une autre (`np.testing.assert_array_equal`) et
compare par identité de clé de registre, pas par nom ni position.
"""

from __future__ import annotations

import numpy as np
import pytest

from plotyx.core import Column, ColumnRole, Project
from plotyx.persistence import load_project, save_project
from tests.model_state import assert_same_state, columns_of, snapshot


def _rich_project() -> Project:
    project = Project("RichDemo")

    book1 = project.add_workbook("Book1")
    sheet1 = book1.sheet("Sheet1")
    sheet1.add_column(
        "A",
        [1.0, 2.5, np.nan, 4.25, np.nan],
        role=ColumnRole.X,
        unit="s",
        long_name="Temps",
        comment="axe temporel",
        display_format="%.3f",
    )
    sheet1.add_column("B", [10.0, 20.0], role=ColumnRole.Y, unit="V")  # plus courte
    sheet1.insert_column(
        2, Column("C", ["alpha", None, "", "gamma"], column_type="text", role=ColumnRole.LABEL)
    )
    sheet1.add_column("D", [])  # colonne vide

    sheet2 = book1.add_sheet("Sheet2")
    sheet2.add_column("E", np.linspace(0, 1, 37))

    subfolder = project.add_folder("Instruments")
    book2 = project.add_workbook("Book2", subfolder)
    book2.sheet("Sheet1").add_column("X", [-1.0, np.inf, -np.inf, 0.0])
    project.add_graph("Graph1", subfolder)
    project.add_matrix("Mat1", subfolder)
    project.add_note("Note1", subfolder, text="quelques observations\nsur deux lignes")
    project.add_note("Note2", text="")

    return project


@pytest.fixture
def rich_project() -> Project:
    return _rich_project()


def test_round_trip_preserves_state_exactly(tmp_path, rich_project):
    path = tmp_path / "project.plx"
    save_project(rich_project, path)
    reloaded = load_project(path)

    assert_same_state(snapshot(rich_project), snapshot(reloaded), context="round-trip")


def test_round_trip_preserves_registry_keys(tmp_path, rich_project):
    """Décision de la phase 4 : les UUID de naissance sont persistés, pas
    régénérés — voir le docstring de `plotyx.persistence.reader`."""
    path = tmp_path / "project.plx"
    save_project(rich_project, path)
    reloaded = load_project(path)

    assert reloaded.root.key == rich_project.root.key
    original_keys = {c.name: c.key for c in columns_of(rich_project)}
    reloaded_keys = {c.name: c.key for c in columns_of(reloaded)}
    assert original_keys == reloaded_keys


def test_round_trip_preserves_variable_column_lengths(tmp_path, rich_project):
    path = tmp_path / "project.plx"
    save_project(rich_project, path)
    reloaded = load_project(path)

    sheet = reloaded.resolve_path("[Book1]Sheet1")
    assert len(sheet.column("A")) == 5
    assert len(sheet.column("B")) == 2
    assert len(sheet.column("D")) == 0
    assert sheet.row_count == 5  # le max des longueurs, pas une table remplie


def test_round_trip_preserves_text_none_vs_empty_string(tmp_path, rich_project):
    path = tmp_path / "project.plx"
    save_project(rich_project, path)
    reloaded = load_project(path)

    column = reloaded.resolve_path("[Book1]Sheet1!C")
    assert column.get(0) == "alpha"
    assert column.get(1) is None
    assert column.get(2) == ""  # chaîne vide, distincte de None
    assert column.get(3) == "gamma"


def test_round_trip_preserves_window_nodes_and_note_text(tmp_path, rich_project):
    path = tmp_path / "project.plx"
    save_project(rich_project, path)
    reloaded = load_project(path)

    graph = reloaded.find_window("Graph1")
    matrix = reloaded.find_window("Mat1")
    note = reloaded.find_window("Note1")
    assert graph is not None and type(graph).__name__ == "Graph"
    assert matrix is not None and type(matrix).__name__ == "Matrix"
    assert note is not None
    assert note.text == "quelques observations\nsur deux lignes"


def test_round_trip_empty_project(tmp_path):
    project = Project("Empty")
    path = tmp_path / "empty.plx"
    save_project(project, path)
    reloaded = load_project(path)
    assert_same_state(snapshot(project), snapshot(reloaded))


def test_save_project_is_atomic_no_leftover_tmp_file(tmp_path, rich_project):
    path = tmp_path / "project.plx"
    save_project(rich_project, path)
    leftovers = list(tmp_path.glob("*.tmp"))
    assert leftovers == []
    assert path.exists()


def test_save_project_returns_the_path(tmp_path, rich_project):
    path = tmp_path / "project.plx"
    result = save_project(rich_project, path)
    assert result == path


def test_open_replaces_a_previously_saved_file(tmp_path, rich_project):
    path = tmp_path / "project.plx"
    save_project(rich_project, path)
    other = Project("Other")
    other.add_workbook("Solo")
    save_project(other, path)
    reloaded = load_project(path)
    assert reloaded.name == "Other"
    assert reloaded.find_window("Book1") is None
