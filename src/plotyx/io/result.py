"""Structures de retour du parsing : `ImportResult` et ce qu'il transporte.

Aucune de ces classes ne mute quoi que ce soit — ce sont des paquets de
données passifs. C'est `plotyx.ui.import_apply` qui les traduit en commandes
du bus, dans une macro.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

import numpy as np

from plotyx.core.column import ColumnRole, ColumnType

__all__ = ["ColumnImport", "ImportResult", "ImportWarning"]


@dataclass(frozen=True)
class ImportWarning:
    """Un avertissement structuré, jamais une exception qui plante l'import.

    `code` est une chaîne stable (`"npoints_mismatch"`, `"decimal_guess"`,
    ...) exploitable par l'UI ou un test sans parser `message`.
    """

    code: str
    message: str
    severity: str = "warning"  #: "warning" ou "error" (le fichier reste illisible)


@dataclass
class ColumnImport:
    """Une colonne telle qu'extraite du fichier, pas encore dans le modèle.

    Longueur libre par construction : rien n'oblige les `ColumnImport` d'un
    même `ImportResult`, ni de deux fichiers importés dans la même macro, à
    partager une longueur — c'est le cas d'usage réel des spectres
    hétérogènes (décision de la phase 1).
    """

    name: str
    values: np.ndarray
    column_type: ColumnType = ColumnType.NUMERIC
    role: ColumnRole = ColumnRole.Y
    unit: str = ""
    long_name: str = ""
    comment: str = ""


@dataclass
class ImportResult:
    """Ce que rend `FileParser.parse()`."""

    columns: list[ColumnImport] = field(default_factory=list)
    metadata: dict[str, Any] = field(default_factory=dict)
    warnings: list[ImportWarning] = field(default_factory=list)
    parser_name: str = ""

    @property
    def ok(self) -> bool:
        """Faux si un avertissement de sévérité `"error"` a été émis."""
        return not any(w.severity == "error" for w in self.warnings)
