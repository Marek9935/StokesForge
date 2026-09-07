"""`Column` : un `np.ndarray` plus ses métadonnées.

Deux décisions de la phase 1 sont matérialisées ici :

1. **Valeurs manquantes = `NaN` dans un `float64`**, pas de `numpy.ma`.
   Les tableaux masqués coûtent un masque booléen parallèle et une couche
   d'indirection à chaque opération, pour un service que `NaN` rend déjà en
   usage scientifique (`np.nanmean`, propagation naturelle).
2. **Longueur libre.** Une colonne connaît sa propre longueur ; c'est la
   feuille qui compose des colonnes de longueurs différentes.
"""

from __future__ import annotations

from collections.abc import Sequence
from dataclasses import dataclass
from enum import StrEnum
from typing import Any

import numpy as np

from plotyx.core.node import DataChanged, ModelObject, StructureChanged

__all__ = ["Column", "ColumnRole", "ColumnType", "ValuesDelta"]


class ColumnRole(StrEnum):
    """Rôle de la colonne dans un tracé, façon Origin."""

    X = "X"
    Y = "Y"
    Z = "Z"
    X_ERROR = "xErr"
    Y_ERROR = "yErr"
    LABEL = "Label"
    NONE = "None"


class ColumnType(StrEnum):
    """Type de données stocké.

    `NUMERIC` et `DATETIME` partagent la représentation `float64` (pour
    `DATETIME`, un timestamp POSIX en secondes) ; `TEXT` utilise un tableau
    `object` dont la valeur manquante est `None`.
    """

    NUMERIC = "numeric"
    DATETIME = "datetime"
    TEXT = "text"

    @property
    def dtype(self) -> np.dtype:
        return np.dtype(object) if self is ColumnType.TEXT else np.dtype(np.float64)

    @property
    def missing(self) -> Any:
        return None if self is ColumnType.TEXT else np.nan


@dataclass(frozen=True)
class ValuesDelta:
    """Ce qu'il faut pour annuler un `set_values` sans copier la colonne.

    La phase 2 enveloppe la primitive et rejoue `previous` à `start`, puis
    retaille la colonne à `previous_length`.
    """

    start: int
    previous: np.ndarray
    previous_length: int


def _empty(count: int, column_type: ColumnType) -> np.ndarray:
    """Tableau de `count` valeurs manquantes du type demandé."""
    return np.full(count, column_type.missing, dtype=column_type.dtype)


class Column(ModelObject):
    """Vecteur de données nommé, de longueur libre."""

    type_name = "column"

    def __init__(
        self,
        name: str,
        values: Sequence[Any] | np.ndarray | None = None,
        *,
        column_type: ColumnType = ColumnType.NUMERIC,
        role: ColumnRole = ColumnRole.Y,
        unit: str = "",
        long_name: str = "",
        comment: str = "",
        display_format: str = "",
    ) -> None:
        super().__init__(name)
        self._type = ColumnType(column_type)
        self._values = self._coerce(values if values is not None else [])
        self.role = ColumnRole(role)
        self.unit = unit
        self.long_name = long_name
        self.comment = comment
        self.display_format = display_format

    # -- lecture ----------------------------------------------------------

    @property
    def column_type(self) -> ColumnType:
        return self._type

    @property
    def values(self) -> np.ndarray:
        """Le tableau sous-jacent, en lecture seule.

        La vue et les calculs y accèdent sans copie ; l'écriture passe
        obligatoirement par les primitives de mutation (`WRITEABLE` est
        désarmé pour rendre l'infraction bruyante plutôt que silencieuse).
        """
        view = self._values.view()
        view.flags.writeable = False
        return view

    def __len__(self) -> int:
        return int(self._values.size)

    def __getitem__(self, index: Any) -> Any:
        return self._values[index]

    def get(self, row: int) -> Any:
        """Valeur à la ligne `row`, ou la valeur manquante hors bornes.

        Les lignes hors longueur ne sont pas une erreur : dans une feuille
        aux colonnes hétérogènes, elles sont la règle.
        """
        if row < 0:
            raise IndexError(f"indice de ligne négatif : {row}")
        if row >= self._values.size:
            return self._type.missing
        return self._values[row]

    def is_missing(self, row: int) -> bool:
        value = self.get(row)
        if self._type is ColumnType.TEXT:
            return value is None or value == ""
        return bool(np.isnan(value))

    def numeric_values(self) -> np.ndarray:
        """Vue `float64` des valeurs. Erreur sur une colonne texte."""
        if self._type is ColumnType.TEXT:
            raise TypeError(f"la colonne {self.name!r} est de type texte")
        return self.values

    def _coerce(self, values: Sequence[Any] | np.ndarray) -> np.ndarray:
        """Convertit une séquence quelconque vers la représentation du type."""
        if self._type is ColumnType.TEXT:
            array = np.empty(len(values), dtype=object)
            array[:] = [None if v is None else str(v) for v in values]
            return array
        return np.asarray(values, dtype=np.float64).ravel().copy()

    # -- mutations primitives ---------------------------------------------
    #
    # Jeu fermé, à envelopper tel quel en `QUndoCommand` en phase 2. Chaque
    # primitive retourne le delta nécessaire à son inversion — jamais un
    # snapshot de la colonne entière.

    def set_values(self, start: int, values: Sequence[Any] | np.ndarray) -> ValuesDelta:
        """Écrit `values` à partir de la ligne `start`, en étendant si besoin.

        L'extension remplit d'éventuels trous avec la valeur manquante, ce
        qui reproduit le comportement d'Origin quand on colle des données
        plus bas que la fin de la colonne.
        """
        if start < 0:
            raise IndexError(f"indice de ligne négatif : {start}")
        block = self._coerce(values)
        previous_length = self._values.size
        stop = start + block.size
        overlap = self._values[start : min(stop, previous_length)].copy()
        if stop > previous_length:
            padding = _empty(stop - previous_length, self._type)
            self._values = np.concatenate([self._values, padding])
        self._values[start:stop] = block
        self._notify_data(
            DataChanged(self, start, stop, length_changed=self._values.size != previous_length)
        )
        return ValuesDelta(start, overlap, previous_length)

    def set_length(self, length: int) -> ValuesDelta:
        """Tronque ou complète la colonne à `length` lignes."""
        if length < 0:
            raise ValueError(f"longueur négative : {length}")
        previous_length = self._values.size
        if length == previous_length:
            return ValuesDelta(length, _empty(0, self._type), previous_length)
        if length < previous_length:
            removed = self._values[length:].copy()
            self._values = self._values[:length].copy()
        else:
            removed = _empty(0, self._type)
            padding = _empty(length - previous_length, self._type)
            self._values = np.concatenate([self._values, padding])
        self._notify_data(
            DataChanged(self, min(length, previous_length), max(length, previous_length), True)
        )
        return ValuesDelta(length, removed, previous_length)

    def insert_rows(self, at: int, count: int) -> None:
        """Insère `count` lignes manquantes à partir de `at`."""
        if count < 0:
            raise ValueError(f"nombre de lignes négatif : {count}")
        at = self._check_row(at, allow_end=True)
        self._values = np.concatenate(
            [self._values[:at], _empty(count, self._type), self._values[at:]]
        )
        self._notify_data(DataChanged(self, at, self._values.size, length_changed=count > 0))

    def delete_rows(self, at: int, count: int) -> np.ndarray:
        """Supprime `count` lignes à partir de `at`. Retourne les valeurs ôtées."""
        if count < 0:
            raise ValueError(f"nombre de lignes négatif : {count}")
        at = self._check_row(at, allow_end=True)
        stop = min(at + count, self._values.size)
        removed = self._values[at:stop].copy()
        self._values = np.concatenate([self._values[:at], self._values[stop:]])
        self._notify_data(DataChanged(self, at, self._values.size, length_changed=removed.size > 0))
        return removed

    def set_metadata(self, **fields: Any) -> dict[str, Any]:
        """Écrit des métadonnées d'un coup. Retourne les valeurs précédentes.

        Champs admis : `unit`, `long_name`, `comment`, `display_format`,
        `role`. Le nom passe par `Worksheet.rename_column` (unicité) et le
        type par `set_column_type` (conversion des données).
        """
        allowed = {"unit", "long_name", "comment", "display_format", "role"}
        unknown = set(fields) - allowed
        if unknown:
            raise AttributeError(f"métadonnée(s) inconnue(s) : {sorted(unknown)}")
        previous = {field_name: getattr(self, field_name) for field_name in fields}
        for field_name, value in fields.items():
            setattr(self, field_name, ColumnRole(value) if field_name == "role" else value)
        self._notify_structure(
            StructureChanged(self, "column_metadata_changed", {"fields": dict(fields)})
        )
        return previous

    def set_column_type(self, column_type: ColumnType) -> tuple[ColumnType, np.ndarray]:
        """Change le type et convertit les données. Retourne l'état précédent."""
        column_type = ColumnType(column_type)
        previous_type, previous_values = self._type, self._values
        if column_type is previous_type:
            return previous_type, previous_values.copy()
        self._type = column_type
        if column_type is ColumnType.TEXT:
            self._values = self._coerce(
                [None if isinstance(v, float) and np.isnan(v) else v for v in previous_values]
            )
        else:
            converted = np.empty(previous_values.size, dtype=np.float64)
            for index, value in enumerate(previous_values):
                try:
                    converted[index] = float(value)
                except (TypeError, ValueError):
                    converted[index] = np.nan
            self._values = converted
        self._notify_structure(
            StructureChanged(self, "column_type_changed", {"column_type": column_type})
        )
        self._notify_data(DataChanged(self, 0, self._values.size))
        return previous_type, previous_values.copy()

    # -- utilitaires ------------------------------------------------------

    def _check_row(self, row: int, *, allow_end: bool = False) -> int:
        limit = self._values.size + (1 if allow_end else 0)
        if row < 0:
            row += self._values.size
        if not 0 <= row < max(limit, 1):
            raise IndexError(f"ligne {row} hors bornes (longueur {self._values.size})")
        return row
