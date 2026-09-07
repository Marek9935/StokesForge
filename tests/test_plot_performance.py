"""Performance du tracé à **10⁶ points** — mesurée, pas présumée.

La roadmap l'exige : « Perf : `downsample`/`clipToView` sur gros datasets,
tests à 10⁶ points ». Ce qui est mesuré ici :

1. **la résolution** d'une série, qui doit être indépendante du nombre de
   points (c'est le sens de « référence, pas copie ») ;
2. **le premier rendu** d'une courbe d'un million de points ;
3. **le zoom**, où `clipToView` doit rendre le coût indépendant de la
   longueur totale ;
4. **la mise à jour des données**, chemin rapide du rafraîchissement
   automatique ;
5. **le seul calcul en O(n)** de la chaîne, le binning d'un histogramme,
   qui justifie (ou non) le passage par un `QThread`.

Les seuils sont volontairement larges : ce test protège d'une régression
d'ordre de grandeur (une copie de tableau qui se glisse dans le chemin,
un `downsample` désarmé), pas d'une variation de quelques pourcents entre
machines. Les valeurs observées sur la machine de dev sont dans le rapport
de phase.
"""

from __future__ import annotations

import time

import numpy as np
import pyqtgraph as pg
import pytest

from plotyx.core.column import ColumnRole
from plotyx.core.project import Project
from plotyx.plot import edit
from plotyx.plot.resolve import histogram, prepare, resolve_spec
from plotyx.plot.spec import PlotSpec, PlotType, SeriesSpec
from plotyx.ui.mpl_renderer import MatplotlibRenderer, new_canvas
from plotyx.ui.plot_renderer import PyQtGraphRenderer
from plotyx.ui.plot_worker import ASYNC_POINT_THRESHOLD

POINTS = 1_000_000


@pytest.fixture(scope="module")
def big_project():
    project = Project("Perf")
    book = project.add_workbook("Book1")
    sheet = book.sheet("Sheet1")
    x = np.linspace(0.0, 1000.0, POINTS)
    sheet.add_column("T", x, role=ColumnRole.X, unit="s")
    sheet.add_column("V", np.sin(x) + 0.1 * np.random.default_rng(0).normal(size=POINTS))
    return project, sheet


@pytest.fixture
def big_spec(big_project):
    _project, sheet = big_project
    return edit.add_series(
        PlotSpec.empty(),
        SeriesSpec(y_key=sheet.column("V").key, x_key=sheet.column("T").key),
    )


def test_the_dataset_really_holds_a_million_points(big_project):
    _project, sheet = big_project
    assert len(sheet.column("V")) == POINTS


def test_resolution_is_independent_of_the_point_count(big_project, big_spec):
    """« Référence, pas copie » : résoudre ne doit toucher aucune donnée."""
    project, sheet = big_project
    start = time.perf_counter()
    for _ in range(100):
        resolve_spec(project, big_spec)
    elapsed = time.perf_counter() - start
    assert elapsed < 0.5, f"résolution trop lente : {elapsed:.3f}s pour 100 résolutions"
    resolved = resolve_spec(project, big_spec).panels[0].series[0]
    assert np.shares_memory(resolved.y, sheet.column("V").values)


def test_first_render_of_a_million_points(big_project, big_spec, qtbot):
    project, _sheet = big_project
    widget = pg.GraphicsLayoutWidget()
    qtbot.addWidget(widget)
    widget.resize(900, 600)
    renderer = PyQtGraphRenderer(widget)
    resolved = resolve_spec(project, big_spec)

    start = time.perf_counter()
    renderer.render(resolved)
    widget.grab()  # force la peinture, sinon on ne mesure que la construction
    elapsed = time.perf_counter() - start

    curve = next(iter(renderer.curves.values()))
    assert curve.opts["autoDownsample"] and curve.opts["clipToView"]
    # La donnée d'origine est intacte (aucune copie décimée n'est stockée),
    # mais ce qui est réellement peint est décimé d'un ordre de grandeur :
    # c'est exactement l'effet attendu de `downsample` + `clipToView`.
    assert curve.getOriginalDataset()[1].size == POINTS
    displayed = curve.getData()[1].size
    assert displayed < POINTS // 10, f"le downsampling n'a pas joué ({displayed} points peints)"
    assert elapsed < 3.0, f"premier rendu trop lent : {elapsed:.3f}s"
    print(
        f"\n[perf] premier rendu de {POINTS} points : {elapsed * 1000:.0f} ms "
        f"({displayed} points effectivement peints)"
    )


def test_zooming_stays_cheap_thanks_to_clip_to_view(big_project, big_spec, qtbot):
    project, _sheet = big_project
    widget = pg.GraphicsLayoutWidget()
    qtbot.addWidget(widget)
    widget.resize(900, 600)
    renderer = PyQtGraphRenderer(widget)
    renderer.render(resolve_spec(project, big_spec))
    plot = renderer.plot_items[0]
    widget.grab()

    start = time.perf_counter()
    for index in range(10):
        low = index * 10.0
        plot.setXRange(low, low + 5.0, padding=0)
        widget.grab()
    elapsed = time.perf_counter() - start
    assert elapsed < 3.0, f"zoom trop lent : {elapsed:.3f}s pour 10 recadrages"
    print(f"\n[perf] 10 zooms sur {POINTS} points : {elapsed * 1000:.0f} ms")


def test_data_update_takes_the_fast_path(big_project, big_spec, qtbot):
    """Le chemin emprunté quand une valeur change dans la feuille."""
    project, sheet = big_project
    widget = pg.GraphicsLayoutWidget()
    qtbot.addWidget(widget)
    renderer = PyQtGraphRenderer(widget)
    renderer.render(resolve_spec(project, big_spec))

    sheet.column("V").set_values(0, [3.0])
    start = time.perf_counter()
    assert renderer.update_data(resolve_spec(project, big_spec)) is True
    elapsed = time.perf_counter() - start
    assert elapsed < 1.0, f"mise à jour trop lente : {elapsed:.3f}s"
    print(f"\n[perf] mise à jour de {POINTS} points : {elapsed * 1000:.0f} ms")


def test_the_only_o_n_computation_is_the_histogram(big_project):
    """Le chiffre qui justifie le seuil de bascule sur `QThread`."""
    _project, sheet = big_project
    values = np.asarray(sheet.column("V").values)
    start = time.perf_counter()
    edges, counts = histogram(values, 100)
    elapsed = time.perf_counter() - start
    assert counts.sum() == POINTS
    assert len(edges) == 101
    assert elapsed < 1.0, f"binning trop lent : {elapsed:.3f}s"
    print(f"\n[perf] histogramme de {POINTS} points : {elapsed * 1000:.0f} ms")


def test_a_million_point_histogram_crosses_the_async_threshold(big_project, qtbot):
    """Au-delà du seuil, `GraphWindow` doit passer par le fil, pas l'UI."""
    project, sheet = big_project
    spec = edit.add_series(
        PlotSpec.empty(),
        SeriesSpec(y_key=sheet.column("V").key, plot_type=PlotType.HISTOGRAM),
    )
    from plotyx.ui.graph_window import GraphWindow

    graph = project.add_graph("PerfGraph")
    graph.set_plot_spec(spec)
    from plotyx.commands import CommandBus

    window = GraphWindow(graph, project, CommandBus(project))
    qtbot.addWidget(window)
    assert POINTS > ASYNC_POINT_THRESHOLD
    assert window._needs_async(resolve_spec(project, spec))
    qtbot.waitUntil(lambda: bool(window.renderer.curves), timeout=10_000)
    window.disconnect_signals()
    project.root.remove_child(graph)


def test_prepare_of_a_million_points_reports_progress(big_project):
    project, sheet = big_project
    spec = edit.add_series(
        PlotSpec.empty(),
        SeriesSpec(y_key=sheet.column("V").key, plot_type=PlotType.HISTOGRAM),
    )
    steps: list[float] = []
    start = time.perf_counter()
    resolved = prepare(project, spec, progress=steps.append)
    elapsed = time.perf_counter() - start
    assert steps == [1.0]
    assert resolved.panels[0].series[0].histogram is not None
    print(f"\n[perf] prepare() sur {POINTS} points : {elapsed * 1000:.0f} ms")


# -- le rendu par défaut : Matplotlib (décision 2026-09-07) --------------------------


def test_first_render_of_a_million_points_on_matplotlib(big_project, big_spec, qtbot):
    """Le chiffre qui a fait basculer le défaut, remis sous test.

    Matplotlib n'a pas de `downsample` explicite : ce qui tient le coût est
    `path.simplify`, actif par défaut, qui élimine au tracé les points
    indiscernables à la résolution du canevas. Le test vérifie qu'il est
    bien armé — le désactiver quelque part ferait exploser ce temps sans
    qu'aucun autre test ne s'en aperçoive.
    """
    import matplotlib

    project, _sheet = big_project
    canvas = new_canvas()
    qtbot.addWidget(canvas)
    canvas.resize(900, 600)
    renderer = MatplotlibRenderer(canvas)

    assert matplotlib.rcParams["path.simplify"] is True

    start = time.perf_counter()
    renderer.render(resolve_spec(project, big_spec))
    canvas.draw()  # force la peinture, sinon on ne mesure que la construction
    elapsed = time.perf_counter() - start

    curve = next(iter(renderer.curves.values()))
    # Aucune copie : l'artiste porte le million de points d'origine, c'est
    # la rastérisation qui en peint moins.
    assert curve.get_ydata().size == POINTS
    assert elapsed < 5.0, f"premier rendu Matplotlib trop lent : {elapsed:.3f}s"
    print(f"\n[perf] premier rendu Matplotlib de {POINTS} points : {elapsed * 1000:.0f} ms")


def test_the_blitted_crosshair_is_far_cheaper_than_a_full_redraw(big_project, big_spec, qtbot):
    """La justification du blit, mesurée plutôt que postulée.

    C'est le point où le mode de rendu de Matplotlib (tout redessiner à
    chaque interaction) coûterait vraiment : sans blit, chaque mouvement de
    souris repeindrait la courbe entière. Le rapport mesuré ici est le seul
    garde-fou contre un retour silencieux au `draw_idle`.
    """
    project, _sheet = big_project
    canvas = new_canvas()
    qtbot.addWidget(canvas)
    canvas.resize(900, 600)
    renderer = MatplotlibRenderer(canvas)
    renderer.render(resolve_spec(project, big_spec))
    renderer.set_crosshair(True)

    start = time.perf_counter()
    canvas.draw()
    full_draw = time.perf_counter() - start

    renderer.move_crosshair(0, 1.0, 0.0)  # amorce la capture du fond
    start = time.perf_counter()
    for index in range(20):
        renderer.move_crosshair(0, float(index), 0.0)
    per_move = (time.perf_counter() - start) / 20

    assert per_move < full_draw / 5, (
        f"réticule non blitté ? {per_move * 1000:.1f} ms par mouvement "
        f"contre {full_draw * 1000:.1f} ms pour un dessin complet"
    )
    print(
        f"\n[perf] réticule blitté sur {POINTS} points : {per_move * 1000:.1f} ms "
        f"par mouvement, contre {full_draw * 1000:.0f} ms pour un dessin complet"
    )
