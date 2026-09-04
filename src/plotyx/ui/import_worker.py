"""`ImportWorker` : parsing d'un ou plusieurs fichiers hors du thread UI.

Premier vrai usage du motif « `QThread` + progression + annulation »
(point de vigilance #2 de l'architecture) — il sera repris tel quel en
phase 8 pour les calculs longs. La logique de parsing elle-même
(`plotyx.io`) ne connaît pas Qt : ce fil ne fait qu'appeler le registre
avec des callables `progress`/`should_cancel` en option, et relayer le
résultat par signal.

Annulation : `cancel()` positionne un drapeau lu par `_should_cancel()`,
consulté par le parseur *entre les chunks* (voir `CsvParser._read_dataframe`)
et jamais au milieu d'une écriture — l'annulation ne laisse donc jamais de
`ImportResult` partiel appliqué au modèle, elle est vérifiée avant toute
poussée de commande, dans `ImportWizardDialog`.
"""

from __future__ import annotations

from pathlib import Path

from PySide6.QtCore import QThread, Signal

from plotyx.io.parser import ImportCancelled, ImportOptions
from plotyx.io.registry import ParserRegistry
from plotyx.io.result import ImportResult

__all__ = ["ImportWorker"]


class ImportWorker(QThread):
    """Parse une liste de fichiers en arrière-plan, avec progression globale."""

    #: Fraction `[0, 1]` de la progression globale (tous fichiers confondus).
    progress = Signal(float)
    #: Un fichier a été parsé (succès ou échec structuré) : `(chemin, ImportResult)`.
    file_finished = Signal(str, object)
    #: Un fichier a levé une exception *fatale* (pas un warning structuré) : `(chemin, message)`.
    failed = Signal(str, str)
    #: Tous les fichiers ont été traités (annulation ou échecs individuels compris).
    finished_all = Signal()
    #: L'utilisateur a annulé avant la fin.
    cancelled = Signal()

    def __init__(
        self,
        registry: ParserRegistry,
        paths: list[Path],
        options: ImportOptions | None = None,
        parent=None,
    ) -> None:
        super().__init__(parent)
        self._registry = registry
        self._paths = list(paths)
        self._base_options = dict(options or {})
        self._cancel_requested = False

    def cancel(self) -> None:
        self._cancel_requested = True

    def _should_cancel(self) -> bool:
        return self._cancel_requested

    def run(self) -> None:  # noqa: N802 - API Qt
        total = len(self._paths) or 1
        for index, path in enumerate(self._paths):
            if self._cancel_requested:
                self.cancelled.emit()
                return
            base_fraction = index / total
            options = dict(self._base_options)
            options["should_cancel"] = self._should_cancel

            def _file_progress(fraction: float, base: float = base_fraction) -> None:
                self.progress.emit(min(base + fraction / total, 1.0))

            options["progress"] = _file_progress
            try:
                result: ImportResult = self._registry.parse(path, options)
            except ImportCancelled:
                self.cancelled.emit()
                return
            except Exception as error:  # noqa: BLE001 - relayé, jamais fatal pour les autres fichiers
                self.failed.emit(str(path), str(error))
                continue
            self.file_finished.emit(str(path), result)
        self.progress.emit(1.0)
        self.finished_all.emit()
