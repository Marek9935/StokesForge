"""Persistance du `PlotSpec` : schéma 2, migration depuis le schéma 1.

Le critère de sortie de la phase 6 : « un projet avec graphes sauvé puis
rouvert restitue les graphes **et leurs séries correctement reliées** aux
colonnes ». C'est ce que la persistance des clés de naissance (phase 4)
rendait possible ; c'est vérifié ici de bout en bout.
"""

from __future__ import annotations

import json

import h5py
import numpy as np
import pytest

from plotyx.core.column import ColumnRole
from plotyx.core.project import Project
from plotyx.persistence import load_project, save_project
from plotyx.persistence.errors import CorruptedProjectFileError
from plotyx.persistence.migration import migrate
from plotyx.persistence.schema import (
    ATTR_PLOT_SPEC_JSON,
    ATTR_SCHEMA_VERSION,
    CURRENT_SCHEMA_VERSION,
    GROUP_PROJECT,
)
from plotyx.plot import edit
from plotyx.plot.resolve import resolve_spec
from plotyx.plot.spec import AxisScale, AxisSide, PlotSpec, PlotType, SeriesSpec


@pytest.fixture
def project_with_graph():
    project = Project("P")
    book = project.add_workbook("Book1")
    sheet = book.sheet("Sheet1")
    x = sheet.add_column("T", np.linspace(0.0, 1.0, 10), role=ColumnRole.X, unit="s")
    y = sheet.add_column("V", np.linspace(0.0, 5.0, 10), unit="V")
    graph = project.add_graph("Graph1")
    spec = edit.add_series(
        PlotSpec.empty("Campagne"),
        SeriesSpec(y_key=y.key, x_key=x.key, plot_type=PlotType.LINE_SCATTER, name="Signal"),
    )
    spec = edit.add_panel(spec)
    spec = edit.add_series(
        spec, SeriesSpec(y_key=y.key, plot_type=PlotType.HISTOGRAM, axis=AxisSide.RIGHT), 1
    )
    spec = edit.update_axis(spec, 0, AxisSide.LEFT, scale=AxisScale.LOG, label="Tension", unit="V")
    graph.set_plot_spec(spec)
    return project, sheet, x, y, graph, spec


def test_the_schema_version_was_bumped_for_the_plot_spec():
    assert CURRENT_SCHEMA_VERSION == 2


def test_a_graph_round_trips_with_its_whole_spec(project_with_graph, tmp_path):
    project, _sheet, _x, _y, graph, spec = project_with_graph
    path = tmp_path / "p.plx"
    save_project(project, path)
    reloaded = load_project(path)
    other = reloaded.find_window("Graph1")
    assert other is not graph
    assert other.plot_spec == spec  # égalité stricte, ids compris


def test_the_series_are_still_wired_to_their_columns_after_a_round_trip(
    project_with_graph, tmp_path
):
    """Le critère de sortie : les clés de naissance survivent au fichier."""
    project, _sheet, x, y, _graph, _spec = project_with_graph
    path = tmp_path / "p.plx"
    save_project(project, path)
    reloaded = load_project(path)
    graph = reloaded.find_window("Graph1")

    resolved = resolve_spec(reloaded, graph.plot_spec)
    first = resolved.panels[0].series[0]
    assert first.ok and not first.broken
    assert list(first.y) == list(y.values)
    assert list(first.x) == list(x.values)
    # Et les clés sont bien celles d'origine, pas des UUID régénérés.
    assert graph.plot_spec.panels[0].series[0].y_key == y.key
    assert reloaded.registry.get(x.key) is reloaded.resolve_path("[Book1]Sheet1!T")


def test_a_renamed_column_still_resolves_after_a_round_trip(project_with_graph, tmp_path):
    """La clé n'est fonction ni du nom, ni du chemin (invariant phase 1)."""
    project, sheet, _x, y, _graph, _spec = project_with_graph
    sheet.rename_column(y, "Tension")
    path = tmp_path / "p.plx"
    save_project(project, path)
    reloaded = load_project(path)
    graph = reloaded.find_window("Graph1")
    assert resolve_spec(reloaded, graph.plot_spec).panels[0].series[0].ok


def test_a_graph_never_plotted_writes_no_spec_attribute(tmp_path):
    """« Jamais tracé » ≠ « explicitement vide » : rien n'est écrit."""
    project = Project("P")
    project.add_graph("Graph1")
    path = tmp_path / "p.plx"
    save_project(project, path)
    with h5py.File(path, "r") as file:
        assert ATTR_PLOT_SPEC_JSON not in file[f"{GROUP_PROJECT}/Graph1"].attrs
    assert load_project(path).find_window("Graph1").plot_spec is None


def test_the_spec_is_stored_as_readable_json(project_with_graph, tmp_path):
    project, _sheet, _x, y, _graph, _spec = project_with_graph
    path = tmp_path / "p.plx"
    save_project(project, path)
    with h5py.File(path, "r") as file:
        raw = file[f"{GROUP_PROJECT}/Graph1"].attrs[ATTR_PLOT_SPEC_JSON]
    payload = json.loads(raw)
    assert payload["title"] == "Campagne"
    assert payload["panels"][0]["series"][0]["y_key"] == y.key
    assert payload["panels"][0]["left"]["scale"] == "log"


def test_a_corrupted_spec_attribute_raises_cleanly(project_with_graph, tmp_path):
    project, _sheet, _x, _y, _graph, _spec = project_with_graph
    path = tmp_path / "p.plx"
    save_project(project, path)
    with h5py.File(path, "r+") as file:
        file[f"{GROUP_PROJECT}/Graph1"].attrs[ATTR_PLOT_SPEC_JSON] = "{ pas du json"
    with pytest.raises(CorruptedProjectFileError, match="PlotSpec illisible"):
        load_project(path)


# -- migration 1 → 2 ------------------------------------------------------------------


def test_a_schema_1_file_still_opens_and_its_graphs_are_simply_empty(tmp_path):
    """Migration additive : rien à réécrire, et surtout aucun spec inventé."""
    project = Project("P")
    project.add_graph("Graph1")
    path = tmp_path / "vieux.plx"
    save_project(project, path)
    with h5py.File(path, "r+") as file:
        file.attrs[ATTR_SCHEMA_VERSION] = 1  # on rétrograde le fichier
    reloaded = load_project(path)
    assert reloaded.find_window("Graph1").plot_spec is None


def test_migrate_from_1_leaves_the_file_untouched(tmp_path):
    project = Project("P")
    project.add_graph("Graph1")
    path = tmp_path / "p.plx"
    save_project(project, path)
    before = path.read_bytes()
    with h5py.File(path, "r") as file:
        migrate(file, 1)
    assert path.read_bytes() == before


def test_migrate_refuses_an_unknown_older_schema(tmp_path):
    project = Project("P")
    path = tmp_path / "p.plx"
    save_project(project, path)
    with h5py.File(path, "r") as file, pytest.raises(CorruptedProjectFileError):
        migrate(file, 0)


# -- autosave -------------------------------------------------------------------------


def test_an_autosave_also_carries_the_spec(project_with_graph, tmp_path, monkeypatch):
    from plotyx.persistence.autosave import recover, write_autosave

    monkeypatch.setenv("PLOTYX_AUTOSAVE_DIR", str(tmp_path))
    project, _sheet, _x, _y, _graph, spec = project_with_graph
    write_autosave(project, "session-test", None)
    recovered = recover(tmp_path / "session-test.autosave.plx")
    assert recovered.find_window("Graph1").plot_spec == spec
