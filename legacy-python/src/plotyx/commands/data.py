"""Commandes de mutation des **valeurs** et des métadonnées d'une colonne.

Stratégie mémoire, dans l'ordre de préférence :

1. **Paramètres seuls** — `InsertRowsCommand` n'a besoin que de `(at, count)`
   pour s'annuler : zéro octet de donnée retenu.
2. **Delta** — `SetCellsCommand` ne conserve que les cellules réellement
   écrasées (`ValuesDelta.previous`), jamais la colonne. Coller 10 valeurs
   dans une colonne d'un million de points coûte 10 flottants d'historique.
3. **Snapshot** — une seule commande y a recours,
   :class:`SetColumnTypeCommand`, et c'est documenté sur place : la
   conversion de type est destructrice (`"abc"` → `NaN`), aucun delta ne
   permet de la reconstruire.

Copy-on-write : les tableaux non touchés ne sont jamais copiés. `Column`
travaille par `np.concatenate` sur des tranches, ce qui laisse intact le
tableau d'origine tant que personne ne l'a modifié ; les commandes ne
copient que le bloc écrit (une fois, à la construction) et le bloc écrasé.
"""

from __future__ import annotations

from collections.abc import Sequence
from typing import Any

import numpy as np

from plotyx.commands.base import ModelCommand
from plotyx.core.column import Column, ColumnType, ValuesDelta

__all__ = [
    "DeleteRowsCommand",
    "InsertRowsCommand",
    "SetCellsCommand",
    "SetColumnLengthCommand",
    "SetColumnMetadataCommand",
    "SetColumnTypeCommand",
]


class SetCellsCommand(ModelCommand):
    """Écrit un bloc de valeurs dans une colonne, à partir de `start`.

    Annulation par delta : on restaure la longueur précédente, puis on
    réécrit les seules cellules qui avaient été écrasées.
    """

    def __init__(
        self,
        column: Column,
        start: int,
        values: Sequence[Any] | np.ndarray,
        text: str = "",
    ) -> None:
        super().__init__(text or f"Saisie dans {column.name}")
        if start < 0:
            raise IndexError(f"indice de ligne négatif : {start}")
        self._column = column
        self._start = start
        # Copie unique du bloc écrit : la commande doit pouvoir se rejouer
        # même si l'appelant réutilise son tableau.
        self._values = np.array(values, copy=True)
        self._delta: ValuesDelta | None = None

    @property
    def column(self) -> Column:
        return self._column

    def _apply(self) -> None:
        self._delta = self._column.set_values(self._start, self._values)

    def _revert(self) -> None:
        delta = self._delta
        if len(self._column) != delta.previous_length:
            self._column.set_length(delta.previous_length)
        if delta.previous.size:
            self._column.set_values(delta.start, delta.previous)


class SetColumnLengthCommand(ModelCommand):
    """Tronque ou complète une colonne. L'annulation restaure la queue ôtée."""

    def __init__(self, column: Column, length: int, text: str = "") -> None:
        super().__init__(text or f"Longueur de {column.name}")
        if length < 0:
            raise ValueError(f"longueur négative : {length}")
        self._column = column
        self._length = length
        self._delta: ValuesDelta | None = None

    def _apply(self) -> None:
        self._delta = self._column.set_length(self._length)

    def _revert(self) -> None:
        delta = self._delta
        self._column.set_length(delta.previous_length)
        if delta.previous.size:
            self._column.set_values(delta.start, delta.previous)


class InsertRowsCommand(ModelCommand):
    """Insère des lignes manquantes dans une colonne. Annulation : paramètres seuls."""

    def __init__(self, column: Column, at: int, count: int = 1, text: str = "") -> None:
        super().__init__(text or f"Insertion de lignes dans {column.name}")
        if count < 0:
            raise ValueError(f"nombre de lignes négatif : {count}")
        self._column = column
        self._at = at if at >= 0 else at + len(column)
        self._count = count

    def _apply(self) -> None:
        self._column.insert_rows(self._at, self._count)

    def _revert(self) -> None:
        self._column.delete_rows(self._at, self._count)


class DeleteRowsCommand(ModelCommand):
    """Supprime des lignes d'une colonne. Delta = les seules valeurs ôtées."""

    def __init__(self, column: Column, at: int, count: int = 1, text: str = "") -> None:
        super().__init__(text or f"Suppression de lignes dans {column.name}")
        if count < 0:
            raise ValueError(f"nombre de lignes négatif : {count}")
        self._column = column
        self._at = at if at >= 0 else at + len(column)
        self._count = count
        self._removed: np.ndarray | None = None

    def _apply(self) -> None:
        self._removed = self._column.delete_rows(self._at, self._count)

    def _revert(self) -> None:
        removed = self._removed
        self._column.insert_rows(self._at, int(removed.size))
        if removed.size:
            self._column.set_values(self._at, removed)


class SetColumnMetadataCommand(ModelCommand):
    """Écrit des métadonnées (`unit`, `long_name`, `comment`, `display_format`, `role`)."""

    def __init__(self, column: Column, text: str = "", **fields: Any) -> None:
        super().__init__(text or f"Propriétés de {column.name}")
        if not fields:
            raise ValueError("aucune métadonnée à écrire")
        self._column = column
        self._fields = dict(fields)
        self._previous: dict[str, Any] = {}

    def _apply(self) -> None:
        self._previous = self._column.set_metadata(**self._fields)

    def _revert(self) -> None:
        self._column.set_metadata(**self._previous)


class SetColumnTypeCommand(ModelCommand):
    """Change le type d'une colonne — **la seule commande à snapshot**.

    La conversion est destructrice dans les deux sens : `float` → `text`
    perd la représentation exacte, `text` → `float` remplace tout ce qui ne
    se parse pas par `NaN`. Aucun delta ne permet de la reconstruire, il
    faut donc conserver le tableau précédent en entier.

    C'est le « dernier recours documenté » de la stratégie mémoire de la
    roadmap. Conséquence pratique : sur un très gros jeu de données, une
    conversion de type double temporairement l'empreinte mémoire de la
    colonne, jusqu'à ce que la commande sorte de la pile (`setUndoLimit`).
    """

    def __init__(self, column: Column, column_type: ColumnType, text: str = "") -> None:
        super().__init__(text or f"Type de {column.name}")
        self._column = column
        self._type = ColumnType(column_type)
        self._previous_type: ColumnType | None = None
        self._previous_values: np.ndarray | None = None

    def _apply(self) -> None:
        self._previous_type, self._previous_values = self._column.set_column_type(self._type)

    def _revert(self) -> None:
        self._column.set_column_type(self._previous_type)
        self._column.set_values(0, self._previous_values)

    def release(self) -> None:
        super().release()
        self._previous_values = None  # le snapshot est le poste mémoire, on le lâche
