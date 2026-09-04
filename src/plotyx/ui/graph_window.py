"""`GraphWindow` : la sous-fenêtre MDI d'un graphe.

Elle assemble trois choses qui, elles, sont testables séparément :

* un `Graph` du modèle, dont le contenu est un `PlotSpec` (phase 6) ;
* `plotyx.plot.resolve`, qui transforme les clés de registre du spec en
  colonnes réelles — sans copie ;
* `plotyx.ui.plot_renderer`, qui peint le tout avec PyQtGraph.

## Mise à jour automatique

C'est la contrepartie du « modèle de série = référence, jamais une copie »
de la roadmap : rien n'est à synchroniser à la main. La fenêtre s'abonne
**une seule fois**, à la racine du projet, aux deux signaux Python purs de
`core` — les notifications y remontent depuis n'importe quel descendant
(voir `core/node.py`). Une saisie dans la feuille, un import, un `undo`
repassent donc par ici sans que rien ne les ait câblés explicitement.

Deux chemins, comme dans le renderer :

* **donnée changée** → `update_data`, qui ne fait que repasser les
  tableaux aux courbes existantes ;
* **structure ou spec changés** → rendu complet.

Les notifications sont **coalescées** par un `QTimer` à 0 ms : une macro
d'import qui ajoute cinquante colonnes ne provoque qu'un seul rendu, à la
fin, quand la boucle d'événements reprend la main.

## Références rompues

Une série dont la colonne a été supprimée n'est ni retirée du spec, ni
tracée : un bandeau la nomme en haut de la fenêtre. C'est le seul
traitement qui reste correct dans les deux sens — le registre est en
références faibles, un `undo` de la suppression réinscrit la colonne sous
la même clé et le graphe se répare de lui-même au rendu suivant.

## Wayland

Cette fenêtre est une sous-fenêtre MDI, jamais un `QWidget` de premier
niveau. Le détachement d'un graphe sur un second écran (annoncé au §2 de
l'architecture) se heurte à la même limite Wayland que les docks flottants
et n'est **pas** implémenté ici — voir le rapport de phase.
"""

from __future__ import annotations

from typing import Any

import pyqtgraph as pg
from PySide6.QtCore import QTimer
from PySide6.QtWidgets import (
    QCheckBox,
    QHBoxLayout,
    QLabel,
    QPushButton,
    QVBoxLayout,
    QWidget,
)

from plotyx.commands import CommandBus
from plotyx.core.node import DataChanged, StructureChanged
from plotyx.core.nodes import Graph
from plotyx.core.project import Project
from plotyx.plot.resolve import ResolvedPlot, resolve_spec
from plotyx.plot.spec import PlotSpec, PlotType
from plotyx.ui.plot_renderer import PyQtGraphRenderer
from plotyx.ui.plot_worker import ASYNC_POINT_THRESHOLD, PlotPrepareWorker

__all__ = ["GraphWindow"]

#: Kinds de `StructureChanged` qui n'affectent pas un graphe. Tout le reste
#: déclenche un rendu complet — être trop large ici ne coûte qu'un rendu de
#: plus, être trop étroit laisserait un graphe périmé à l'écran.
_IGNORED_KINDS = frozenset({"rows_inserted", "rows_deleted"})


class GraphWindow(QWidget):
    """Widget de graphe, destiné à `QMdiArea.addSubWindow`."""

    def __init__(
        self,
        graph: Graph,
        project: Project,
        bus: CommandBus,
        parent: QWidget | None = None,
    ) -> None:
        super().__init__(parent)
        self.setObjectName(f"graphWindow_{graph.key}")
        self._graph = graph
        self._project = project
        self._bus = bus
        self._worker: PlotPrepareWorker | None = None
        self._pending_full_render = True

        self._plot_widget = pg.GraphicsLayoutWidget()
        self._plot_widget.setObjectName("graphPlotWidget")
        self._renderer = PyQtGraphRenderer(self._plot_widget)

        self._banner = QLabel("")
        self._banner.setObjectName("graphBanner")
        self._banner.setWordWrap(True)
        self._banner.hide()

        self._coords = QLabel("")
        self._coords.setObjectName("graphCoords")
        self._crosshair_box = QCheckBox("Curseurs")
        self._crosshair_box.setObjectName("graphCrosshairBox")
        self._crosshair_box.toggled.connect(self._on_crosshair_toggled)
        auto_button = QPushButton("Ajuster")
        auto_button.setObjectName("graphAutoRangeButton")
        auto_button.setToolTip("Recadrer tous les panneaux sur les données")
        auto_button.clicked.connect(self.auto_range)

        toolbar = QHBoxLayout()
        toolbar.setContentsMargins(4, 2, 4, 2)
        toolbar.addWidget(self._crosshair_box)
        toolbar.addWidget(auto_button)
        toolbar.addStretch(1)
        toolbar.addWidget(self._coords)

        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.addLayout(toolbar)
        layout.addWidget(self._banner)
        layout.addWidget(self._plot_widget, 1)

        self._plot_widget.scene().sigMouseMoved.connect(self._on_mouse_moved)

        self._refresh_timer = QTimer(self)
        self._refresh_timer.setSingleShot(True)
        self._refresh_timer.setInterval(0)
        self._refresh_timer.timeout.connect(self._refresh_now)

        self._data_conn = project.data_changed.connect(self._on_data_changed)
        self._structure_conn = project.structure_changed.connect(self._on_structure_changed)

        # Rendu synchrone à la construction : la fenêtre doit être correcte
        # dès qu'elle est retournée, sans dépendre d'un tour de boucle
        # d'événements (l'ouverture d'un graphe n'est jamais dans un chemin
        # chaud, contrairement aux rafraîchissements suivants).
        self.refresh_now(full=True)

    # -- accès ----------------------------------------------------------------

    @property
    def graph(self) -> Graph:
        return self._graph

    @property
    def bus(self) -> CommandBus:
        return self._bus

    @property
    def project(self) -> Project:
        return self._project

    @property
    def renderer(self) -> PyQtGraphRenderer:
        return self._renderer

    @property
    def plot_widget(self) -> pg.GraphicsLayoutWidget:
        return self._plot_widget

    @property
    def banner_text(self) -> str:
        """Texte du bandeau d'avertissement (vide s'il est masqué).

        `isHidden()` et non `isVisible()` : une sous-fenêtre MDI jamais
        affichée (le cas d'un test headless) a tous ses enfants
        `isVisible() == False`, ce qui rendrait ce témoin inutilisable.
        `isHidden()` ne parle, lui, que du masquage explicite.
        """
        return "" if self._banner.isHidden() else self._banner.text()

    def spec(self) -> PlotSpec:
        """Le spec courant du graphe, ou un spec vide s'il n'en a pas."""
        spec = self._graph.plot_spec
        return spec if isinstance(spec, PlotSpec) else PlotSpec.empty(self._graph.name)

    # -- cycle de vie ----------------------------------------------------------

    def disconnect_signals(self) -> None:
        """À appeler à la fermeture : plus aucune notification ne doit arriver."""
        self._refresh_timer.stop()
        self._data_conn.disconnect()
        self._structure_conn.disconnect()
        self._stop_worker()

    def _stop_worker(self) -> None:
        worker, self._worker = self._worker, None
        if worker is not None and worker.isRunning():
            worker.cancel()
            worker.wait(2000)

    # -- rafraîchissement -------------------------------------------------------

    def refresh(self, *, full: bool = False) -> None:
        """Programme un rafraîchissement (coalescé sur la boucle d'événements)."""
        self._pending_full_render = self._pending_full_render or full
        self._refresh_timer.start()

    def refresh_now(self, *, full: bool = False) -> None:
        """Rafraîchit immédiatement — utilisé par les tests et à l'ouverture."""
        self._pending_full_render = self._pending_full_render or full
        self._refresh_timer.stop()
        self._refresh_now()

    def _refresh_now(self) -> None:
        spec = self.spec()
        resolved = resolve_spec(self._project, spec)
        if self._needs_async(resolved):
            self._start_worker(spec)
            return
        self._paint(resolved, allow_fast_path=not self._pending_full_render)

    def _needs_async(self, resolved: ResolvedPlot) -> bool:
        """Vrai si la préparation mérite un fil (voir `plotyx.ui.plot_worker`)."""
        points = sum(
            series.point_count
            for panel in resolved.panels
            for series in panel.series
            if series.ok and series.spec.plot_type is PlotType.HISTOGRAM
        )
        return points > ASYNC_POINT_THRESHOLD

    def _start_worker(self, spec: PlotSpec) -> None:
        self._stop_worker()
        worker = PlotPrepareWorker(self._project, spec, self)
        worker.ready.connect(self._on_worker_ready)
        worker.failed.connect(self._on_worker_failed)
        self._worker = worker
        worker.start()

    def _on_worker_ready(self, resolved: object) -> None:
        self._worker = None
        if isinstance(resolved, ResolvedPlot):
            self._paint(resolved, allow_fast_path=False)

    def _on_worker_failed(self, message: str) -> None:
        self._worker = None
        self._show_banner(f"Préparation du graphe impossible : {message}")

    def _paint(self, resolved: ResolvedPlot, *, allow_fast_path: bool) -> None:
        if not (allow_fast_path and self._renderer.update_data(resolved)):
            self._renderer.render(resolved)
        self._pending_full_render = False
        self._update_banner(resolved)

    def _update_banner(self, resolved: ResolvedPlot) -> None:
        broken = resolved.broken_series
        if not broken:
            self._banner.hide()
            self._banner.setText("")
            return
        names = ", ".join(
            series.spec.name or f"série {position + 1}"
            for position, series in enumerate(broken)
        )
        self._show_banner(
            f"{len(broken)} série(s) sans données : {names}. "
            "La ou les colonnes référencées ont été supprimées ; annuler la "
            "suppression (Ctrl+Z) rebranche la série."
        )

    def _show_banner(self, text: str) -> None:
        self._banner.setText(text)
        self._banner.show()

    # -- notifications du modèle -------------------------------------------------

    def _on_data_changed(self, event: DataChanged) -> None:
        """Une valeur a changé : chemin rapide, sauf si la longueur bouge."""
        if event.length_changed:
            self.refresh(full=True)
        else:
            self.refresh()

    def _on_structure_changed(self, event: StructureChanged) -> None:
        if event.kind in _IGNORED_KINDS:
            self.refresh()
            return
        self.refresh(full=True)

    # -- interactions --------------------------------------------------------------

    def auto_range(self) -> None:
        """Recadre tous les panneaux sur leurs données (zoom/pan remis à zéro)."""
        for plot in self._renderer.plot_items:
            plot.getViewBox().autoRange()

    def _on_crosshair_toggled(self, enabled: bool) -> None:
        self._renderer.set_crosshair(enabled)
        if not enabled:
            self._coords.setText("")

    def _on_mouse_moved(self, position: Any) -> None:
        """Suit la souris : réticule et lecture des coordonnées."""
        index = self._renderer.panel_at(position)
        if index is None:
            return
        x, y = self._renderer.data_coords(index, position)
        self._renderer.move_crosshair(index, x, y)
        self._coords.setText(f"x = {x:.6g}    y = {y:.6g}")

    # -- Qt ------------------------------------------------------------------------

    def closeEvent(self, event) -> None:  # noqa: N802 - API Qt
        self.disconnect_signals()
        super().closeEvent(event)
