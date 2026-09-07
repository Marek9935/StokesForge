"""Résolution des clés d'un `PlotSpec` — et le cas des **références rompues**.

Le point délicat de la phase 6 : le registre est en références faibles, une
colonne supprimée en sort et n'y revient qu'au `undo`. Une série orpheline
ne doit ni planter, ni disparaître silencieusement. C'est ce qui est
vérifié ici, sans aucune UI.
"""

from __future__ import annotations

import numpy as np
import pytest

from plotyx.commands import CommandBus, DeleteColumnCommand
from plotyx.core.column import Column, ColumnRole
from plotyx.core.project import Project
from plotyx.plot import edit
from plotyx.plot.resolve import (
    ComputeCancelled,
    histogram,
    prepare,
    resolve_column,
    resolve_series,
    resolve_spec,
)
from plotyx.plot.spec import PlotSpec, PlotType, SeriesSpec


@pytest.fixture
def plotted():
    """Un projet, deux colonnes numériques, un spec qui les trace."""
    project = Project("P")
    book = project.add_workbook("Book1")
    sheet = book.sheet("Sheet1")
    x = sheet.add_column("T", [0.0, 1.0, 2.0, 3.0], role=ColumnRole.X, unit="s")
    y = sheet.add_column("V", [10.0, 20.0, 30.0, 40.0], unit="V", long_name="Tension")
    spec = edit.add_series(PlotSpec.empty(), SeriesSpec(y_key=y.key, x_key=x.key))
    return project, sheet, x, y, spec


# -- résolution nominale ---------------------------------------------------------


def test_a_series_resolves_to_the_columns_arrays(plotted):
    project, _sheet, x, y, spec = plotted
    resolved = resolve_spec(project, spec).panels[0].series[0]
    assert resolved.ok and not resolved.broken
    assert list(resolved.x) == list(x.values)
    assert list(resolved.y) == list(y.values)
    assert resolved.point_count == 4
    assert resolved.label == "Tension"  # le long_name prime sur le nom court


def test_resolution_never_copies_the_data(plotted):
    """« Référence, pas copie » : le tableau résolu partage la mémoire."""
    project, _sheet, _x, y, spec = plotted
    resolved = resolve_spec(project, spec).panels[0].series[0]
    assert np.shares_memory(resolved.y, y.values)


def test_the_label_follows_a_column_rename_without_touching_the_spec(plotted):
    project, sheet, _x, y, spec = plotted
    sheet.rename_column(y, "Signal")
    y.set_metadata(long_name="")
    assert resolve_spec(project, spec).panels[0].series[0].label == "Signal"


def test_an_explicit_series_name_wins_over_the_column(plotted):
    project, _sheet, _x, y, _spec = plotted
    series = SeriesSpec(y_key=y.key, name="Essai 3")
    assert resolve_series(project, series).label == "Essai 3"


def test_a_series_without_x_gets_the_row_index(plotted):
    project, _sheet, _x, y, _spec = plotted
    resolved = resolve_series(project, SeriesSpec(y_key=y.key))
    assert list(resolved.x) == [0.0, 1.0, 2.0, 3.0]


def test_a_histogram_series_needs_no_x_at_all(plotted):
    project, _sheet, _x, y, _spec = plotted
    resolved = resolve_series(
        project, SeriesSpec(y_key=y.key, x_key="clé-inexistante", plot_type=PlotType.HISTOGRAM)
    )
    assert resolved.ok
    assert resolved.x is None


def test_columns_of_different_lengths_are_truncated_to_the_common_prefix(plotted):
    """Longueur libre (décision phase 1) : on trace le préfixe commun."""
    project, sheet, x, y, _spec = plotted
    errors = sheet.add_column("E", [1.0, 1.0], role=ColumnRole.Y_ERROR)
    resolved = resolve_series(
        project,
        SeriesSpec(y_key=y.key, x_key=x.key, y_error_key=errors.key, plot_type=PlotType.ERROR_BAR),
    )
    assert resolved.ok and resolved.truncated
    assert resolved.point_count == 2
    assert list(resolved.y) == [10.0, 20.0]


# -- références rompues ------------------------------------------------------------


def test_deleting_the_column_makes_the_series_broken_not_absent(plotted, qapp):
    project, sheet, _x, y, spec = plotted
    bus = CommandBus(project)
    bus.push(DeleteColumnCommand(sheet, y))

    resolved = resolve_spec(project, spec)
    series = resolved.panels[0].series[0]
    # La série existe toujours dans le spec — elle n'est pas traçable.
    assert spec.series_count() == 1
    assert series.broken and not series.ok
    assert series.missing == ("y",)
    assert series.label == "(référence rompue)"
    assert resolved.broken_series == (series,)
    assert resolved.point_count == 0


def test_undoing_the_deletion_repairs_the_series_on_its_own(plotted, qapp):
    """Le registre réinscrit la colonne **sous la même clé** : rien à recâbler."""
    project, sheet, _x, y, spec = plotted
    bus = CommandBus(project)
    bus.push(DeleteColumnCommand(sheet, y))
    assert resolve_spec(project, spec).panels[0].series[0].broken
    bus.undo()
    assert resolve_spec(project, spec).panels[0].series[0].ok


def test_a_missing_x_or_error_column_is_reported_by_role(plotted):
    project, _sheet, _x, y, _spec = plotted
    resolved = resolve_series(
        project,
        SeriesSpec(
            y_key=y.key,
            x_key="absent-x",
            x_error_key="absent-xe",
            y_error_key="absent-ye",
        ),
    )
    assert set(resolved.missing) == {"x", "xErr", "yErr"}


def test_a_text_column_is_not_plottable(plotted):
    project, sheet, _x, _y, _spec = plotted
    text = sheet.insert_column(0, Column("Note", ["a", "b"], column_type="text"))
    assert resolve_series(project, SeriesSpec(y_key=text.key)).missing == ("y",)


def test_resolve_column_covers_absent_empty_and_wrong_type(plotted):
    project, sheet, _x, _y, _spec = plotted
    assert resolve_column(project, None) is None
    assert resolve_column(project, "") is None
    assert resolve_column(project, "clé-inconnue") is None
    # Une clé qui résout vers autre chose qu'une colonne n'est pas traçable.
    assert resolve_column(project, sheet.key) is None


def test_an_empty_column_is_resolvable_but_not_ok(plotted):
    project, sheet, _x, _y, _spec = plotted
    empty = sheet.add_column("Vide", [])
    resolved = resolve_series(project, SeriesSpec(y_key=empty.key))
    assert not resolved.broken  # la colonne existe...
    assert not resolved.ok  # ...mais il n'y a rien à tracer


# -- pré-calcul (le seul O(n) de la chaîne) -------------------------------------------


def test_prepare_precomputes_histograms_only(plotted):
    project, _sheet, x, y, _spec = plotted
    spec = edit.add_series(PlotSpec.empty(), SeriesSpec(y_key=y.key, x_key=x.key))
    spec = edit.add_series(
        spec, SeriesSpec(y_key=y.key, plot_type=PlotType.HISTOGRAM, histogram_bins=4)
    )
    steps: list[float] = []
    resolved = prepare(project, spec, progress=steps.append)
    assert resolved.panels[0].series[0].histogram is None
    edges, counts = resolved.panels[0].series[1].histogram
    assert len(edges) == 5 and len(counts) == 4
    assert steps == [0.5, 1.0]


def test_prepare_is_cancellable(plotted):
    project, _sheet, _x, y, _spec = plotted
    spec = edit.add_series(PlotSpec.empty(), SeriesSpec(y_key=y.key))
    with pytest.raises(ComputeCancelled):
        prepare(project, spec, should_cancel=lambda: True)


def test_histogram_ignores_nan_and_survives_an_all_nan_column():
    edges, counts = histogram(np.array([1.0, np.nan, 3.0]), bins=2)
    assert counts.sum() == 2
    edges, counts = histogram(np.array([np.nan, np.nan]), bins=5)
    assert counts.size == 0 and edges.size == 1
