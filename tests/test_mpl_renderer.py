"""Le renderer Matplotlib : traduction fidèle du vocabulaire neutre.

Le pendant de `test_plot_renderer.py`, pour le moteur devenu **le défaut**
le 2026-09-07. Ce qui est vérifié ici, ce n'est pas « ça a l'air joli » —
c'est que chaque notion du `PlotSpec` arrive bien dans le bon réglage
Matplotlib, et que ce que la fenêtre attend d'un renderer (pan, zoom,
réticule, recadrage, chemin rapide) est réellement là et non supposé.

Les deux fichiers sont volontairement parallèles, test pour test : c'est
ce qui rend visible, à la lecture, ce que le visualiseur léger n'honore
pas — et c'est aussi ce qui ferait apparaître une divergence de
vocabulaire, la seule chose que la couche `PlotSpec` a pour mission
d'empêcher.
"""

from __future__ import annotations

from dataclasses import replace

import numpy as np
import pytest
from matplotlib.backend_bases import MouseButton, MouseEvent
from matplotlib.lines import Line2D
from matplotlib.patches import StepPatch

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
from plotyx.ui.mpl_renderer import (
    MatplotlibRenderer,
    _scaled_range,
    line_style,
    marker_of,
    new_canvas,
)


@pytest.fixture
def scene(qtbot):
    canvas = new_canvas()
    qtbot.addWidget(canvas)
    canvas.resize(400, 300)
    return canvas, MatplotlibRenderer(canvas)


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


def test_every_neutral_style_has_a_matplotlib_translation():
    """Aucune valeur du vocabulaire neutre ne doit rester intraduite."""
    for style in LineStyle:
        assert isinstance(line_style(style), str)
    for shape in SymbolShape:
        assert (marker_of(shape) is None) == (shape is SymbolShape.NONE)
    assert line_style("dash") == "--"
    assert marker_of("square") == "s"


# -- les six types de tracé -----------------------------------------------------------


@pytest.mark.parametrize("plot_type", list(PlotType))
def test_every_plot_type_renders(data_project, scene, plot_type):
    project, sheet = data_project
    _canvas, renderer = scene
    spec = _spec_with(
        sheet,
        plot_type,
        y_error_key=sheet.column("E").key if plot_type is PlotType.ERROR_BAR else None,
    )
    _render(project, renderer, spec)
    ax = renderer.axes[0]
    assert ax.lines or ax.patches or ax.containers, f"{plot_type} n'a produit aucun artiste"


def test_a_line_has_a_stroke_and_no_marker(data_project, scene):
    project, sheet = data_project
    _canvas, renderer = scene
    _render(project, renderer, _spec_with(sheet, PlotType.LINE))
    curve = next(iter(renderer.curves.values()))
    assert curve.get_linestyle() == "-"
    assert curve.get_marker() in ("None", "", None)


def test_a_scatter_has_markers_and_no_stroke(data_project, scene):
    project, sheet = data_project
    _canvas, renderer = scene
    _render(project, renderer, _spec_with(sheet, PlotType.SCATTER))
    curve = next(iter(renderer.curves.values()))
    assert curve.get_linestyle() == "None"
    assert curve.get_marker() == "o"


def test_line_and_scatter_has_both(data_project, scene):
    project, sheet = data_project
    _canvas, renderer = scene
    _render(project, renderer, _spec_with(sheet, PlotType.LINE_SCATTER))
    curve = next(iter(renderer.curves.values()))
    assert curve.get_linestyle() == "-" and curve.get_marker() == "o"


def test_error_bars_add_a_dedicated_container(data_project, scene):
    project, sheet = data_project
    _canvas, renderer = scene
    spec = _spec_with(sheet, PlotType.ERROR_BAR, y_error_key=sheet.column("E").key)
    _render(project, renderer, spec)
    assert renderer.axes[0].containers, "aucune moustache d'erreur"


def test_bars_deduce_their_width_and_honour_an_imposed_one(data_project, scene):
    project, sheet = data_project
    _canvas, renderer = scene
    _render(project, renderer, _spec_with(sheet, PlotType.BAR))
    bars = renderer.axes[0].patches
    assert bars and bars[0].get_width() > 0
    # Largeur imposée : elle prime sur la déduction.
    _render(project, renderer, _spec_with(sheet, PlotType.BAR, bar_width=3.0))
    assert renderer.axes[0].patches[0].get_width() == 3.0


def test_a_histogram_draws_steps_over_bin_edges(data_project, scene):
    project, sheet = data_project
    _canvas, renderer = scene
    _render(project, renderer, _spec_with(sheet, PlotType.HISTOGRAM, histogram_bins=8))
    patch = next(iter(renderer.curves.values()))
    assert isinstance(patch, StepPatch)
    edges, counts = renderer.series_xy(next(iter(renderer.curves)))
    assert len(edges) == len(counts) + 1 == 9


def test_a_precomputed_histogram_is_reused(data_project, scene):
    """Le pré-calcul fait hors du thread UI ne doit pas être refait ici."""
    project, sheet = data_project
    _canvas, renderer = scene
    spec = _spec_with(sheet, PlotType.HISTOGRAM, histogram_bins=5)
    resolved = prepare(project, spec)
    assert resolved.panels[0].series[0].histogram is not None
    renderer.render(resolved)
    edges, _counts = renderer.series_xy(next(iter(renderer.curves)))
    assert len(edges) == 6


# -- axes, légende, panneaux -------------------------------------------------------------


def test_axis_labels_scale_grid_and_limits_reach_the_figure(data_project, scene):
    project, sheet = data_project
    _canvas, renderer = scene
    spec = _spec_with(sheet, PlotType.LINE)
    spec = edit.update_axis(spec, 0, AxisSide.BOTTOM, label="Temps", unit="s", grid=True)
    spec = edit.update_axis(
        spec, 0, AxisSide.LEFT, scale=AxisScale.LOG, auto_range=False, range_min=1, range_max=4
    )
    _render(project, renderer, spec)
    ax = renderer.axes[0]
    assert "Temps" in ax.get_xlabel() and "(s)" in ax.get_xlabel()
    assert ax.get_yscale() == "log"
    assert ax.get_ylim() == pytest.approx((1.0, 4.0))


def test_an_inverted_and_hidden_axis_is_honoured(data_project, scene):
    project, sheet = data_project
    _canvas, renderer = scene
    spec = _spec_with(sheet, PlotType.LINE)
    spec = edit.update_axis(spec, 0, AxisSide.BOTTOM, inverted=True, visible=False)
    _render(project, renderer, spec)
    ax = renderer.axes[0]
    assert ax.xaxis.get_inverted()
    assert not ax.xaxis.get_visible()


def test_the_legend_is_created_only_when_asked(data_project, scene):
    project, sheet = data_project
    _canvas, renderer = scene
    spec = _spec_with(sheet, PlotType.LINE)
    _render(project, renderer, spec)
    assert renderer.axes[0].get_legend() is not None
    _render(project, renderer, edit.update_legend(spec, 0, visible=False))
    assert renderer.axes[0].get_legend() is None


@pytest.mark.parametrize("position", list(LegendPosition))
def test_every_legend_position_is_translated(data_project, scene, position):
    project, sheet = data_project
    _canvas, renderer = scene
    spec = edit.update_legend(_spec_with(sheet, PlotType.LINE), 0, position=position)
    _render(project, renderer, spec)
    assert renderer.axes[0].get_legend() is not None


def test_a_series_hidden_from_the_legend_is_not_listed_but_is_drawn(data_project, scene):
    project, sheet = data_project
    _canvas, renderer = scene
    _render(project, renderer, _spec_with(sheet, PlotType.LINE, show_in_legend=False))
    assert len(renderer.curves) == 1
    assert renderer.axes[0].get_legend() is None


def test_a_secondary_axis_gets_its_own_twin_and_enters_the_legend(data_project, scene):
    project, sheet = data_project
    _canvas, renderer = scene
    spec = _spec_with(sheet, PlotType.LINE)
    spec = edit.add_series(
        spec, SeriesSpec(y_key=sheet.column("E").key, axis=AxisSide.RIGHT, name="Erreur")
    )
    spec = edit.update_axis(spec, 0, AxisSide.RIGHT, label="Erreur", unit="V")
    _render(project, renderer, spec)
    assert len(renderer.curves) == 2
    legend = renderer.axes[0].get_legend()
    # La série de droite n'est pas enfant du panneau : sans la collecte
    # explicite du renderer, la légende l'ignorerait.
    assert [text.get_text() for text in legend.get_texts()].count("Erreur") == 1


def test_multiple_panels_are_laid_out_on_the_grid(data_project, scene):
    project, sheet = data_project
    _canvas, renderer = scene
    spec = _spec_with(sheet, PlotType.LINE)
    spec = edit.add_panel(spec)
    spec = edit.add_series(spec, SeriesSpec(y_key=sheet.column("V").key), 1)
    spec = edit.update_panel(spec, 1, title="Second")
    _render(project, renderer, spec)
    assert len(renderer.axes) == 2
    assert renderer.axes[1].get_title() == "Second"


def test_the_graph_title_becomes_the_figure_title(data_project, scene):
    """Divergence assumée avec PyQtGraph : `suptitle` ne déforme pas la grille."""
    project, sheet = data_project
    _canvas, renderer = scene
    spec = edit.update_title(_spec_with(sheet, PlotType.LINE), "Campagne 3")
    spec = edit.update_panel(spec, 0, title="Panneau A")
    _render(project, renderer, spec)
    assert renderer.figure.get_suptitle() == "Campagne 3"
    assert renderer.axes[0].get_title() == "Panneau A"


def test_a_background_colour_is_applied(data_project, scene):
    project, sheet = data_project
    _canvas, renderer = scene
    _render(project, renderer, replace(_spec_with(sheet, PlotType.LINE), background="#101010"))
    assert renderer.figure.get_facecolor()[:3] == pytest.approx((0.0625,) * 3, abs=0.01)


# -- séries invisibles et rompues ----------------------------------------------------------


def test_an_invisible_series_draws_nothing(data_project, scene):
    project, sheet = data_project
    _canvas, renderer = scene
    spec = _spec_with(sheet, PlotType.LINE)
    series_id = spec.panels[0].series[0].id
    _render(project, renderer, edit.update_series(spec, series_id, visible=False))
    assert renderer.curves == {}


def test_a_broken_series_draws_nothing_and_does_not_raise(data_project, scene):
    project, _sheet = data_project
    _canvas, renderer = scene
    spec = edit.add_series(PlotSpec.empty(), SeriesSpec(y_key="clé-morte"))
    _render(project, renderer, spec)  # ne doit pas lever
    assert renderer.curves == {}


# -- chemin rapide -----------------------------------------------------------------------


def test_update_data_takes_the_fast_path_without_rebuilding(data_project, scene):
    project, sheet = data_project
    _canvas, renderer = scene
    spec = _spec_with(sheet, PlotType.LINE)
    _render(project, renderer, spec)
    curve = next(iter(renderer.curves.values()))
    sheet.column("V").set_values(0, [42.0])
    assert renderer.update_data(resolve_spec(project, spec)) is True
    assert renderer.curves[next(iter(renderer.curves))] is curve  # même artiste
    assert renderer.series_xy(next(iter(renderer.curves)))[1][0] == 42.0


def test_update_data_refuses_when_the_structure_changed(data_project, scene):
    project, sheet = data_project
    _canvas, renderer = scene
    spec = _spec_with(sheet, PlotType.LINE)
    _render(project, renderer, spec)
    bigger = edit.add_series(spec, SeriesSpec(y_key=sheet.column("E").key))
    assert renderer.update_data(resolve_spec(project, bigger)) is False


def test_update_data_before_any_render_refuses(data_project, scene):
    project, sheet = data_project
    _canvas, renderer = scene
    assert renderer.update_data(resolve_spec(project, _spec_with(sheet, PlotType.LINE))) is False


def test_update_data_also_refreshes_a_histogram(data_project, scene):
    project, sheet = data_project
    _canvas, renderer = scene
    spec = _spec_with(sheet, PlotType.HISTOGRAM, histogram_bins=4)
    _render(project, renderer, spec)
    sheet.column("V").set_values(0, np.full(50, 3.0))
    assert renderer.update_data(resolve_spec(project, spec)) is True


@pytest.mark.parametrize("plot_type", [PlotType.BAR, PlotType.ERROR_BAR])
def test_bars_and_error_bars_never_take_the_fast_path(data_project, scene, plot_type):
    """Leurs artistes secondaires ne se réécrivent pas : ils resteraient périmés."""
    project, sheet = data_project
    _canvas, renderer = scene
    spec = _spec_with(
        sheet,
        plot_type,
        y_error_key=sheet.column("E").key if plot_type is PlotType.ERROR_BAR else None,
    )
    _render(project, renderer, spec)
    assert renderer.update_data(resolve_spec(project, spec)) is False


def test_the_fast_path_follows_the_data_while_the_axes_are_automatic(data_project, scene):
    """Matplotlib ne recadre rien tout seul : c'est le renderer qui s'en charge."""
    project, sheet = data_project
    _canvas, renderer = scene
    spec = _spec_with(sheet, PlotType.LINE)
    _render(project, renderer, spec)
    sheet.column("V").set_values(0, [1000.0])
    assert renderer.update_data(resolve_spec(project, spec)) is True
    assert renderer.axes[0].get_ylim()[1] > 500.0


# -- curseurs ------------------------------------------------------------------------------------


def test_crosshair_is_added_removed_and_moved(data_project, scene):
    project, sheet = data_project
    _canvas, renderer = scene
    _render(project, renderer, _spec_with(sheet, PlotType.LINE))
    ax = renderer.axes[0]
    before = len(ax.lines)

    renderer.set_crosshair(True)
    assert renderer.crosshair_enabled
    assert len(ax.lines) == before + 2
    renderer.set_crosshair(True)  # idempotent
    assert len(ax.lines) == before + 2

    renderer.move_crosshair(0, 3.0, 4.0)
    vline, hline = renderer._crosshair_items[0]
    assert vline.get_xdata()[0] == 3.0
    assert hline.get_ydata()[0] == 4.0

    renderer.set_crosshair(False)
    assert len(ax.lines) == before


def test_the_crosshair_lines_are_animated(data_project, scene):
    """Sinon le fond capturé les contiendrait et le blit laisserait des traînées."""
    project, sheet = data_project
    _canvas, renderer = scene
    _render(project, renderer, _spec_with(sheet, PlotType.LINE))
    renderer.set_crosshair(True)
    vline, hline = renderer._crosshair_items[0]
    assert vline.get_animated() and hline.get_animated()


def test_crosshair_survives_a_full_rerender(data_project, scene):
    project, sheet = data_project
    _canvas, renderer = scene
    spec = _spec_with(sheet, PlotType.LINE)
    _render(project, renderer, spec)
    renderer.set_crosshair(True)
    _render(project, renderer, spec)
    assert len(renderer._crosshair_items) == 1


def test_moving_the_crosshair_when_disabled_is_a_no_op(data_project, scene):
    project, sheet = data_project
    _canvas, renderer = scene
    _render(project, renderer, _spec_with(sheet, PlotType.LINE))
    renderer.move_crosshair(0, 1.0, 1.0)  # ne doit pas lever


# -- interactions --------------------------------------------------------------------------------


def _at_centre(renderer, canvas):
    """Coordonnées écran du centre du premier panneau."""
    canvas.draw()
    bbox = renderer.axes[0].bbox
    return (bbox.x0 + bbox.x1) / 2, (bbox.y0 + bbox.y1) / 2


def _send(canvas, name, x, y, button=None, step=0):
    event = MouseEvent(name, canvas, int(x), int(y), button=button, step=step)
    canvas.callbacks.process(name, event)
    return event


def test_the_wheel_zooms_around_the_cursor(data_project, scene):
    project, sheet = data_project
    canvas, renderer = scene
    _render(project, renderer, _spec_with(sheet, PlotType.LINE))
    x, y = _at_centre(renderer, canvas)
    before = renderer.axes[0].get_xlim()
    _send(canvas, "scroll_event", x, y, step=1)
    after = renderer.axes[0].get_xlim()
    assert (after[1] - after[0]) < (before[1] - before[0])
    # Et un cran en arrière rouvre la vue.
    _send(canvas, "scroll_event", x, y, step=-1)
    back = renderer.axes[0].get_xlim()
    assert (back[1] - back[0]) > (after[1] - after[0])


def test_zooming_disarms_the_automatic_range_and_auto_range_rearms_it(data_project, scene):
    project, sheet = data_project
    canvas, renderer = scene
    _render(project, renderer, _spec_with(sheet, PlotType.LINE))
    x, y = _at_centre(renderer, canvas)
    _send(canvas, "scroll_event", x, y, step=1)
    assert renderer._auto[0] == [False, False]
    renderer.auto_range()
    assert renderer._auto[0] == [True, True]
    low, high = renderer.axes[0].get_xlim()
    assert low <= 1.0 and high >= 10.0


def test_dragging_pans_the_view(data_project, scene):
    project, sheet = data_project
    canvas, renderer = scene
    _render(project, renderer, _spec_with(sheet, PlotType.LINE))
    x, y = _at_centre(renderer, canvas)
    before = renderer.axes[0].get_xlim()
    _send(canvas, "button_press_event", x, y, button=MouseButton.LEFT)
    _send(canvas, "motion_notify_event", x - 40, y, button=MouseButton.LEFT)
    _send(canvas, "button_release_event", x - 40, y, button=MouseButton.LEFT)
    after = renderer.axes[0].get_xlim()
    assert after[0] > before[0] and after[1] > before[1]
    assert (after[1] - after[0]) == pytest.approx(before[1] - before[0], rel=1e-6)


def test_hovering_reports_the_coordinates_and_leaving_clears_them(data_project, scene):
    project, sheet = data_project
    canvas, renderer = scene
    _render(project, renderer, _spec_with(sheet, PlotType.LINE))
    seen: list[tuple[float, float] | None] = []
    renderer.set_coords_callback(seen.append)
    x, y = _at_centre(renderer, canvas)
    _send(canvas, "motion_notify_event", x, y)
    assert seen and seen[-1] is not None
    _send(canvas, "motion_notify_event", 1, 1)  # hors du panneau
    assert seen[-1] is None


def test_panel_at_recognises_a_secondary_axis_as_its_panel(data_project, scene):
    project, sheet = data_project
    _canvas, renderer = scene
    spec = _spec_with(sheet, PlotType.LINE)
    spec = edit.add_series(
        spec, SeriesSpec(y_key=sheet.column("E").key, axis=AxisSide.RIGHT)
    )
    _render(project, renderer, spec)
    assert renderer.panel_at(renderer.axes[0]) == 0
    assert renderer.panel_at(renderer._right_axes[0]) == 0
    assert renderer.panel_at(None) is None


# -- cycle de vie --------------------------------------------------------------------------------


def test_detaching_stops_the_callbacks_and_is_idempotent(data_project, scene):
    project, sheet = data_project
    canvas, renderer = scene
    _render(project, renderer, _spec_with(sheet, PlotType.LINE))
    seen: list[object] = []
    renderer.set_coords_callback(seen.append)
    renderer.detach()
    renderer.detach()  # ne doit pas lever
    _send(canvas, "motion_notify_event", 100, 100)
    assert seen == []


def test_series_xy_of_an_unknown_series_is_none(data_project, scene):
    project, sheet = data_project
    _canvas, renderer = scene
    _render(project, renderer, _spec_with(sheet, PlotType.LINE))
    assert renderer.series_xy("inconnue") is None
    assert isinstance(next(iter(renderer.curves.values())), Line2D)


# -- échelles et limites : les cas que le zoom peut casser -----------------------------


def test_zooming_a_log_axis_stays_in_log_space(data_project, scene):
    """Un zoom uniforme sur un axe log se calcule sur les logarithmes.

    Appliquer le facteur aux valeurs elles-mêmes déplacerait le centre du
    zoom à chaque cran — c'est le genre d'erreur qui ne se voit pas sur un
    axe linéaire et saute aux yeux sur un spectre.
    """
    project, sheet = data_project
    canvas, renderer = scene
    spec = edit.update_axis(_spec_with(sheet, PlotType.LINE), 0, AxisSide.BOTTOM,
                            scale=AxisScale.LOG)
    _render(project, renderer, spec)
    x, y = _at_centre(renderer, canvas)
    ax = renderer.axes[0]
    low, high = ax.get_xlim()
    focus = ax.transData.inverted().transform((x, y))[0]

    _send(canvas, "scroll_event", x, y, step=1)
    new_low, new_high = ax.get_xlim()
    assert new_low > low and new_high < high
    # Le point sous le curseur n'a pas bougé : c'est la définition d'un
    # zoom centré, et elle ne tient en log que si le calcul y a eu lieu.
    ratio_before = np.log10(focus / low) / np.log10(high / low)
    ratio_after = np.log10(focus / new_low) / np.log10(new_high / new_low)
    assert ratio_after == pytest.approx(ratio_before, abs=0.02)


def test_a_log_axis_reaching_zero_refuses_to_zoom_rather_than_raise(data_project, scene):
    """`log10(0)` n'est pas une raison de faire tomber l'application."""
    project, sheet = data_project
    _canvas, renderer = scene
    _render(project, renderer, _spec_with(sheet, PlotType.LINE))
    assert _scaled_range(0.0, 10.0, 5.0, 0.8, log=True) == (0.0, 10.0)


def test_an_explicit_x_range_survives_a_data_update(data_project, scene):
    """Des bornes fixées dans le panneau ne doivent pas être recadrées."""
    project, sheet = data_project
    _canvas, renderer = scene
    spec = edit.update_axis(
        _spec_with(sheet, PlotType.LINE),
        0,
        AxisSide.BOTTOM,
        auto_range=False,
        range_min=2,
        range_max=6,
    )
    _render(project, renderer, spec)
    assert renderer.axes[0].get_xlim() == pytest.approx((2.0, 6.0))
    sheet.column("T").set_values(0, [-500.0])
    assert renderer.update_data(resolve_spec(project, spec)) is True
    assert renderer.axes[0].get_xlim() == pytest.approx((2.0, 6.0))


def test_an_explicit_range_on_the_secondary_axis_is_honoured(data_project, scene):
    project, sheet = data_project
    _canvas, renderer = scene
    spec = _spec_with(sheet, PlotType.LINE)
    spec = edit.add_series(
        spec, SeriesSpec(y_key=sheet.column("E").key, axis=AxisSide.RIGHT, name="Erreur")
    )
    spec = edit.update_axis(
        spec, 0, AxisSide.RIGHT, auto_range=False, range_min=0, range_max=1
    )
    _render(project, renderer, spec)
    assert renderer._right_axes[0].get_ylim() == pytest.approx((0.0, 1.0))


def test_auto_range_also_recenters_the_secondary_axis(data_project, scene):
    project, sheet = data_project
    _canvas, renderer = scene
    spec = _spec_with(sheet, PlotType.LINE)
    spec = edit.add_series(
        spec, SeriesSpec(y_key=sheet.column("E").key, axis=AxisSide.RIGHT, name="Erreur")
    )
    _render(project, renderer, spec)
    twin = renderer._right_axes[0]
    twin.set_ylim(100.0, 200.0)
    renderer.auto_range()
    low, high = twin.get_ylim()
    assert low < 0.2 < high
