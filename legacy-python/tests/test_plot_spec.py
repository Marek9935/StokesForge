"""Le `PlotSpec` lui-même : construction, invariants, sérialisation, édition.

Aucune de ces vérifications n'a besoin de Qt ni de PyQtGraph — c'est le
point : la couche de description est testable seule, exactement comme
`plotyx.core`.
"""

from __future__ import annotations

import pytest

from plotyx.plot import edit
from plotyx.plot.spec import (
    DEFAULT_PALETTE,
    AxisScale,
    AxisSide,
    AxisSpec,
    LegendPosition,
    LegendSpec,
    LineSpec,
    LineStyle,
    PanelSpec,
    PlotSpec,
    PlotType,
    SeriesSpec,
    SymbolShape,
    SymbolSpec,
    new_id,
    palette_color,
)

# -- construction et invariants ------------------------------------------------


def test_an_empty_spec_always_has_one_panel():
    spec = PlotSpec.empty("Titre")
    assert spec.title == "Titre"
    assert len(spec.panels) == 1
    assert spec.series_count() == 0
    assert spec.rows == 1 and spec.columns == 1


def test_a_spec_built_without_panels_gets_one_anyway():
    """Un graphe sans panneau ne serait pas rendable : le spec en garantit un."""
    assert len(PlotSpec(panels=()).panels) == 1


def test_a_series_must_designate_a_y_column():
    with pytest.raises(ValueError, match="clé de registre"):
        SeriesSpec(y_key="")


def test_a_series_only_attaches_to_the_left_or_right_axis():
    with pytest.raises(ValueError, match="axe gauche ou droit"):
        SeriesSpec(y_key="k", axis=AxisSide.BOTTOM)


def test_string_values_are_coerced_to_enums():
    """Ce qui vient d'un fichier JSON est du texte : il doit devenir enum."""
    series = SeriesSpec(y_key="k", plot_type="scatter", axis="right")
    assert series.plot_type is PlotType.SCATTER
    assert series.axis is AxisSide.RIGHT
    assert AxisSpec(scale="log").scale is AxisScale.LOG
    assert LineSpec(style="dash").style is LineStyle.DASH
    assert SymbolSpec(shape="square").shape is SymbolShape.SQUARE
    assert LegendSpec(position="bottom_left").position is LegendPosition.BOTTOM_LEFT


def test_series_keys_lists_only_the_references_that_exist():
    series = SeriesSpec(y_key="y", x_key="x", y_error_key="e")
    assert series.keys() == ("x", "y", "e")
    assert series.references("e")
    assert not series.references("absent")


def test_histogram_does_not_need_an_x_column():
    assert not PlotType.HISTOGRAM.needs_x
    assert PlotType.BAR.needs_x


def test_plot_type_draw_flags():
    assert PlotType.LINE.draws_line and not PlotType.LINE.draws_symbols
    assert PlotType.SCATTER.draws_symbols and not PlotType.SCATTER.draws_line
    assert PlotType.LINE_SCATTER.draws_line and PlotType.LINE_SCATTER.draws_symbols
    assert PlotType.ERROR_BAR.draws_line and PlotType.ERROR_BAR.draws_symbols


def test_axis_display_label_never_shows_empty_parentheses():
    assert AxisSpec(label="Temps", unit="s").display_label == "Temps (s)"
    assert AxisSpec(label="Temps").display_label == "Temps"
    assert AxisSpec(unit="s").display_label == "(s)"
    assert AxisSpec().display_label == ""


def test_explicit_range_needs_both_bounds_and_auto_off():
    assert not AxisSpec(range_min=0, range_max=1).has_explicit_range  # auto_range vaut True
    assert not AxisSpec(auto_range=False, range_min=0).has_explicit_range
    assert AxisSpec(auto_range=False, range_min=0, range_max=1).has_explicit_range


def test_ids_are_unique():
    assert new_id() != new_id()
    assert SeriesSpec(y_key="k").id != SeriesSpec(y_key="k").id


def test_palette_cycles():
    assert palette_color(0) == DEFAULT_PALETTE[0]
    assert palette_color(len(DEFAULT_PALETTE)) == DEFAULT_PALETTE[0]


def test_histogram_bins_and_widths_are_sanitised():
    series = SeriesSpec(y_key="k", histogram_bins=0, bar_width=2)
    assert series.histogram_bins == 1
    assert isinstance(series.bar_width, float)


# -- lecture d'un spec composé ---------------------------------------------------


@pytest.fixture
def two_panel_spec() -> PlotSpec:
    spec = PlotSpec.empty("Deux panneaux")
    spec = edit.add_series(spec, SeriesSpec(y_key="y1", x_key="x"))
    spec = edit.add_series(spec, SeriesSpec(y_key="y2", x_key="x", axis=AxisSide.RIGHT))
    spec = edit.add_panel(spec)
    spec = edit.add_series(spec, SeriesSpec(y_key="y3", plot_type=PlotType.HISTOGRAM), 1)
    return spec


def test_iter_series_walks_every_panel(two_panel_spec):
    assert [index for index, _ in two_panel_spec.iter_series()] == [0, 0, 1]
    assert two_panel_spec.series_count() == 3


def test_keys_are_deduplicated_and_ordered(two_panel_spec):
    assert two_panel_spec.keys() == ("x", "y1", "y2", "y3")
    assert two_panel_spec.panels[0].keys() == ("x", "y1", "y2")


def test_a_panel_knows_it_uses_the_secondary_axis(two_panel_spec):
    assert two_panel_spec.panels[0].uses_right_axis
    assert not two_panel_spec.panels[1].uses_right_axis


def test_added_panels_stack_vertically(two_panel_spec):
    assert two_panel_spec.rows == 2
    assert two_panel_spec.columns == 1
    assert two_panel_spec.panels[1].row == 1


def test_panel_index_and_find_series(two_panel_spec):
    panel = two_panel_spec.panels[1]
    assert two_panel_spec.panel_index(panel.id) == 1
    assert two_panel_spec.panel_index("inconnu") is None
    series = panel.series[0]
    assert two_panel_spec.find_series(series.id) == (1, series)
    assert two_panel_spec.find_series("inconnu") is None
    assert panel.find_series("inconnu") is None


def test_panel_axis_lookup_by_side(two_panel_spec):
    panel = two_panel_spec.panels[0]
    assert panel.axis(AxisSide.LEFT) is panel.left
    assert panel.axis("bottom") is panel.bottom


# -- sérialisation ----------------------------------------------------------------


def test_round_trip_through_dict_is_faithful(two_panel_spec):
    payload = two_panel_spec.to_dict()
    assert isinstance(payload["panels"], list)
    assert payload["panels"][0]["series"][0]["plot_type"] == "line"
    assert PlotSpec.from_dict(payload) == two_panel_spec


def test_round_trip_survives_json(two_panel_spec):
    import json

    assert PlotSpec.from_dict(json.loads(json.dumps(two_panel_spec.to_dict()))) == two_panel_spec


def test_unknown_fields_are_ignored_rather_than_fatal():
    """Un spec écrit par une version plus récente doit rester lisible."""
    payload = PlotSpec.empty("T").to_dict()
    payload["future_option"] = 42
    payload["panels"][0]["series"] = [{"y_key": "k", "future_style": "néon"}]
    payload["panels"][0]["left"]["future_axis"] = True
    spec = PlotSpec.from_dict(payload)
    assert spec.title == "T"
    assert spec.panels[0].series[0].y_key == "k"


# -- édition (fonctions pures) ------------------------------------------------------


def test_edit_never_mutates_the_input_spec():
    spec = PlotSpec.empty()
    other = edit.add_series(spec, SeriesSpec(y_key="k"))
    assert spec.series_count() == 0
    assert other.series_count() == 1
    assert other is not spec


def test_series_for_column_picks_the_next_palette_colour():
    spec = PlotSpec.empty()
    first = edit.series_for_column(spec, "y1")
    assert first.line.color == DEFAULT_PALETTE[0]
    assert first.symbol.color == DEFAULT_PALETTE[0]
    spec = edit.add_series(spec, first)
    second = edit.series_for_column(spec, "y2")
    assert second.line.color == DEFAULT_PALETTE[1]
    assert edit.next_series_color(spec) == DEFAULT_PALETTE[1]


def test_series_for_column_accepts_an_explicit_colour_and_overrides():
    series = edit.series_for_column(
        PlotSpec.empty(), "y", "x", color="#123456", plot_type=PlotType.SCATTER
    )
    assert series.line.color == "#123456"
    assert series.x_key == "x"
    assert series.plot_type is PlotType.SCATTER


def test_update_series_accepts_partial_sub_specs(two_panel_spec):
    series_id = two_panel_spec.panels[0].series[0].id
    updated = edit.update_series(
        two_panel_spec, series_id, line={"width": 4.0}, symbol={"size": 12.0}
    )
    changed = updated.find_series(series_id)[1]
    assert changed.line.width == 4.0
    # Les champs non touchés du sous-spec sont conservés.
    assert changed.line.color == two_panel_spec.panels[0].series[0].line.color
    assert changed.symbol.size == 12.0


def test_update_and_remove_reject_an_unknown_series(two_panel_spec):
    with pytest.raises(KeyError):
        edit.update_series(two_panel_spec, "inconnu", visible=False)
    with pytest.raises(KeyError):
        edit.remove_series(two_panel_spec, "inconnu")
    with pytest.raises(KeyError):
        edit.move_series(two_panel_spec, "inconnu", 1)


def test_remove_series_only_touches_its_own_panel(two_panel_spec):
    series_id = two_panel_spec.panels[1].series[0].id
    updated = edit.remove_series(two_panel_spec, series_id)
    assert len(updated.panels[0].series) == 2
    assert updated.panels[1].series == ()


def test_move_series_between_panels_keeps_its_style(two_panel_spec):
    series = two_panel_spec.panels[0].series[0]
    updated = edit.move_series(two_panel_spec, series.id, 1)
    assert updated.panels[1].series[-1] == series
    assert len(updated.panels[0].series) == 1
    # Déplacer une série sur son propre panneau est un non-événement.
    assert edit.move_series(updated, series.id, 1) is updated


def test_update_axis_legend_panel_and_title(two_panel_spec):
    spec = edit.update_axis(two_panel_spec, 0, AxisSide.LEFT, scale=AxisScale.LOG, unit="V")
    assert spec.panels[0].left.scale is AxisScale.LOG
    assert spec.panels[0].left.unit == "V"
    spec = edit.update_legend(spec, 0, visible=False)
    assert not spec.panels[0].legend.visible
    spec = edit.update_panel(spec, 1, title="Distribution")
    assert spec.panels[1].title == "Distribution"
    assert edit.update_title(spec, "Autre").title == "Autre"


def test_add_panel_accepts_an_explicit_grid_position():
    spec = edit.add_panel(PlotSpec.empty(), PanelSpec(row=0, column=1))
    assert spec.rows == 1 and spec.columns == 2


def test_remove_panel_refuses_to_empty_the_graph(two_panel_spec):
    spec = edit.remove_panel(two_panel_spec, 1)
    assert len(spec.panels) == 1
    with pytest.raises(ValueError, match="au moins un panneau"):
        edit.remove_panel(spec, 0)


def test_editing_an_out_of_range_panel_raises(two_panel_spec):
    with pytest.raises(IndexError):
        edit.update_panel(two_panel_spec, 9, title="x")
    with pytest.raises(IndexError):
        edit.remove_panel(edit.add_panel(two_panel_spec), 9)
