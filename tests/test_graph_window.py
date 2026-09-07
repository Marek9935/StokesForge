"""`GraphWindow` : mise à jour automatique, séries orphelines, moteurs de rendu.

Le cœur de ce que la roadmap demande à la phase 6 : « modèle de série =
référence, jamais une copie → mise à jour auto quand la donnée change ».
Rien ici n'appelle explicitement le renderer après une saisie — c'est bien
le point.

Depuis la réouverture de la phase 6 (2026-09-07), **tout ce fichier tourne
deux fois**, une par moteur : la fixture `graphed` est paramétrée sur
`RenderMode`. C'est la garantie que le contrat de la fenêtre — se mettre à
jour toute seule, signaler une série rompue, se déconnecter proprement — ne
dépend pas du moteur qui peint. Les assertions passent donc par
`renderer.series_xy`, le point d'introspection neutre, et non par l'API
d'un moteur ; les deux seuls endroits qui doivent regarder un moteur en
face le font par les aiguillages `_symbol_is_drawn` et `_left_axis_is_log`,
regroupés ici plutôt que dispersés dans les tests.
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
from plotyx.ui.renderers import RenderMode


@pytest.fixture(params=list(RenderMode), ids=lambda mode: mode.value)
def graphed(qtbot, request):
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
    window = GraphWindow(graph, project, bus, mode=request.param)
    qtbot.addWidget(window)
    yield project, sheet, graph, bus, window
    window.disconnect_signals()


def _ydata(window) -> np.ndarray:
    """Les ordonnées réellement tracées, quel que soit le moteur."""
    series_id = next(iter(window.renderer.curves))
    xy = window.renderer.series_xy(series_id)
    assert xy is not None
    return xy[1]


def _symbol_is_drawn(window) -> bool:
    curve = next(iter(window.renderer.curves.values()))
    if window.render_mode is RenderMode.DIRECT:
        return curve.opts["symbol"] == "o"
    return curve.get_marker() == "o"


def _left_axis_is_log(window) -> bool:
    if window.render_mode is RenderMode.DIRECT:
        return bool(window.renderer.plot_items[0].getAxis("left").logMode)
    return window.renderer.axes[0].get_yscale() == "log"


def test_the_window_renders_as_soon_as_it_is_built(graphed):
    _project, _sheet, _graph, _bus, window = graphed
    assert len(window.renderer.curves) == 1
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
    assert _ydata(window)[0] == 99.0


def test_undo_of_a_cell_edit_propagates_too(graphed):
    _project, sheet, _graph, bus, window = graphed
    bus.push(SetCellsCommand(sheet.column("V"), 0, [99.0]))
    bus.undo()
    window.refresh_now()
    assert _ydata(window)[0] == 0.0


def test_a_change_of_spec_triggers_a_full_render(graphed):
    _project, _sheet, graph, bus, window = graphed
    series_id = graph.plot_spec.panels[0].series[0].id
    bus.push(update_series_command(graph, series_id, plot_type=PlotType.SCATTER))
    window.refresh_now()
    assert _symbol_is_drawn(window)
    bus.push(update_axis_command(graph, 0, AxisSide.LEFT, scale=AxisScale.LOG))
    window.refresh_now()
    assert _left_axis_is_log(window)


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
    assert _ydata(window).size == 20


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
    assert "sans données" not in window.banner_text
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


# -- moteur de rendu (décision 2026-09-07) -------------------------------------------


def test_the_default_engine_is_matplotlib(graphed):
    """Le défaut ne se déduit pas d'un réglage : il est posé, on le vérifie."""
    _project, _sheet, graph, bus, _window = graphed
    fresh = GraphWindow(graph, _project, bus)
    assert fresh.render_mode is RenderMode.FIGURE
    assert fresh.renderer.supports_full_styling
    fresh.disconnect_signals()


def test_switching_engine_keeps_the_graph_and_pushes_nothing_on_the_bus(graphed):
    """Changer de vue n'est pas une modification du projet."""
    _project, _sheet, _graph, bus, window = graphed
    before, before_modified = bus.stack.count(), bus.is_modified
    target = (
        RenderMode.FIGURE if window.render_mode is RenderMode.DIRECT else RenderMode.DIRECT
    )
    window.set_render_mode(target)
    assert window.render_mode is target
    assert len(window.renderer.curves) == 1
    assert _ydata(window)[-1] == 5.0
    assert bus.stack.count() == before
    assert bus.is_modified is before_modified


def test_the_light_view_announces_what_it_does_not_honour(graphed):
    _project, _sheet, _graph, _bus, window = graphed
    window.set_render_mode(RenderMode.DIRECT)
    assert "Visualiseur léger" in window.banner_text
    window.set_render_mode(RenderMode.FIGURE)
    assert window.banner_text == ""


def test_the_banner_reports_the_mode_and_the_broken_series_together(graphed):
    """Être en vue directe ne dispense pas de signaler une référence cassée."""
    _project, sheet, _graph, bus, window = graphed
    window.set_render_mode(RenderMode.DIRECT)
    bus.push(DeleteColumnCommand(sheet, "V"))
    window.refresh_now()
    assert "Visualiseur léger" in window.banner_text
    assert "sans données" in window.banner_text


def test_the_crosshair_survives_a_change_of_engine(graphed):
    _project, _sheet, _graph, _bus, window = graphed
    window._crosshair_box.setChecked(True)
    target = (
        RenderMode.FIGURE if window.render_mode is RenderMode.DIRECT else RenderMode.DIRECT
    )
    window.set_render_mode(target)
    assert window.renderer.crosshair_enabled


def test_switching_to_the_same_mode_is_a_no_op(graphed):
    _project, _sheet, _graph, _bus, window = graphed
    renderer = window.renderer
    window.set_render_mode(window.render_mode)
    assert window.renderer is renderer


def test_the_data_still_flows_after_a_change_of_engine(graphed):
    """La bascule ne doit pas laisser la fenêtre abonnée à l'ancien renderer."""
    _project, sheet, _graph, bus, window = graphed
    target = (
        RenderMode.FIGURE if window.render_mode is RenderMode.DIRECT else RenderMode.DIRECT
    )
    window.set_render_mode(target)
    bus.push(SetCellsCommand(sheet.column("V"), 0, [42.0]))
    window.refresh_now()
    assert _ydata(window)[0] == 42.0


# -- interactions --------------------------------------------------------------------


def test_the_crosshair_checkbox_drives_the_renderer(graphed):
    _project, _sheet, _graph, _bus, window = graphed
    window._crosshair_box.setChecked(True)
    assert window.renderer.crosshair_enabled
    window._crosshair_box.setChecked(False)
    assert not window.renderer.crosshair_enabled


def test_the_coordinate_readout_follows_what_the_renderer_reports(graphed):
    """La fenêtre ne suit plus la souris elle-même : elle reçoit un rappel."""
    _project, _sheet, _graph, _bus, window = graphed
    window._on_coords((1.5, -2.25))
    assert "x = 1.5" in window._coords.text()
    assert "y = -2.25" in window._coords.text()
    # Hors de tout panneau : la lecture s'efface plutôt que de mentir.
    window._on_coords(None)
    assert window._coords.text() == ""


def test_auto_range_recenters_every_panel(graphed):
    _project, _sheet, _graph, _bus, window = graphed
    if window.render_mode is RenderMode.DIRECT:
        plot = window.renderer.plot_items[0]
        plot.setXRange(100, 200, padding=0)
        window.auto_range()
        low, high = plot.getViewBox().viewRange()[0]
    else:
        ax = window.renderer.axes[0]
        ax.set_xlim(100, 200)
        window.auto_range()
        low, high = ax.get_xlim()
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
