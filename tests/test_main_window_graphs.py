"""Intégration phase 6 dans `MainWindow` : MDI, dock « Properties », menu.

Le dock « Properties » existait depuis la phase 0 comme stub ; il est ici
rempli par le panneau de propriétés du graphe actif, **sans qu'un second
dock soit créé** — c'est vérifié explicitement.
"""

from __future__ import annotations

import numpy as np
import pytest
from PySide6.QtWidgets import QDockWidget, QMessageBox

from plotyx.commands import SetCellsCommand
from plotyx.core.column import ColumnRole
from plotyx.core.nodes import Graph
from plotyx.core.project import Project
from plotyx.persistence import save_project
from plotyx.plot.spec import PlotType
from plotyx.ui.graph_window import GraphWindow
from plotyx.ui.main_window import MainWindow
from plotyx.ui.plot_actions import create_graph_from_columns, plottable_columns
from plotyx.ui.plot_properties import PlotPropertiesPanel


def _project_with_data() -> Project:
    project = Project("P")
    book = project.add_workbook("Book1")
    sheet = book.sheet("Sheet1")
    sheet.add_column("T", np.linspace(0.0, 1.0, 20), role=ColumnRole.X, unit="s")
    sheet.add_column("V", np.linspace(0.0, 5.0, 20), unit="V", long_name="Tension")
    sheet.add_column("W", np.linspace(5.0, 0.0, 20), unit="V")
    return project


@pytest.fixture
def window(qtbot, monkeypatch):
    # Même filet que `test_main_window_ui.py` : `closeEvent` ouvre un
    # `QMessageBox` modal quand le document est modifié, et la fermeture
    # automatique de `qtbot.addWidget` a lieu avant tout code de teardown
    # écrit ici. Tous les tests de ce fichier laissent le projet modifié
    # (créer un graphe *est* une modification) sans rien attendre du
    # disque : on répond « abandonner ».
    monkeypatch.setattr(
        QMessageBox, "warning", staticmethod(lambda *a, **k: QMessageBox.StandardButton.Discard)
    )
    win = MainWindow(_project_with_data(), autosave_interval_ms=0)
    qtbot.addWidget(win)
    yield win
    win.close()


# -- création d'un graphe depuis une sélection ---------------------------------------


def test_plottable_columns_skips_x_error_and_text_columns():
    project = _project_with_data()
    sheet = project.resolve_path("[Book1]Sheet1")
    from plotyx.core.column import Column

    sheet.insert_column(0, Column("Note", ["a"], column_type="text"))
    sheet.add_column("E", [0.1], role=ColumnRole.Y_ERROR)
    assert [c.name for c in plottable_columns(sheet.columns)] == ["V", "W"]


def test_creating_a_graph_is_a_single_history_entry(window):
    sheet = window.project.resolve_path("[Book1]Sheet1")
    before = window.bus.count
    graph = create_graph_from_columns(window.project, window.bus, list(sheet.columns))
    assert window.bus.count == before + 1
    assert graph.plot_spec.series_count() == 2  # T est l'abscisse, pas une série
    window.bus.undo()
    assert window.project.find_window(graph.name) is None


def test_the_x_column_of_the_sheet_becomes_the_abscissa(window):
    sheet = window.project.resolve_path("[Book1]Sheet1")
    graph = create_graph_from_columns(
        window.project, window.bus, [sheet.column("V"), sheet.column("W")]
    )
    x_key = sheet.column("T").key
    assert all(s.x_key == x_key for _, s in graph.plot_spec.iter_series())


def test_axes_are_labelled_from_the_columns(window):
    sheet = window.project.resolve_path("[Book1]Sheet1")
    graph = create_graph_from_columns(window.project, window.bus, [sheet.column("V")])
    panel = graph.plot_spec.panels[0]
    assert panel.bottom.display_label == "T (s)"
    assert panel.left.display_label == "Tension (V)"


def test_an_error_column_in_the_selection_becomes_error_bars(window):
    sheet = window.project.resolve_path("[Book1]Sheet1")
    errors = sheet.add_column("E", np.full(20, 0.2), role=ColumnRole.Y_ERROR)
    graph = create_graph_from_columns(
        window.project, window.bus, [sheet.column("V"), errors]
    )
    series = graph.plot_spec.panels[0].series[0]
    assert series.plot_type is PlotType.ERROR_BAR
    assert series.y_error_key == errors.key


def test_creating_a_graph_without_a_plottable_column_raises(window):
    with pytest.raises(ValueError, match="aucune colonne"):
        create_graph_from_columns(window.project, window.bus, [])


def test_a_series_falls_back_on_the_row_index_without_any_x_column(window):
    project = Project("Q")
    book = project.add_workbook("Book1")
    sheet = book.sheet("Sheet1")
    column = sheet.add_column("Y", [1.0, 2.0, 3.0])
    from plotyx.commands import CommandBus

    graph = create_graph_from_columns(project, CommandBus(project), [column])
    assert graph.plot_spec.panels[0].series[0].x_key is None


# -- sous-fenêtres MDI --------------------------------------------------------------


def test_a_graph_opens_as_an_mdi_sub_window(window):
    before = len(window.mdi_area.subWindowList())
    sheet = window.project.resolve_path("[Book1]Sheet1")
    window._create_graph(list(sheet.columns))
    assert len(window.mdi_area.subWindowList()) == before + 1
    sub = window.mdi_area.subWindowList()[-1]
    assert isinstance(sub.widget(), GraphWindow)


def test_opening_the_same_graph_twice_reuses_the_window(window):
    graph = window.project.add_graph("G")
    first = window.open_graph(graph)
    assert window.open_graph(graph) is first


def test_double_clicking_a_graph_in_the_explorer_opens_it(window):
    graph = window.project.add_graph("G")
    model = window._project_tree_model
    index = model.index_of(graph) if hasattr(model, "index_of") else None
    if index is None:
        window._on_explorer_double_clicked(_index_for(model, graph))
    else:  # pragma: no cover - dépend de l'API du modèle d'arbre
        window._on_explorer_double_clicked(index)
    assert graph.key in window._open_graphs


def _index_for(model, node):
    """Retrouve l'index de l'arbre qui porte `node`."""
    for row in range(model.rowCount()):
        index = model.index(row, 0)
        if model.node_at(index) is node:
            return index
    raise AssertionError(f"{node!r} absent de l'arbre")


def test_closing_the_sub_window_unregisters_the_graph(window, qtbot):
    graph = window.project.add_graph("G")
    sub = window.open_graph(graph)
    sub.close()
    qtbot.waitUntil(lambda: graph.key not in window._open_graphs, timeout=1000)


def test_the_plot_action_uses_the_active_worksheet_selection(window):
    """Sans sélection, on trace toute la feuille active."""
    before = len(window.project.windows())
    window._on_plot_action()
    assert len(window.project.windows()) == before + 1


def test_the_plot_action_warns_when_there_is_nothing_to_plot(window, monkeypatch):
    warnings: list[str] = []
    monkeypatch.setattr(
        QMessageBox,
        "warning",
        staticmethod(lambda *args, **kwargs: warnings.append(args[2])),
    )
    graph = window.project.add_graph("G")
    window.mdi_area.setActiveSubWindow(window.open_graph(graph))
    window._on_plot_action()  # la fenêtre active est un graphe, pas une feuille
    assert warnings and "aucune colonne" in warnings[0]


# -- dock « Properties » ---------------------------------------------------------------


def test_there_is_still_exactly_one_properties_dock(window):
    docks = [
        dock
        for dock in window.findChildren(QDockWidget)
        if dock.objectName() == "propertiesDock"
    ]
    assert len(docks) == 1


def test_activating_a_graph_fills_the_properties_dock(window):
    sheet = window.project.resolve_path("[Book1]Sheet1")
    graph = window._create_graph(list(sheet.columns))
    window.mdi_area.setActiveSubWindow(window._open_graphs[graph.key])
    panel = window.properties_panel
    assert isinstance(panel, PlotPropertiesPanel)
    assert panel.graph is graph
    assert window._properties_dock.widget() is panel


def test_activating_a_worksheet_empties_the_properties_dock(window):
    sheet = window.project.resolve_path("[Book1]Sheet1")
    graph = window._create_graph(list(sheet.columns))
    window.mdi_area.setActiveSubWindow(window._open_graphs[graph.key])
    assert window.properties_panel is not None
    window.mdi_area.setActiveSubWindow(window._open_sheets[sheet.key])
    assert window.properties_panel is None
    assert not isinstance(window._properties_dock.widget(), PlotPropertiesPanel)


def test_reactivating_the_same_graph_keeps_the_same_panel(window):
    graph = window.project.add_graph("G")
    window.mdi_area.setActiveSubWindow(window.open_graph(graph))
    panel = window.properties_panel
    window._show_properties_for(graph)
    assert window.properties_panel is panel


def test_closing_the_graph_window_clears_the_properties_dock(window, qtbot):
    graph = window.project.add_graph("G")
    sub = window.open_graph(graph)
    window.mdi_area.setActiveSubWindow(sub)
    assert window.properties_panel is not None
    sub.close()
    qtbot.waitUntil(lambda: window.properties_panel is None, timeout=1000)


# -- interaction avec le reste de l'application -------------------------------------------


def test_editing_a_cell_updates_the_open_graph(window):
    sheet = window.project.resolve_path("[Book1]Sheet1")
    graph = window._create_graph([sheet.column("V")])
    view: GraphWindow = window._open_graphs[graph.key].widget()
    window.bus.push(SetCellsCommand(sheet.column("V"), 0, [42.0]))
    view.refresh_now()
    series_id = next(iter(view.renderer.curves))
    assert view.renderer.series_xy(series_id)[1][0] == 42.0


def test_opening_another_project_closes_the_graph_windows(window, tmp_path):
    sheet = window.project.resolve_path("[Book1]Sheet1")
    graph = window._create_graph(list(sheet.columns))
    assert graph.key in window._open_graphs

    path = tmp_path / "autre.plx"
    save_project(_project_with_data(), path)
    assert window.open_path(path) is True
    assert window._open_graphs == {}
    assert window.properties_panel is None


def test_a_reopened_project_can_show_its_graphs_again(window, tmp_path):
    sheet = window.project.resolve_path("[Book1]Sheet1")
    window._create_graph(list(sheet.columns))
    path = tmp_path / "p.plx"
    save_project(window.project, path)

    assert window.open_path(path) is True
    reloaded = window.project.find_window("Graph1")
    assert isinstance(reloaded, Graph)
    view: GraphWindow = window.open_graph(reloaded).widget()
    assert len(view.renderer.curves) == 2
    assert view.banner_text == ""
