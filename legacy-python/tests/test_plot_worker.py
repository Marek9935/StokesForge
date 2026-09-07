"""`PlotPrepareWorker` : le motif `QThread` de la phase 5, réutilisé tel quel.

Ce qui est vérifié : les mêmes signaux, la même annulation, et surtout que
le fil ne touche **jamais** au modèle — il ne fait que lire et rendre un
`ResolvedPlot`.
"""

from __future__ import annotations

import numpy as np
import pytest

from plotyx.commands.guard import mutation_guard
from plotyx.core.project import Project
from plotyx.plot import edit
from plotyx.plot.resolve import ResolvedPlot
from plotyx.plot.spec import PlotSpec, PlotType, SeriesSpec
from plotyx.ui.plot_worker import PlotPrepareWorker


@pytest.fixture
def histogram_spec():
    project = Project("P")
    book = project.add_workbook("Book1")
    sheet = book.sheet("Sheet1")
    column = sheet.add_column("V", np.linspace(0.0, 1.0, 5_000))
    spec = edit.add_series(
        PlotSpec.empty(),
        SeriesSpec(y_key=column.key, plot_type=PlotType.HISTOGRAM, histogram_bins=16),
    )
    return project, spec


def test_the_worker_prepares_off_the_ui_thread(histogram_spec, qtbot):
    project, spec = histogram_spec
    worker = PlotPrepareWorker(project, spec)
    results: list[object] = []
    progress: list[float] = []
    worker.ready.connect(results.append)
    worker.progress.connect(progress.append)

    with qtbot.waitSignal(worker.finished, timeout=5000):
        worker.start()

    assert len(results) == 1
    resolved = results[0]
    assert isinstance(resolved, ResolvedPlot)
    assert resolved.panels[0].series[0].histogram is not None
    assert progress and progress[-1] == 1.0


def test_the_worker_never_mutates_the_model(histogram_spec, qtbot):
    project, spec = histogram_spec
    worker = PlotPrepareWorker(project, spec)
    with mutation_guard(), qtbot.waitSignal(worker.finished, timeout=5000):
        worker.start()


def test_cancelling_before_the_start_emits_cancelled(histogram_spec, qtbot):
    project, spec = histogram_spec
    worker = PlotPrepareWorker(project, spec)
    cancelled: list[int] = []
    worker.cancelled.connect(lambda: cancelled.append(1))
    worker.cancel()
    with qtbot.waitSignal(worker.finished, timeout=5000):
        worker.start()
    assert cancelled == [1]


def test_a_failure_is_relayed_and_never_fatal(histogram_spec, qtbot, monkeypatch):
    project, spec = histogram_spec
    monkeypatch.setattr(
        "plotyx.ui.plot_worker.prepare",
        lambda *args, **kwargs: (_ for _ in ()).throw(RuntimeError("boum")),
    )
    worker = PlotPrepareWorker(project, spec)
    failures: list[str] = []
    worker.failed.connect(failures.append)
    with qtbot.waitSignal(worker.finished, timeout=5000):
        worker.start()
    assert failures == ["boum"]


def test_a_worker_failure_is_shown_in_the_graph_banner(histogram_spec, qtbot):
    from plotyx.commands import CommandBus
    from plotyx.ui.graph_window import GraphWindow

    project, spec = histogram_spec
    graph = project.add_graph("G")
    window = GraphWindow(graph, project, CommandBus(project))
    qtbot.addWidget(window)
    window._on_worker_failed("disque plein")
    assert "disque plein" in window.banner_text
    window.disconnect_signals()


# -- appels directs à `run()` -------------------------------------------------
#
# Même raison qu'en phase 5 (voir `tests/test_ui_import_worker.py`) :
# `QThread.run()` s'exécute dans un thread OS géré par Qt, que
# l'instrumentation de couverture posée sur le thread principal ne voit
# pas. Les tests ci-dessus vérifient le comportement réel en thread
# séparé ; ceux-ci appellent `run()` dans le thread de test pour que la
# logique de contrôle soit mesurée.


def test_run_directly_emits_ready(histogram_spec, qapp):
    project, spec = histogram_spec
    worker = PlotPrepareWorker(project, spec)
    results: list[object] = []
    worker.ready.connect(results.append)
    worker.run()
    assert isinstance(results[0], ResolvedPlot)


def test_run_directly_honours_an_early_cancel(histogram_spec, qapp):
    project, spec = histogram_spec
    worker = PlotPrepareWorker(project, spec)
    cancelled: list[int] = []
    ready: list[object] = []
    worker.cancelled.connect(lambda: cancelled.append(1))
    worker.ready.connect(ready.append)
    worker.cancel()
    worker.run()
    assert cancelled == [1] and ready == []


def test_run_directly_relays_an_unexpected_error(histogram_spec, qapp, monkeypatch):
    project, spec = histogram_spec
    monkeypatch.setattr(
        "plotyx.ui.plot_worker.prepare",
        lambda *args, **kwargs: (_ for _ in ()).throw(ValueError("cassé")),
    )
    worker = PlotPrepareWorker(project, spec)
    failures: list[str] = []
    worker.failed.connect(failures.append)
    worker.run()
    assert failures == ["cassé"]
