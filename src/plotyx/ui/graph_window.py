"""`GraphWindow` : la sous-fenêtre MDI d'un graphe.

Elle assemble trois choses qui, elles, sont testables séparément :

* un `Graph` du modèle, dont le contenu est un `PlotSpec` (phase 6) ;
* `plotyx.plot.resolve`, qui transforme les clés de registre du spec en
  colonnes réelles — sans copie ;
* un renderer, qui peint le tout — Matplotlib par défaut, PyQtGraph en
  visualiseur léger (voir `plotyx.ui.renderers`).

## Deux moteurs, une fenêtre

Depuis la décision du 2026-09-07, cette fenêtre ne connaît plus aucun
moteur : elle demande un renderer pour un `RenderMode` et ne parle qu'au
protocole `PlotRenderer`. Tout ce que la bascule doit faire tient donc dans
`set_render_mode` — détacher l'ancien renderer, en construire un autre,
redessiner. Le mode est un état de fenêtre, jamais un attribut du graphe :
rien n'en est écrit dans le `.plx`.

Le suivi de la souris (coordonnées, réticule) est **dans le renderer**, pas
ici : une scène `QGraphicsView` et un canevas Matplotlib ne le signalent pas
de la même façon, et la fenêtre n'a pas à trancher entre les deux. Elle
reçoit le résultat par `set_coords_callback`.

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

## Le bandeau

Un seul bandeau, deux motifs de l'afficher, cumulables :

* **références rompues** — une série dont la colonne a été supprimée n'est
  ni retirée du spec, ni tracée : le bandeau la nomme. C'est le seul
  traitement correct dans les deux sens — le registre est en références
  faibles, un `undo` de la suppression réinscrit la colonne sous la même
  clé et le graphe se répare de lui-même au rendu suivant ;
* **visualiseur léger** — ce mode n'honore pas tout ce que le panneau de
  propriétés expose. Les contrôles y restent volontairement **actifs** :
  ils éditent le `PlotSpec`, jamais un renderer, donc l'édition est bien
  enregistrée, annulable et persistée, et devient visible au retour en vue
  normale. Les griser aurait fait dépendre le panneau du moteur actif — ce
  que l'architecture évite partout ailleurs — et interdirait de préparer
  une mise en forme pendant une acquisition. Le bandeau est là pour que
  « pas encore visible » ne se lise pas « perdu ».

## Wayland

Cette fenêtre est une sous-fenêtre MDI, jamais un `QWidget` de premier
niveau. Le détachement d'un graphe sur un second écran (annoncé au §2 de
l'architecture) se heurte à la même limite Wayland que les docks flottants
et n'est **pas** implémenté ici — voir le rapport de phase.
"""

from __future__ import annotations

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
from plotyx.ui.plot_worker import ASYNC_POINT_THRESHOLD, PlotPrepareWorker
from plotyx.ui.renderers import DEFAULT_RENDER_MODE, PlotRenderer, RenderMode, create_renderer

__all__ = ["GraphWindow"]

#: Texte du bandeau en visualiseur léger. Il dit les deux choses qui
#: comptent : ce qui manque, et que rien n'est perdu.
_DIRECT_VIEW_NOTICE = (
    "Visualiseur léger (vue directe) : le rendu ignore une partie de la mise "
    "en forme. Les réglages du panneau Propriétés restent enregistrés et "
    "s'appliquent au retour en vue normale."
)

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
        mode: RenderMode = DEFAULT_RENDER_MODE,
    ) -> None:
        super().__init__(parent)
        self.setObjectName(f"graphWindow_{graph.key}")
        self._graph = graph
        self._project = project
        self._bus = bus
        self._worker: PlotPrepareWorker | None = None
        self._pending_full_render = True
        self._mode = RenderMode(mode)
        self._renderer: PlotRenderer = create_renderer(self._mode)
        self._renderer.set_coords_callback(self._on_coords)

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
        layout.addWidget(self._renderer.widget, 1)
        self._layout = layout

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
    def renderer(self) -> PlotRenderer:
        return self._renderer

    @property
    def plot_widget(self) -> QWidget:
        return self._renderer.widget

    @property
    def render_mode(self) -> RenderMode:
        return self._mode

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
        self._renderer.detach()

    # -- moteur de rendu ---------------------------------------------------------

    def set_render_mode(self, mode: RenderMode) -> None:
        """Change de moteur. Le graphe, lui, ne bouge pas d'un octet.

        Tout ce qui décrit le tracé est dans le `PlotSpec` : basculer ne
        fait donc que jeter des artistes et en repeindre d'autres à partir
        de la même description. Rien n'est à sauvegarder avant, rien n'est
        perdu après, et aucune commande n'est poussée sur le bus — un
        changement de vue n'est pas une modification du projet, il n'a donc
        pas à s'annuler ni à marquer le projet modifié.
        """
        mode = RenderMode(mode)
        if mode is self._mode:
            return
        self._stop_worker()
        old = self._renderer
        crosshair = old.crosshair_enabled
        self._layout.removeWidget(old.widget)
        old.detach()
        old.widget.setParent(None)
        old.widget.deleteLater()

        self._mode = mode
        self._renderer = create_renderer(mode)
        self._renderer.set_coords_callback(self._on_coords)
        self._layout.addWidget(self._renderer.widget, 1)
        self._coords.setText("")
        self.refresh_now(full=True)
        # Après le rendu : le réticule de PyQtGraph a besoin de ses panneaux.
        self._renderer.set_crosshair(crosshair)

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
        """Compose le bandeau : avertissement de mode, puis séries rompues.

        Les deux motifs se cumulent — être en vue directe ne dispense pas de
        signaler une référence cassée, et l'inverse encore moins.
        """
        messages: list[str] = []
        if not self._renderer.supports_full_styling:
            messages.append(_DIRECT_VIEW_NOTICE)

        broken = resolved.broken_series
        if broken:
            names = ", ".join(
                series.spec.name or f"série {position + 1}"
                for position, series in enumerate(broken)
            )
            messages.append(
                f"{len(broken)} série(s) sans données : {names}. "
                "La ou les colonnes référencées ont été supprimées ; annuler la "
                "suppression (Ctrl+Z) rebranche la série."
            )

        if not messages:
            self._banner.hide()
            self._banner.setText("")
            return
        self._show_banner("\n".join(messages))

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
        self._renderer.auto_range()

    def _on_crosshair_toggled(self, enabled: bool) -> None:
        self._renderer.set_crosshair(enabled)
        if not enabled:
            self._coords.setText("")

    def _on_coords(self, position: tuple[float, float] | None) -> None:
        """Affiche les coordonnées sous le curseur, rapportées par le renderer."""
        if position is None:
            self._coords.setText("")
            return
        x, y = position
        self._coords.setText(f"x = {x:.6g}    y = {y:.6g}")

    # -- Qt ------------------------------------------------------------------------

    def closeEvent(self, event) -> None:  # noqa: N802 - API Qt
        self.disconnect_signals()
        super().closeEvent(event)
