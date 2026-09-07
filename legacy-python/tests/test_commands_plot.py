"""Commandes de graphe : la règle d'or s'applique aussi au tracé.

Créer un graphe, ajouter une série, changer une couleur, passer un axe en
logarithmique : tout est annulable, et rien ne mute le modèle hors du bus.
"""

from __future__ import annotations

import numpy as np
import pytest

from plotyx.commands import AddObjectCommand, CommandBus, DeleteObjectCommand
from plotyx.commands.guard import MutationOutsideCommandError, mutation_guard
from plotyx.commands.plot import (
    SetPlotSpecCommand,
    add_panel_command,
    add_series_command,
    current_spec,
    move_series_command,
    remove_panel_command,
    remove_series_command,
    set_graph_title_command,
    update_axis_command,
    update_legend_command,
    update_panel_command,
    update_series_command,
)
from plotyx.core.errors import StructureError
from plotyx.core.nodes import Graph
from plotyx.core.project import Project
from plotyx.plot.spec import AxisScale, AxisSide, LineStyle, PlotSpec, PlotType, SeriesSpec


@pytest.fixture
def graph_project(qapp):
    project = Project("P")
    book = project.add_workbook("Book1")
    sheet = book.sheet("Sheet1")
    sheet.add_column("X", np.arange(5.0))
    sheet.add_column("Y", np.arange(5.0) ** 2)
    graph = project.add_graph("Graph1")
    return project, sheet, graph, CommandBus(project)


# -- le nœud `Graph` porte un spec ------------------------------------------------


def test_a_fresh_graph_has_no_spec_at_all():
    """`None`, pas un spec vide : « jamais tracé » ≠ « explicitement vide »."""
    assert Graph("G").plot_spec is None
    assert current_spec(Graph("G")).series_count() == 0


def test_current_spec_refuses_a_non_graph(graph_project):
    _project, sheet, _graph, _bus = graph_project
    with pytest.raises(StructureError):
        current_spec(sheet)


def test_setting_a_spec_is_a_primitive_caught_by_the_guard(graph_project):
    _project, _sheet, graph, _bus = graph_project
    with mutation_guard(), pytest.raises(MutationOutsideCommandError):
        graph.set_plot_spec(PlotSpec.empty())


# -- la commande de base ------------------------------------------------------------


def test_set_plot_spec_applies_and_undoes(graph_project):
    _project, _sheet, graph, bus = graph_project
    spec = PlotSpec.empty("Titre")
    bus.push(SetPlotSpecCommand(graph, spec))
    assert graph.plot_spec is spec
    bus.undo()
    assert graph.plot_spec is None
    bus.redo()
    assert graph.plot_spec is spec


def test_set_plot_spec_rejects_a_non_spec_and_a_non_graph(graph_project):
    _project, sheet, graph, _bus = graph_project
    with pytest.raises(TypeError):
        SetPlotSpecCommand(graph, "pas un spec")
    with pytest.raises(StructureError):
        SetPlotSpecCommand(sheet, PlotSpec.empty())


def test_the_command_carries_its_graph_and_spec(graph_project):
    _project, _sheet, graph, _bus = graph_project
    spec = PlotSpec.empty()
    command = SetPlotSpecCommand(graph, spec)
    assert command.graph is graph and command.spec is spec


# -- les fabriques ---------------------------------------------------------------------


def test_adding_a_series_is_undoable(graph_project):
    _project, sheet, graph, bus = graph_project
    series = SeriesSpec(y_key=sheet.column("Y").key, x_key=sheet.column("X").key)
    bus.push(add_series_command(graph, series))
    assert graph.plot_spec.series_count() == 1
    bus.undo()
    assert current_spec(graph).series_count() == 0


def test_the_whole_style_vocabulary_goes_through_the_bus(graph_project):
    _project, sheet, graph, bus = graph_project
    bus.push(add_series_command(graph, SeriesSpec(y_key=sheet.column("Y").key)))
    series_id = graph.plot_spec.panels[0].series[0].id

    bus.push(update_series_command(graph, series_id, plot_type=PlotType.SCATTER))
    bus.push(update_series_command(graph, series_id, line={"color": "#ff0000"}))
    bus.push(update_series_command(graph, series_id, line={"style": LineStyle.DOT}))
    bus.push(update_axis_command(graph, 0, AxisSide.LEFT, scale=AxisScale.LOG))
    bus.push(update_axis_command(graph, 0, AxisSide.BOTTOM, label="Temps", unit="s"))
    bus.push(update_legend_command(graph, 0, visible=False))
    bus.push(update_panel_command(graph, 0, title="Panneau"))
    bus.push(set_graph_title_command(graph, "Mon graphe"))

    spec = graph.plot_spec
    series = spec.panels[0].series[0]
    assert series.plot_type is PlotType.SCATTER
    assert series.line.color == "#ff0000"
    assert series.line.style is LineStyle.DOT
    assert spec.panels[0].left.scale is AxisScale.LOG
    assert spec.panels[0].bottom.display_label == "Temps (s)"
    assert not spec.panels[0].legend.visible
    assert spec.panels[0].title == "Panneau"
    assert spec.title == "Mon graphe"

    for _ in range(8):
        bus.undo()
    assert current_spec(graph).series_count() == 1  # il ne reste que l'ajout de série


def test_panels_are_added_removed_and_series_moved(graph_project):
    _project, sheet, graph, bus = graph_project
    bus.push(add_series_command(graph, SeriesSpec(y_key=sheet.column("Y").key)))
    bus.push(add_panel_command(graph))
    assert len(graph.plot_spec.panels) == 2

    series_id = graph.plot_spec.panels[0].series[0].id
    bus.push(move_series_command(graph, series_id, 1))
    assert graph.plot_spec.panels[1].series[0].id == series_id
    bus.undo()
    assert graph.plot_spec.panels[0].series[0].id == series_id

    bus.push(remove_panel_command(graph, 1))
    assert len(graph.plot_spec.panels) == 1
    bus.undo()
    assert len(graph.plot_spec.panels) == 2


def test_removing_a_series_is_undoable(graph_project):
    _project, sheet, graph, bus = graph_project
    bus.push(add_series_command(graph, SeriesSpec(y_key=sheet.column("Y").key)))
    series_id = graph.plot_spec.panels[0].series[0].id
    bus.push(remove_series_command(graph, series_id))
    assert graph.plot_spec.series_count() == 0
    bus.undo()
    assert graph.plot_spec.panels[0].series[0].id == series_id


def test_a_graph_with_its_series_is_one_history_entry(graph_project):
    """Un `Ctrl+Z` défait « nouveau graphe », pas « une série sur trois »."""
    project, sheet, _graph, bus = graph_project
    before = bus.count
    graph = Graph("Graph2")
    with bus.macro("Nouveau graphe"):
        bus.push(AddObjectCommand(project.root, graph))
        for name in ("X", "Y"):
            bus.push(add_series_command(graph, SeriesSpec(y_key=sheet.column(name).key)))
    assert bus.count == before + 1
    bus.undo()
    assert project.find_window("Graph2") is None


def test_everything_stays_inside_the_guard(graph_project):
    """Le garde-fou armé, toute la phase 6 doit passer sans lever."""
    _project, sheet, graph, bus = graph_project
    with mutation_guard():
        bus.push(add_series_command(graph, SeriesSpec(y_key=sheet.column("Y").key)))
        series_id = graph.plot_spec.panels[0].series[0].id
        bus.push(update_series_command(graph, series_id, visible=False))
        bus.push(update_axis_command(graph, 0, AxisSide.BOTTOM, grid=True))
        bus.undo()
        bus.redo()


# -- coût mémoire de l'historique ------------------------------------------------------


def test_the_undo_delta_holds_no_data(graph_project):
    """Le delta est l'ancien spec : des chaînes, jamais un `ndarray`."""
    _project, sheet, graph, bus = graph_project
    big = sheet.add_column("Gros", np.arange(1_000_000, dtype=np.float64))
    bus.push(add_series_command(graph, SeriesSpec(y_key=big.key)))
    command = bus.stack.command(bus.count - 1)
    assert isinstance(command, SetPlotSpecCommand)
    # Aucun objet retenu hors de l'arbre, et le spec ne porte qu'une clé.
    assert command.retained_objects() == ()
    assert command.spec.keys() == (big.key,)
    assert bus.retained_objects() == ()


def test_deleting_the_plotted_column_leaves_the_series_in_place(graph_project):
    """Symétrique du test de `resolve` : côté commandes, rien n'efface la série."""
    project, sheet, graph, bus = graph_project
    column = sheet.column("Y")
    bus.push(add_series_command(graph, SeriesSpec(y_key=column.key)))
    bus.push(DeleteObjectCommand(column))
    assert graph.plot_spec.series_count() == 1
    assert project.registry.get(column.key) is None
    bus.undo()
    assert project.registry.get(column.key) is column
