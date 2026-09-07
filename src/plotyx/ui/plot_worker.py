"""`PlotPrepareWorker` : préparation d'un graphe hors du thread UI.

Point de vigilance #2 de l'architecture. Le motif est **exactement celui
de `plotyx.ui.import_worker`** (phase 5) — `QThread`, un signal de
progression, un drapeau d'annulation consulté par le calcul lui-même — et
c'est délibéré : la roadmap demande « un pattern unique réutilisé partout »,
pas deux mécanismes concurrents. Les noms de signaux (`progress`,
`failed`, `cancelled`) sont repris à l'identique pour que le code appelant
se lise pareil des deux côtés.

## Ce qui est calculé ici, et ce qui ne l'est pas

Presque rien de la chaîne de tracé n'est en O(n) : une série est une
**référence** (`plotyx.plot.resolve`), le tableau part tel quel au
renderer, et c'est le moteur qui décime au moment de peindre — par
`path.simplify` côté Matplotlib, par `downsample`/`clipToView` côté
PyQtGraph. Le seul calcul qui parcourt réellement les données est le
**binning d'un histogramme**, d'où `plotyx.plot.resolve.prepare`. Ce module
est donc resté indifférent au changement de rendu par défaut du
2026-09-07 : il ne connaît aucun moteur.

Mesuré sur cette machine (voir `tests/test_plot_performance.py`) : un
histogramme de 10⁶ points coûte de l'ordre de quelques millisecondes.
`GraphWindow` ne bascule donc sur ce fil qu'au-delà de
:data:`ASYNC_POINT_THRESHOLD` points, et reste synchrone en dessous —
lancer un `QThread` pour 5 ms de calcul coûterait plus cher que le calcul.
Le fil existe pour le jour où la phase 8 branchera un lissage ou un fit sur
la même chaîne : le point d'accroche est là, testé, et il n'y en aura pas
un second à inventer.
"""

from __future__ import annotations

from PySide6.QtCore import QThread, Signal

from plotyx.core.project import Project
from plotyx.plot.resolve import ComputeCancelled, prepare
from plotyx.plot.spec import PlotSpec

__all__ = ["ASYNC_POINT_THRESHOLD", "PlotPrepareWorker"]

#: Au-delà de ce nombre de points *à pré-calculer*, la préparation part sur
#: un fil. Choisi par mesure, pas par principe — voir le docstring.
ASYNC_POINT_THRESHOLD = 200_000


class PlotPrepareWorker(QThread):
    """Résout et pré-calcule un `PlotSpec` en arrière-plan."""

    #: Fraction `[0, 1]` de la progression.
    progress = Signal(float)
    #: Préparation terminée : porte le `ResolvedPlot`.
    ready = Signal(object)
    #: Échec non fatal : le message est affiché, le graphe garde son état.
    failed = Signal(str)
    #: L'utilisateur (ou une fermeture de fenêtre) a annulé.
    cancelled = Signal()

    def __init__(self, project: Project, spec: PlotSpec, parent=None) -> None:
        super().__init__(parent)
        self._project = project
        self._spec = spec
        self._cancel_requested = False

    def cancel(self) -> None:
        self._cancel_requested = True

    def _should_cancel(self) -> bool:
        return self._cancel_requested

    def run(self) -> None:  # noqa: N802 - API Qt
        try:
            resolved = prepare(
                self._project,
                self._spec,
                progress=self.progress.emit,
                should_cancel=self._should_cancel,
            )
        except ComputeCancelled:
            self.cancelled.emit()
            return
        except Exception as error:  # noqa: BLE001 - relayé, jamais fatal pour l'UI
            self.failed.emit(str(error))
            return
        self.progress.emit(1.0)
        self.ready.emit(resolved)
