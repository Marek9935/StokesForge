"""Le renderer PyQtGraph : traduction fidèle du vocabulaire neutre.

Ce qui est vérifié ici, ce n'est pas « ça a l'air joli » — c'est que chaque
notion du `PlotSpec` arrive bien dans le bon réglage PyQtGraph, et que les
réglages de performance (`downsample`, `clipToView`) sont réellement armés.
"""

from __future__ import annotations

import numpy as np
import pyqtgraph as pg
import pytest
from PySide6.QtCore import Qt

from plotyx.core.column import ColumnRole
from plotyx.core.project import Project
from plotyx.plot import edit
from plotyx.plot.resolve import prepare, resolve_spec
from plotyx.plot.spec import (
    AxisScale,
    AxisSide,
    LegendPosition,
    LineStyle,
    PlotSpec,
    PlotType,
    SeriesSpec,
    SymbolShape,
)
from plotyx.ui.plot_renderer import PyQtGraphRenderer, pen_style, symbol_of


@pytest.fixture
def scene(qtbot):
    widget = pg.GraphicsLayoutWidget()
    qtbot.addWidget(widget)
    return widget, PyQtGraphRenderer(widget)


@pytest.fixture
def data_project():
    project = Project("P")
    book = project.add_workbook("Book1")
    sheet = book.sheet("Sheet1")
    sheet.add_column("T", np.linspace(1.0, 10.0, 50), role=ColumnRole.X, unit="s")
    sheet.add_column("V", np.linspace(1.0, 5.0, 50), unit="V")
    sheet.add_column("E", np.full(50, 0.2), role=ColumnRole.Y_ERROR)
    return project, sheet


def _spec_with(sheet, plot_type, **overrides):
    series = SeriesSpec(
        y_key=sheet.column("V").key,
        x_key=sheet.column("T").key,
        plot_type=plot_type,
        **overrides,
    )
    return edit.add_series(PlotSpec.empty(), series)


def _render(project, renderer, spec):
    renderer.render(resolve_spec(project, spec))


# -- tables de traduction ----------------------------------------------------------


def test_every_neutral_style_has_a_pyqtgraph_translation():
    """Aucune valeur du vocabulaire neutre ne doit rester intraduite."""
    for style in LineStyle:
        assert isinstance(pen_style(style), Qt.PenStyle)
    for shape in SymbolShape:
        assert (symbol_of(shape) is None) == (shape is SymbolShape.NONE)
    assert pen_style("dash") is Qt.PenStyle.DashLine
    assert symbol_of("square") == "s"


# -- les six types de tracé -----------------------------------------------------------


@pytest.mark.parametrize("plot_type", list(PlotType))
def test_every_plot_type_renders(data_project, scene, plot_type):
    project, sheet = data_project
    _widget, renderer = scene
    spec = _spec_with(
        sheet,
        plot_type,
        y_error_key=sheet.column("E").key if plot_type is PlotType.ERROR_BAR else None,
    )
    _render(project, renderer, spec)
    plot = renderer.plot_items[0]
    assert plot.items, f"{plot_type} n'a produit aucun item"


def test_a_line_has_a_pen_and_no_symbol(data_project, scene):
    project, sheet = data_project
    _widget, renderer = scene
    _render(project, renderer, _spec_with(sheet, PlotType.LINE))
    curve = next(iter(renderer.curves.values()))
    assert curve.opts["pen"] is not None
    assert curve.opts["symbol"] is None


def test_a_scatter_has_symbols_and_no_pen(data_project, scene):
    project, sheet = data_project
    _widget, renderer = scene
    _render(project, renderer, _spec_with(sheet, PlotType.SCATTER))
    curve = next(iter(renderer.curves.values()))
    assert curve.opts["pen"] is None
    assert curve.opts["symbol"] == "o"


def test_line_and_scatter_has_both(data_project, scene):
    project, sheet = data_project
    _widget, renderer = scene
    _render(project, renderer, _spec_with(sheet, PlotType.LINE_SCATTER))
    curve = next(iter(renderer.curves.values()))
    assert curve.opts["pen"] is not None and curve.opts["symbol"] == "o"


def test_error_bars_add_a_dedicated_item(data_project, scene):
    project, sheet = data_project
    _widget, renderer = scene
    spec = _spec_with(sheet, PlotType.ERROR_BAR, y_error_key=sheet.column("E").key)
    _render(project, renderer, spec)
    assert any(isinstance(item, pg.ErrorBarItem) for item in renderer.plot_items[0].items)


def test_bars_use_a_bar_graph_item_and_deduce_their_width(data_project, scene):
    project, sheet = data_project
    _widget, renderer = scene
    _render(project, renderer, _spec_with(sheet, PlotType.BAR))
    bars = [i for i in renderer.plot_items[0].items if isinstance(i, pg.BarGraphItem)]
    assert len(bars) == 1
    assert bars[0].opts["width"] > 0
    # Largeur imposée : elle prime sur la déduction.
    _render(project, renderer, _spec_with(sheet, PlotType.BAR, bar_width=3.0))
    bars = [i for i in renderer.plot_items[0].items if isinstance(i, pg.BarGraphItem)]
    assert bars[0].opts["width"] == 3.0


def test_a_histogram_draws_steps_over_bin_edges(data_project, scene):
    project, sheet = data_project
    _widget, renderer = scene
    spec = _spec_with(sheet, PlotType.HISTOGRAM, histogram_bins=8)
    _render(project, renderer, spec)
    curve = next(iter(renderer.curves.values()))
    x, y = curve.getData()
    assert len(x) == len(y) + 1 == 9


def test_a_precomputed_histogram_is_reused(data_project, scene):
    """Le pré-calcul fait hors du thread UI ne doit pas être refait ici."""
    project, sheet = data_project
    _widget, renderer = scene
    spec = _spec_with(sheet, PlotType.HISTOGRAM, histogram_bins=5)
    resolved = prepare(project, spec)
    assert resolved.panels[0].series[0].histogram is not None
    renderer.render(resolved)
    x, _y = next(iter(renderer.curves.values())).getData()
    assert len(x) == 6


# -- axes, légende, panneaux -------------------------------------------------------------


def test_axis_labels_scale_grid_and_limits_reach_the_plot(data_project, scene):
    project, sheet = data_project
    _widget, renderer = scene
    spec = _spec_with(sheet, PlotType.LINE)
    spec = edit.update_axis(spec, 0, AxisSide.BOTTOM, label="Temps", unit="s", grid=True)
    spec = edit.update_axis(
        spec, 0, AxisSide.LEFT, scale=AxisScale.LOG, auto_range=False, range_min=1, range_max=4
    )
    _render(project, renderer, spec)
    plot = renderer.plot_items[0]
    assert "Temps" in plot.getAxis("bottom").labelText
    assert plot.getAxis("left").logMode
    low, high = plot.getViewBox().viewRange()[1]
    assert low == pytest.approx(1, abs=0.01) and high == pytest.approx(4, abs=0.01)


def test_an_inverted_and_hidden_axis_is_honoured(data_project, scene):
    project, sheet = data_project
    _widget, renderer = scene
    spec = _spec_with(sheet, PlotType.LINE)
    spec = edit.update_axis(spec, 0, AxisSide.BOTTOM, inverted=True, visible=False)
    _render(project, renderer, spec)
    plot = renderer.plot_items[0]
    assert plot.getViewBox().xInverted()
    assert not plot.getAxis("bottom").isVisible()


def test_the_legend_is_created_only_when_asked(data_project, scene):
    project, sheet = data_project
    _widget, renderer = scene
    spec = _spec_with(sheet, PlotType.LINE)
    _render(project, renderer, spec)
    assert renderer.plot_items[0].legend is not None
    _render(project, renderer, edit.update_legend(spec, 0, visible=False))
    assert renderer.plot_items[0].legend is None


@pytest.mark.parametrize("position", list(LegendPosition))
def test_every_legend_position_is_translated(data_project, scene, position):
    project, sheet = data_project
    _widget, renderer = scene
    spec = edit.update_legend(_spec_with(sheet, PlotType.LINE), 0, position=position)
    _render(project, renderer, spec)
    assert renderer.plot_items[0].legend is not None


def test_a_secondary_axis_gets_its_own_linked_view(data_project, scene):
    project, sheet = data_project
    _widget, renderer = scene
    spec = _spec_with(sheet, PlotType.LINE)
    spec = edit.add_series(
        spec, SeriesSpec(y_key=sheet.column("E").key, axis=AxisSide.RIGHT, name="Erreur")
    )
    spec = edit.update_axis(spec, 0, AxisSide.RIGHT, label="Erreur", unit="V")
    _render(project, renderer, spec)
    plot = renderer.plot_items[0]
    assert plot.getAxis("right").isVisible()
    assert "Erreur" in plot.getAxis("right").labelText
    # La série de droite n'appartient pas à la vue principale.
    assert len(renderer.curves) == 2


def test_multiple_panels_are_laid_out_on_the_grid(data_project, scene):
    project, sheet = data_project
    _widget, renderer = scene
    spec = _spec_with(sheet, PlotType.LINE)
    spec = edit.add_panel(spec)
    spec = edit.add_series(spec, SeriesSpec(y_key=sheet.column("V").key), 1)
    spec = edit.update_panel(spec, 1, title="Second")
    _render(project, renderer, spec)
    assert len(renderer.plot_items) == 2
    assert "Second" in renderer.plot_items[1].titleLabel.text


def test_the_graph_title_lands_on_the_first_panel(data_project, scene):
    project, sheet = data_project
    _widget, renderer = scene
    spec = edit.update_title(_spec_with(sheet, PlotType.LINE), "Campagne 3")
    _render(project, renderer, spec)
    assert "Campagne 3" in renderer.plot_items[0].titleLabel.text
    spec = edit.update_panel(spec, 0, title="Panneau A")
    _render(project, renderer, spec)
    assert "Panneau A" in renderer.plot_items[0].titleLabel.text


def test_a_background_colour_is_applied(data_project, scene):
    project, sheet = data_project
    widget, renderer = scene
    from dataclasses import replace

    _render(project, renderer, replace(_spec_with(sheet, PlotType.LINE), background="#101010"))
    assert widget.backgroundBrush().color().name() == "#101010"


# -- séries invisibles et rompues ----------------------------------------------------------


def test_an_invisible_series_draws_nothing(data_project, scene):
    project, sheet = data_project
    _widget, renderer = scene
    spec = _spec_with(sheet, PlotType.LINE)
    series_id = spec.panels[0].series[0].id
    _render(project, renderer, edit.update_series(spec, series_id, visible=False))
    assert renderer.curves == {}


def test_a_broken_series_draws_nothing_and_does_not_raise(data_project, scene):
    project, _sheet = data_project
    _widget, renderer = scene
    spec = edit.add_series(PlotSpec.empty(), SeriesSpec(y_key="clé-morte"))
    _render(project, renderer, spec)  # ne doit pas lever
    assert renderer.curves == {}


def test_a_series_hidden_from_the_legend_still_draws(data_project, scene):
    project, sheet = data_project
    _widget, renderer = scene
    spec = _spec_with(sheet, PlotType.LINE, show_in_legend=False)
    _render(project, renderer, spec)
    assert len(renderer.curves) == 1


# -- performance et chemin rapide -------------------------------------------------------------


def test_downsampling_and_clipping_are_armed(data_project, scene):
    """Les deux réglages qui font tenir 10⁶ points, vérifiés et pas supposés."""
    project, sheet = data_project
    _widget, renderer = scene
    _render(project, renderer, _spec_with(sheet, PlotType.LINE))
    curve = next(iter(renderer.curves.values()))
    assert curve.opts["autoDownsample"] is True
    assert curve.opts["downsampleMethod"] == "peak"
    assert curve.opts["clipToView"] is True


def test_update_data_takes_the_fast_path_without_rebuilding(data_project, scene):
    project, sheet = data_project
    _widget, renderer = scene
    spec = _spec_with(sheet, PlotType.LINE)
    _render(project, renderer, spec)
    curve = next(iter(renderer.curves.values()))
    sheet.column("V").set_values(0, [42.0])
    assert renderer.update_data(resolve_spec(project, spec)) is True
    assert renderer.curves[next(iter(renderer.curves))] is curve  # même item
    assert curve.getData()[1][0] == 42.0


def test_update_data_refuses_when_the_structure_changed(data_project, scene):
    project, sheet = data_project
    _widget, renderer = scene
    spec = _spec_with(sheet, PlotType.LINE)
    _render(project, renderer, spec)
    bigger = edit.add_series(spec, SeriesSpec(y_key=sheet.column("E").key))
    assert renderer.update_data(resolve_spec(project, bigger)) is False


def test_update_data_before_any_render_refuses(data_project, scene):
    project, sheet = data_project
    _widget, renderer = scene
    assert renderer.update_data(resolve_spec(project, _spec_with(sheet, PlotType.LINE))) is False


def test_update_data_also_refreshes_a_histogram(data_project, scene):
    project, sheet = data_project
    _widget, renderer = scene
    spec = _spec_with(sheet, PlotType.HISTOGRAM, histogram_bins=4)
    _render(project, renderer, spec)
    sheet.column("V").set_values(0, np.full(50, 3.0))
    assert renderer.update_data(resolve_spec(project, spec)) is True


@pytest.mark.parametrize("plot_type", [PlotType.BAR, PlotType.ERROR_BAR])
def test_bars_and_error_bars_never_take_the_fast_path(data_project, scene, plot_type):
    """Leurs items secondaires ne sont pas réécrits : ils resteraient périmés.

    Correctif : l'`ErrorBarItem` d'une série est un item distinct de sa
    courbe, et le chemin rapide ne réécrivait que la courbe — une saisie
    dans la colonne d'erreur laissait des moustaches obsolètes à côté d'une
    courbe à jour. Rendre la main à `render` est la seule réponse correcte
    tant que ces items ne savent pas se mettre à jour.
    """
    project, sheet = data_project
    _widget, renderer = scene
    spec = _spec_with(
        sheet,
        plot_type,
        y_error_key=sheet.column("E").key if plot_type is PlotType.ERROR_BAR else None,
    )
    _render(project, renderer, spec)
    assert renderer.update_data(resolve_spec(project, spec)) is False


# -- curseurs ------------------------------------------------------------------------------------


def test_crosshair_is_added_removed_and_moved(data_project, scene):
    project, sheet = data_project
    _widget, renderer = scene
    _render(project, renderer, _spec_with(sheet, PlotType.LINE))
    plot = renderer.plot_items[0]
    before = len(plot.items)

    renderer.set_crosshair(True)
    assert renderer.crosshair_enabled
    lines = [i for i in plot.items if isinstance(i, pg.InfiniteLine)]
    assert len(lines) == 2
    renderer.set_crosshair(True)  # idempotent
    assert len([i for i in plot.items if isinstance(i, pg.InfiniteLine)]) == 2

    renderer.move_crosshair(0, 3.0, 4.0)
    assert lines[0].value() == 3.0 or lines[1].value() == 3.0

    renderer.set_crosshair(False)
    assert len(plot.items) == before


def test_crosshair_survives_a_full_rerender(data_project, scene):
    project, sheet = data_project
    _widget, renderer = scene
    spec = _spec_with(sheet, PlotType.LINE)
    _render(project, renderer, spec)
    renderer.set_crosshair(True)
    _render(project, renderer, spec)
    lines = [i for i in renderer.plot_items[0].items if isinstance(i, pg.InfiniteLine)]
    assert len(lines) == 2


def test_moving_the_crosshair_when_disabled_is_a_no_op(data_project, scene):
    project, sheet = data_project
    _widget, renderer = scene
    _render(project, renderer, _spec_with(sheet, PlotType.LINE))
    renderer.move_crosshair(0, 1.0, 1.0)  # ne doit pas lever


def test_panel_at_and_data_coords(data_project, scene):
    project, sheet = data_project
    widget, renderer = scene
    widget.resize(400, 300)
    _render(project, renderer, _spec_with(sheet, PlotType.LINE))
    centre = renderer.plot_items[0].sceneBoundingRect().center()
    assert renderer.panel_at(centre) == 0
    x, y = renderer.data_coords(0, centre)
    assert isinstance(x, float) and isinstance(y, float)
    outside = renderer.plot_items[0].sceneBoundingRect().bottomRight()
    outside.setX(outside.x() + 5000)
    assert renderer.panel_at(outside) is None
