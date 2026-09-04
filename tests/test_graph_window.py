"""`GraphWindow` : mise à jour automatique et traitement des séries orphelines.

Le cœur de ce que la roadmap demande à la phase 6 : « modèle de série =
référence, jamais une copie → mise à jour auto quand la donnée change ».
Rien ici n'appelle explicitement le renderer après une saisie — c'est bien
le point.
"""

from __future__ import annotations

import numpy as np
import pytest

from plotyx.commands import (
    AddColumnCommand,
    CommandBus,
    DeleteColumnCommand,
    SetCellsCommand,
)
from plotyx.commands.plot import add_series_command, update_axis_command, update_series_command
from plotyx.core.column import Column, ColumnRole
from plotyx.core.project import Project
from plotyx.plot.spec import AxisScale, AxisSide, PlotSpec, PlotType, SeriesSpec
from plotyx.ui.graph_window import GraphWindow


@pytest.fixture
def graphed(qtbot):
    project = Project("P")
    book = project.add_workbook("Book1")
    sheet = book.sheet("Sheet1")
    sheet.add_column("T", np.linspace(0.0, 1.0, 20), role=ColumnRole.X, unit="s")
    sheet.add_column("V", np.linspace(0.0, 5.0, 20), unit="V")
    graph = project.add_graph("Graph1")
    bus = CommandBus(project)
    bus.push(
        add_series_command(
            graph, SeriesSpec(y_key=sheet.column("V").key, x_key=sheet.column("T").key)
        )
    )
    window = GraphWindow(graph, project, bus)
    qtbot.addWidget(window)
    yield project, sheet, graph, bus, window
    window.disconnect_signals()


def test_the_window_renders_as_soon_as_it_is_built(graphed):
    _project, _sheet, _graph, _bus, window = graphed
    assert len(window.renderer.curves) == 1
    assert window.banner_text == ""
    assert window.spec().series_count() == 1


def test_a_graph_without_a_spec_still_opens(qtbot):
    """`Graph.plot_spec is None` : la fenêtre affiche un panneau vide, pas une erreur."""
    project = Project("P")
    graph = project.add_graph("Graph1")
    window = GraphWindow(graph, project, CommandBus(project))
    qtbot.addWidget(window)
    assert isinstance(window.spec(), PlotSpec)
    assert window.renderer.curves == {}
    window.disconnect_signals()


# -- mise à jour automatique ------------------------------------------------------


def test_editing_a_cell_updates_the_curve_without_any_explicit_call(graphed):
    _project, sheet, _graph, bus, window = graphed
    bus.push(SetCellsCommand(sheet.column("V"), 0, [99.0]))
    window.refresh_now()
    curve = next(iter(window.renderer.curves.values()))
    assert curve.getData()[1][0] == 99.0


def test_undo_of_a_cell_edit_propagates_too(graphed):
    _project, sheet, _graph, bus, window = graphed
    bus.push(SetCellsCommand(sheet.column("V"), 0, [99.0]))
    bus.undo()
    window.refresh_now()
    assert next(iter(window.renderer.curves.values())).getData()[1][0] == 0.0


def test_a_change_of_spec_triggers_a_full_render(graphed):
    _project, _sheet, graph, bus, window = graphed
    series_id = graph.plot_spec.panels[0].series[0].id
    bus.push(update_series_command(graph, series_id, plot_type=PlotType.SCATTER))
    window.refresh_now()
    assert next(iter(window.renderer.curves.values())).opts["symbol"] == "o"
    bus.push(update_axis_command(graph, 0, AxisSide.LEFT, scale=AxisScale.LOG))
    window.refresh_now()
    assert window.renderer.plot_items[0].getAxis("left").logMode


def test_notifications_are_coalesced_into_a_single_render(graphed, qtbot):
    """Une macro de N commandes ne doit pas déclencher N rendus."""
    _project, sheet, _graph, bus, window = graphed
    renders: list[int] = []
    original = window.renderer.render

    def counting(resolved, original=original):
        renders.append(1)
        original(resolved)

    window.renderer.render = counting  # type: ignore[method-assign]
    with bus.macro("Beaucoup de colonnes"):
        for index in range(10):
            bus.push(AddColumnCommand(sheet, Column(f"C{index}", [1.0, 2.0])))
    assert renders == []  # rien tant que la boucle d'événements n'a pas repris
    qtbot.waitUntil(lambda: bool(renders), timeout=1000)
    assert len(renders) == 1


def test_adding_a_row_beyond_the_column_length_refreshes(graphed):
    _project, sheet, _graph, bus, window = graphed
    bus.push(SetCellsCommand(sheet.column("V"), 25, [7.0]))
    window.refresh_now()
    # x et y n'ont plus la même longueur : le préfixe commun est tracé.
    assert next(iter(window.renderer.curves.values())).getData()[1].size == 20


# -- références rompues ------------------------------------------------------------


def test_a_deleted_column_shows_a_banner_and_draws_nothing(graphed):
    _project, sheet, _graph, bus, window = graphed
    bus.push(DeleteColumnCommand(sheet, "V"))
    window.refresh_now()
    assert window.renderer.curves == {}
    assert "sans données" in window.banner_text
    assert "Ctrl+Z" in window.banner_text


def test_undoing_the_deletion_repairs_the_graph(graphed):
    _project, sheet, _graph, bus, window = graphed
    bus.push(DeleteColumnCommand(sheet, "V"))
    window.refresh_now()
    bus.undo()
    window.refresh_now()
    assert window.banner_text == ""
    assert len(window.renderer.curves) == 1


def test_a_named_broken_series_is_named_in_the_banner(qtbot):
    project = Project("P")
    graph = project.add_graph("Graph1")
    bus = CommandBus(project)
    bus.push(add_series_command(graph, SeriesSpec(y_key="clé-morte", name="Spectre A")))
    window = GraphWindow(graph, project, bus)
    qtbot.addWidget(window)
    assert "Spectre A" in window.banner_text
    window.disconnect_signals()


# -- interactions --------------------------------------------------------------------


def test_the_crosshair_checkbox_drives_the_renderer(graphed):
    _project, _sheet, _graph, _bus, window = graphed
    window._crosshair_box.setChecked(True)
    assert window.renderer.crosshair_enabled
    window._crosshair_box.setChecked(False)
    assert not window.renderer.crosshair_enabled


def test_mouse_movement_updates_the_coordinate_readout(graphed):
    _project, _sheet, _graph, _bus, window = graphed
    window.resize(400, 300)
    window._crosshair_box.setChecked(True)
    centre = window.renderer.plot_items[0].sceneBoundingRect().center()
    window._on_mouse_moved(centre)
    assert "x =" in window._coords.text()
    # Hors de tout panneau : la lecture ne change pas et rien ne lève.
    before = window._coords.text()
    far = window.renderer.plot_items[0].sceneBoundingRect().bottomRight()
    far.setX(far.x() + 10_000)
    window._on_mouse_moved(far)
    assert window._coords.text() == before


def test_auto_range_recenters_every_panel(graphed):
    _project, _sheet, _graph, _bus, window = graphed
    plot = window.renderer.plot_items[0]
    plot.setXRange(100, 200, padding=0)
    window.auto_range()
    low, high = plot.getViewBox().viewRange()[0]
    assert low < 1.0 < high or low <= 0.0


# -- cycle de vie ---------------------------------------------------------------------


def test_disconnecting_stops_every_notification(graphed):
    _project, sheet, _graph, bus, window = graphed
    window.disconnect_signals()
    calls: list[int] = []
    window.renderer.render = lambda resolved: calls.append(1)  # type: ignore[method-assign]
    bus.push(SetCellsCommand(sheet.column("V"), 0, [1.0]))
    assert calls == []


def test_closing_the_widget_disconnects_by_itself(graphed):
    _project, sheet, _graph, bus, window = graphed
    window.close()
    calls: list[int] = []
    window.renderer.render = lambda resolved: calls.append(1)  # type: ignore[method-assign]
    bus.push(SetCellsCommand(sheet.column("V"), 0, [1.0]))
    assert calls == []
