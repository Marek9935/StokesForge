"""`Worksheet` : une liste ordonnée de colonnes de longueurs libres.

Contrairement à un `DataFrame`, une feuille PlotYx n'est **pas
rectangulaire** : chaque colonne porte sa propre longueur, exactement comme
dans Origin. Le nombre de lignes affiché (`row_count`) est le maximum des
longueurs ; lire une cellule au-delà d'une colonne courte rend la valeur
manquante du type de cette colonne, sans erreur et sans padding stocké.

Toutes les mutations passent par le jeu de primitives de cette classe et de
`Column` — c'est ce jeu que la phase 2 enveloppera dans des `QUndoCommand`.
"""

from __future__ import annotations

from collections.abc import Iterator, Sequence
from typing import Any

import numpy as np

from plotyx.core.column import Column, ColumnRole, ColumnType, ValuesDelta
from plotyx.core.errors import DuplicateNameError, StructureError
from plotyx.core.node import ModelObject, StructureChanged, attach, check_name, detach

#: Une colonne se désigne par son nom, son index, ou l'objet lui-même.
ColumnRef = str | int | Column

__all__ = ["Column", "ColumnRef", "ColumnRole", "ColumnType", "Worksheet"]


class Worksheet(ModelObject):
    """Feuille de calcul colonnaire."""

    type_name = "worksheet"

    def __init__(self, name: str = "Sheet1", columns: Sequence[Column] | None = None) -> None:
        super().__init__(name)
        self._columns: list[Column] = []
        for column in columns or ():
            self.insert_column(len(self._columns), column)

    # -- lecture ----------------------------------------------------------

    @property
    def columns(self) -> tuple[Column, ...]:
        return tuple(self._columns)

    def children(self) -> tuple[ModelObject, ...]:
        return tuple(self._columns)

    def __len__(self) -> int:
        return len(self._columns)

    def __iter__(self) -> Iterator[Column]:
        return iter(self._columns)

    def __contains__(self, ref: Any) -> bool:
        try:
            self.column(ref)
        except (KeyError, IndexError):
            return False
        return True

    @property
    def column_count(self) -> int:
        return len(self._columns)

    @property
    def row_count(self) -> int:
        """Nombre de lignes de la feuille : la plus longue de ses colonnes."""
        return max((len(column) for column in self._columns), default=0)

    def column_names(self) -> tuple[str, ...]:
        return tuple(column.name for column in self._columns)

    def column(self, ref: ColumnRef) -> Column:
        """Résout une référence de colonne (nom, index, ou objet)."""
        if isinstance(ref, Column):
            if ref not in self._columns:
                raise KeyError(f"colonne {ref.name!r} absente de la feuille {self.name!r}")
            return ref
        if isinstance(ref, int):
            return self._columns[ref]
        for column in self._columns:
            if column.name == ref:
                return column
        raise KeyError(f"aucune colonne nommée {ref!r} dans la feuille {self.name!r}")

    def index_of(self, ref: ColumnRef) -> int:
        return self._columns.index(self.column(ref))

    def get_cell(self, ref: ColumnRef, row: int) -> Any:
        """Valeur d'une cellule ; valeur manquante au-delà de la colonne."""
        return self.column(ref).get(row)

    def to_arrays(self) -> dict[str, np.ndarray]:
        """Copie des colonnes par nom — pour les tests et l'export, pas la vue."""
        return {column.name: np.array(column.values) for column in self._columns}

    # -- mutations primitives : structure ---------------------------------

    def insert_column(self, index: int, column: Column) -> Column:
        """Insère `column` à `index`. La colonne ne doit pas déjà avoir un parent."""
        if not isinstance(column, Column):
            raise StructureError(f"une feuille ne contient que des colonnes, pas {column!r}")
        if column.parent is not None:
            raise StructureError(
                f"la colonne {column.name!r} appartient déjà à {column.parent!r}"
            )
        self._check_free_name(column.name)
        index = self._check_index(index, allow_end=True)
        self._columns.insert(index, column)
        attach(column, self)
        self._notify_structure(
            StructureChanged(self, "column_inserted", {"index": index, "column": column})
        )
        return column

    def add_column(
        self,
        name: str | None = None,
        values: Sequence[Any] | np.ndarray | None = None,
        **kwargs: Any,
    ) -> Column:
        """Confort : construit une colonne et l'ajoute en fin de feuille.

        Sans nom explicite, en génère un libre façon Origin (`A`, `B`, ...).
        """
        column = Column(name if name is not None else self.next_free_name(), values, **kwargs)
        return self.insert_column(len(self._columns), column)

    def remove_column(self, ref: ColumnRef) -> tuple[int, Column]:
        """Retire une colonne. Retourne `(index, colonne)` pour l'annulation."""
        column = self.column(ref)
        index = self._columns.index(column)
        self._columns.pop(index)
        detach(column)
        self._notify_structure(
            StructureChanged(self, "column_removed", {"index": index, "column": column})
        )
        return index, column

    def move_column(self, ref: ColumnRef, to_index: int) -> int:
        """Déplace une colonne. Retourne son index précédent."""
        column = self.column(ref)
        previous = self._columns.index(column)
        to_index = self._check_index(to_index)
        if to_index != previous:
            self._columns.pop(previous)
            self._columns.insert(to_index, column)
            self._notify_structure(
                StructureChanged(
                    self, "column_moved", {"from_index": previous, "to_index": to_index}
                )
            )
        return previous

    def rename_column(self, ref: ColumnRef, name: str) -> str:
        """Renomme une colonne après contrôle d'unicité. Retourne l'ancien nom."""
        column = self.column(ref)
        check_name(name)
        if name != column.name:
            self._check_free_name(name)
        return column._set_name(name)

    def next_free_name(self) -> str:
        """Prochain nom de colonne libre, façon tableur : A, B, ... Z, AA, ..."""
        taken = set(self.column_names())
        index = 0
        while True:
            name, number = "", index
            while True:
                name = chr(ord("A") + number % 26) + name
                number = number // 26 - 1
                if number < 0:
                    break
            if name not in taken:
                return name
            index += 1

    # -- mutations primitives : données -----------------------------------

    def set_values(
        self, ref: ColumnRef, start: int, values: Sequence[Any] | np.ndarray
    ) -> ValuesDelta:
        """Écrit un bloc de valeurs dans une colonne (délègue à `Column`)."""
        return self.column(ref).set_values(start, values)

    def set_cell(self, ref: ColumnRef, row: int, value: Any) -> ValuesDelta:
        """Écrit une cellule ; étend la colonne si `row` est au-delà de sa fin."""
        return self.column(ref).set_values(row, [value])

    def insert_rows(self, at: int, count: int = 1) -> None:
        """Insère `count` lignes vides dans **toutes** les colonnes."""
        for column in self._columns:
            column.insert_rows(min(at, len(column)), count)
        self._notify_structure(
            StructureChanged(self, "rows_inserted", {"at": at, "count": count})
        )

    def delete_rows(self, at: int, count: int = 1) -> dict[str, np.ndarray]:
        """Supprime `count` lignes de toutes les colonnes. Retourne les valeurs ôtées."""
        removed: dict[str, np.ndarray] = {}
        for column in self._columns:
            if at < len(column):
                removed[column.name] = column.delete_rows(at, count)
        self._notify_structure(
            StructureChanged(self, "rows_deleted", {"at": at, "count": count})
        )
        return removed

    # -- confort ----------------------------------------------------------

    def x_column(self) -> Column | None:
        """Première colonne de rôle X, ou None."""
        for column in self._columns:
            if column.role is ColumnRole.X:
                return column
        return None

    # -- utilitaires ------------------------------------------------------

    def _check_free_name(self, name: str) -> None:
        if any(column.name == name for column in self._columns):
            raise DuplicateNameError(
                f"une colonne nommée {name!r} existe déjà dans la feuille {self.name!r}"
            )

    def _check_index(self, index: int, *, allow_end: bool = False) -> int:
        limit = len(self._columns) + (1 if allow_end else 0)
        if index < 0:
            index += limit
        if not 0 <= index < max(limit, 1):
            raise IndexError(f"index de colonne {index} hors bornes (0..{limit})")
        return index
