"""Panneau de propriétés : chaque contrôle pousse une commande, jamais plus.

Le test central est `test_no_widget_writes_outside_a_command` : le garde-fou
armé, on manipule tous les contrôles du panneau. La moindre écriture directe
dans le modèle ferait tomber le test.
"""

from __future__ import annotations

import numpy as np
import pytest
import shiboken6
from PySide6.QtCore import Qt

from plotyx.commands import CommandBus, DeleteColumnCommand
from plotyx.commands.guard import mutation_guard
from plotyx.commands.plot import add_series_command
from plotyx.core.column import ColumnRole
from plotyx.core.project import Project
from plotyx.plot.spec import AxisScale, AxisSide, LineStyle, PlotType, SeriesSpec, SymbolShape
from plotyx.ui.plot_properties import PlotPropertiesPanel


@pytest.fixture
def panel(qtbot):
    project = Project("P")
    book = project.add_workbook("Book1")
    sheet = book.sheet("Sheet1")
    sheet.add_column("T", np.linspace(0.0, 1.0, 10), role=ColumnRole.X, unit="s")
    sheet.add_column("V", np.linspace(0.0, 5.0, 10), unit="V")
    graph = project.add_graph("Graph1")
    bus = CommandBus(project)
    bus.push(
        add_series_command(
            graph, SeriesSpec(y_key=sheet.column("V").key, x_key=sheet.column("T").key)
        )
    )
    widget = PlotPropertiesPanel(graph, project, bus)
    qtbot.addWidget(widget)
    yield project, sheet, graph, bus, widget
    widget.disconnect_signals()


def _child(widget, name):
    from PySide6.QtWidgets import QWidget

    found = widget.findChild(QWidget, name)
    assert found is not None, f"widget {name!r} introuvable dans le panneau"
    return found


def test_the_panel_lists_the_series_of_the_current_panel(panel):
    _project, _sheet, _graph, _bus, widget = panel
    listing = _child(widget, "plotSeriesList")
    assert listing.count() == 1
    assert widget.selected_series is not None
    assert widget.panel_index == 0


def test_every_widget_pushes_an_undoable_command(panel):
    _project, _sheet, graph, bus, widget = panel
    before = bus.count

    _child(widget, "seriesTypeCombo").setCurrentIndex(1)
    assert bus.count == before + 1
    assert graph.plot_spec.panels[0].series[0].plot_type is not PlotType.LINE

    _child(widget, "seriesLineStyleCombo").setCurrentIndex(1)
    _child(widget, "seriesSymbolCombo").setCurrentIndex(2)
    _child(widget, "seriesVisibleBox").setChecked(False)
    _child(widget, "seriesAxisCombo").setCurrentIndex(1)

    spec = graph.plot_spec.panels[0].series[0]
    assert spec.line.style is not LineStyle.SOLID
    assert spec.symbol.shape is not SymbolShape.NONE
    assert spec.visible is False
    assert spec.axis is AxisSide.RIGHT

    while bus.can_undo:
        bus.undo()
    assert graph.plot_spec is None


def test_numeric_editors_push_on_editing_finished(panel):
    _project, _sheet, graph, _bus, widget = panel
    width = _child(widget, "seriesLineWidthSpin")
    width.setValue(5.0)
    width.editingFinished.emit()
    assert graph.plot_spec.panels[0].series[0].line.width == 5.0

    size = _child(widget, "seriesSymbolSizeSpin")
    size.setValue(12.0)
    size.editingFinished.emit()
    assert graph.plot_spec.panels[0].series[0].symbol.size == 12.0

    bins = _child(widget, "seriesBinsSpin")
    bins.setValue(17)
    bins.editingFinished.emit()
    assert graph.plot_spec.panels[0].series[0].histogram_bins == 17


def test_series_name_and_removal(panel):
    _project, _sheet, graph, bus, widget = panel
    name = _child(widget, "seriesNameEdit")
    name.setText("Essai 1")
    name.editingFinished.emit()
    assert graph.plot_spec.panels[0].series[0].name == "Essai 1"

    _child(widget, "seriesRemoveButton").click()
    assert graph.plot_spec.series_count() == 0
    bus.undo()
    assert graph.plot_spec.panels[0].series[0].name == "Essai 1"


def test_the_colour_helper_changes_line_and_symbol_together(panel):
    """Extraite du `QColorDialog` : testable sans dialogue modal."""
    _project, _sheet, graph, _bus, widget = panel
    series_id = graph.plot_spec.panels[0].series[0].id
    widget.set_series_color(series_id, "#00ff00")
    series = graph.plot_spec.panels[0].series[0]
    assert series.line.color == "#00ff00"
    assert series.symbol.color == "#00ff00"


def test_axis_controls_reach_the_spec(panel):
    _project, _sheet, graph, _bus, widget = panel
    label = _child(widget, "axis_bottom_labelEdit")
    label.setText("Temps")
    label.editingFinished.emit()
    unit = _child(widget, "axis_bottom_unitEdit")
    unit.setText("s")
    unit.editingFinished.emit()
    _child(widget, "axis_left_logBox").setChecked(True)
    _child(widget, "axis_left_gridBox").setChecked(True)
    _child(widget, "axis_left_autoBox").setChecked(False)

    panel_spec = graph.plot_spec.panels[0]
    assert panel_spec.bottom.display_label == "Temps (s)"
    assert panel_spec.left.scale is AxisScale.LOG
    assert panel_spec.left.grid
    assert not panel_spec.left.auto_range


def test_editing_an_axis_title_survives_its_own_qt_event(panel, qtbot):
    """Plantage réel reproduit : éditer le titre d'un axe au clavier.

    `label.editingFinished.emit()` (utilisé ci-dessus) appelle le slot en
    Python pur, hors de tout traitement Qt réel, et ne reproduit pas le
    plantage. `QTest.keyClick(..., Qt.Key.Key_Return)` fait émettre
    `editingFinished` **depuis `QLineEdit::keyPressEvent`, encore sur la
    pile d'appels C++** — exactement l'état dans lequel un `reload()`
    synchrone (avant correctif) détruisait ce widget pendant que Qt
    finissait de le traiter.
    """
    _project, _sheet, graph, _bus, widget = panel
    label = _child(widget, "axis_bottom_labelEdit")
    qtbot.keyClicks(label, "Temps")
    qtbot.keyClick(label, Qt.Key.Key_Return)
    assert shiboken6.isValid(label)  # pas détruit pendant son propre événement
    qtbot.waitUntil(lambda: graph.plot_spec.panels[0].bottom.label == "Temps", timeout=1000)


def test_axis_limits_are_pushed_and_used(panel):
    _project, _sheet, graph, _bus, widget = panel
    _child(widget, "axis_left_autoBox").setChecked(False)
    # Chaque poussée reconstruit le panneau : les widgets sont relus après
    # coup, jamais gardés d'une commande à l'autre.
    low = _child(widget, "axis_left_minSpin")
    low.setValue(1.0)
    low.editingFinished.emit()
    high = _child(widget, "axis_left_maxSpin")
    high.setValue(9.0)
    high.editingFinished.emit()
    axis = graph.plot_spec.panels[0].left
    assert axis.has_explicit_range
    assert (axis.range_min, axis.range_max) == (1.0, 9.0)


def test_graph_title_panel_title_and_legend(panel):
    _project, _sheet, graph, _bus, widget = panel
    title = _child(widget, "plotTitleEdit")
    title.setText("Campagne")
    title.editingFinished.emit()
    assert graph.plot_spec.title == "Campagne"

    panel_title = _child(widget, "panelTitleEdit")
    panel_title.setText("Signal")
    panel_title.editingFinished.emit()
    assert graph.plot_spec.panels[0].title == "Signal"

    _child(widget, "panelLegendBox").setChecked(False)
    assert not graph.plot_spec.panels[0].legend.visible


def test_editing_the_graph_title_survives_its_own_qt_event(panel, qtbot):
    """Même plantage que `test_editing_an_axis_title_survives_its_own_qt_event`,
    sur `_push_title` cette fois : `title.editingFinished` part depuis
    `QLineEdit::keyPressEvent` (Entrée), toujours sur la pile C++, pendant
    que `_push_title` pousse la commande qui va faire reconstruire (et donc
    détruire) ce même widget.
    """
    _project, _sheet, graph, _bus, widget = panel
    title = _child(widget, "plotTitleEdit")
    original = title.text()
    qtbot.keyClicks(title, " (campagne)")
    qtbot.keyClick(title, Qt.Key.Key_Return)
    assert shiboken6.isValid(title)  # pas détruit pendant son propre événement
    qtbot.waitUntil(
        lambda: graph.plot_spec.title == original + " (campagne)", timeout=1000
    )


def test_adding_a_panel_switches_the_selection_to_it(panel):
    _project, _sheet, graph, bus, widget = panel
    _child(widget, "plotAddPanelButton").click()
    assert len(graph.plot_spec.panels) == 2
    assert widget.panel_index == 1
    assert _child(widget, "plotSeriesList").count() == 0
    # On revient au premier panneau par la liste déroulante.
    _child(widget, "plotPanelCombo").setCurrentIndex(0)
    assert widget.panel_index == 0
    assert _child(widget, "plotSeriesList").count() == 1
    bus.undo()
    assert len(graph.plot_spec.panels) == 1


def test_selecting_another_series_reloads_the_form(panel, qtbot):
    _project, sheet, graph, bus, widget = panel
    bus.push(add_series_command(graph, SeriesSpec(y_key=sheet.column("T").key, name="Deux")))
    qtbot.waitUntil(lambda: _child(widget, "plotSeriesList").count() == 2, timeout=1000)
    listing = _child(widget, "plotSeriesList")
    listing.setCurrentRow(1)
    assert widget.selected_series == graph.plot_spec.panels[0].series[1].id
    assert _child(widget, "seriesNameEdit").text() == "Deux"


def test_a_no_op_edit_does_not_pollute_the_history(panel):
    """`editingFinished` part aussi sur une perte de focus : pas d'entrée vide."""
    _project, _sheet, _graph, bus, widget = panel
    before = bus.count
    name = _child(widget, "seriesNameEdit")
    name.editingFinished.emit()
    _child(widget, "plotTitleEdit").editingFinished.emit()
    assert bus.count == before


def test_an_orphan_series_stays_listed_and_flagged(panel, qtbot):
    _project, sheet, _graph, bus, widget = panel
    bus.push(DeleteColumnCommand(sheet, "V"))
    qtbot.waitUntil(lambda: "⚠" in _child(widget, "plotSeriesList").item(0).text(), timeout=1000)
    listing = _child(widget, "plotSeriesList")
    assert listing.count() == 1  # listée, jamais retirée en douce
    assert "rompue" in listing.item(0).text()
    bus.undo()
    qtbot.waitUntil(
        lambda: "⚠" not in _child(widget, "plotSeriesList").item(0).text(), timeout=1000
    )


def test_an_invisible_series_is_shown_in_parentheses(panel, qtbot):
    _project, _sheet, graph, bus, widget = panel
    series_id = graph.plot_spec.panels[0].series[0].id
    from plotyx.commands.plot import update_series_command

    bus.push(update_series_command(graph, series_id, visible=False))
    # `_on_structure_changed` diffère la reconstruction d'un tick d'événements
    # (voir son docstring) : le rechargement n'est pas encore fait ici.
    qtbot.waitUntil(
        lambda: _child(widget, "plotSeriesList").item(0).text().startswith("("), timeout=1000
    )


def test_the_panel_follows_an_undo_without_being_told(panel, qtbot):
    """Le panneau n'a pas d'état propre : il se relit sur le signal du graphe."""
    _project, _sheet, graph, bus, widget = panel
    _child(widget, "seriesVisibleBox").setChecked(False)
    bus.undo()
    qtbot.waitUntil(lambda: _child(widget, "seriesVisibleBox").isChecked(), timeout=1000)
    assert graph.plot_spec.panels[0].series[0].visible


def test_a_graph_without_a_spec_shows_an_empty_series_list(qtbot):
    project = Project("P")
    graph = project.add_graph("Graph1")
    widget = PlotPropertiesPanel(graph, project, CommandBus(project))
    qtbot.addWidget(widget)
    listing = widget.findChild(type(_child(widget, "plotSeriesList")), "plotSeriesList")
    assert listing.count() == 0
    assert widget.selected_series is None
    widget.disconnect_signals()


def test_no_widget_writes_outside_a_command(panel):
    """Le garde-fou armé : tous les contrôles doivent rester sur le bus."""
    _project, _sheet, graph, bus, widget = panel
    with mutation_guard():
        _child(widget, "seriesTypeCombo").setCurrentIndex(2)
        _child(widget, "seriesVisibleBox").setChecked(False)
        _child(widget, "axis_left_logBox").setChecked(True)
        _child(widget, "panelLegendBox").setChecked(False)
        _child(widget, "plotAddPanelButton").click()
        widget.set_series_color(graph.plot_spec.panels[0].series[0].id, "#123456")
        bus.undo()
        bus.redo()
