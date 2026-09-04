"""`WorksheetTableModel` : adaptateur Qt entre `Worksheet` et `QTableView`.

Le principe non négociable de la phase 3 est ici à son point le plus
sensible : **aucune copie des données dans la vue**. `data()` lit toujours
à travers `Worksheet.get_cell` / `Column.get`, jamais un tableau mis en
cache ; `setData()` ne touche jamais `core` directement, il pousse une
`SetCellsCommand` sur le bus.

## L'adaptateur de notifications

`core` notifie en Python pur (`plotyx.core.signals.Signal`), pas en signaux
Qt. Ce module est le pont : il s'abonne **une fois**, au niveau de la
feuille, à `data_changed` et `structure_changed`. C'est suffisant pour tout
voir, colonnes présentes et futures comprises, parce que les notifications
remontent l'arbre depuis la colonne jusqu'à ses ancêtres (voir
`core/node.py`) — pas parce qu'on réabonne chaque colonne une à une.

Deux compteurs (`_row_count`, `_column_count`) sont tenus à la main plutôt
que recalculés à la volée : Qt exige que `rowCount()`/`columnCount()`
retournent l'ancienne valeur tant que `beginInsertRows`/`beginInsertColumns`
n'a pas été suivi de son `end*`, alors que la mutation du modèle métier a
déjà eu lieu (la commande applique avant de notifier). Lire directement
`sheet.row_count` à l'intérieur de ce couloir renverrait la nouvelle valeur
trop tôt et corromprait la vue — exactement le piège documenté dans la
consigne de cette phase.

## Colonnes de longueur variable

Une ligne au-delà de la longueur d'une colonne n'existe pas : `data()`
rend une chaîne vide, jamais `"nan"`. Un `NaN` réellement stocké *dans*
la plage de la colonne est en revanche une valeur manquante légitime et
s'affiche (`str(float("nan")) == "nan"`).
"""

from __future__ import annotations

from typing import Any

import numpy as np
from PySide6.QtCore import QAbstractTableModel, QModelIndex, Qt

from plotyx.commands import CommandBus, SetCellsCommand
from plotyx.core.column import Column, ColumnType
from plotyx.core.node import DataChanged, StructureChanged
from plotyx.core.worksheet import Worksheet

__all__ = ["WorksheetTableModel", "parse_cell_text"]

#: Rôle custom : liste de lignes (nom, unité, label long) pour l'en-tête
#: riche peinte par `RichHorizontalHeader`.
HEADER_LINES_ROLE = Qt.ItemDataRole.UserRole + 1


class _Invalid:
    """Sentinelle : la saisie ne se convertit pas dans le type de la colonne."""

    __slots__ = ()


_INVALID = _Invalid()


def parse_cell_text(column: Column, text: Any) -> Any:
    """Convertit une saisie utilisateur vers le type de `column`.

    Chaîne vide → valeur manquante du type. Échec de conversion numérique →
    valeur manquante aussi (comportement de collage tolérant, façon tableur) ;
    `WorksheetTableModel.setData` distingue ce cas pour refuser l'édition
    interactive d'une cellule unique plutôt que d'avaler l'erreur.
    """
    parsed = WorksheetTableModel._parse(column, text)
    return column.column_type.missing if parsed is _INVALID else parsed


class WorksheetTableModel(QAbstractTableModel):
    """Vue Qt en lecture/écriture sur un `Worksheet`, sans copie."""

    def __init__(
        self, sheet: Worksheet, bus: CommandBus, parent: Any = None
    ) -> None:
        super().__init__(parent)
        self._sheet = sheet
        self._bus = bus
        self._row_count = sheet.row_count
        self._column_count = sheet.column_count
        self._data_conn = sheet.data_changed.connect(self._on_data_changed)
        self._structure_conn = sheet.structure_changed.connect(self._on_structure_changed)

    # -- cycle de vie -------------------------------------------------------

    def disconnect_signals(self) -> None:
        """À appeler quand la vue se ferme : évite de notifier un modèle mort."""
        self._data_conn.disconnect()
        self._structure_conn.disconnect()

    @property
    def sheet(self) -> Worksheet:
        return self._sheet

    @property
    def bus(self) -> CommandBus:
        return self._bus

    def column_at(self, index: int) -> Column:
        return self._sheet.column(index)

    # -- QAbstractTableModel : lecture --------------------------------------

    def rowCount(self, parent: QModelIndex = QModelIndex()) -> int:  # noqa: N802,B008 - API Qt
        if parent.isValid():
            return 0
        return self._row_count

    def columnCount(self, parent: QModelIndex = QModelIndex()) -> int:  # noqa: N802,B008 - API Qt
        if parent.isValid():
            return 0
        return self._column_count

    def data(self, index: QModelIndex, role: int = Qt.ItemDataRole.DisplayRole) -> Any:
        if not index.isValid():
            return None
        if role not in (
            Qt.ItemDataRole.DisplayRole,
            Qt.ItemDataRole.EditRole,
            Qt.ItemDataRole.TextAlignmentRole,
        ):
            return None
        column = self._sheet.column(index.column())
        row = index.row()
        if role == Qt.ItemDataRole.TextAlignmentRole:
            align = Qt.AlignmentFlag.AlignVCenter
            align |= (
                Qt.AlignmentFlag.AlignLeft
                if column.column_type is ColumnType.TEXT
                else Qt.AlignmentFlag.AlignRight
            )
            return int(align)
        if row >= len(column):
            # Hors de la longueur propre à cette colonne : pas de ligne du
            # tout, donc une cellule vide — jamais "nan".
            return "" if role == Qt.ItemDataRole.DisplayRole else None
        value = column.get(row)
        return self._format(column, value, role)

    def _format(self, column: Column, value: Any, role: int) -> Any:
        if column.column_type is ColumnType.TEXT:
            return "" if value is None else str(value)
        if role == Qt.ItemDataRole.EditRole:
            # La valeur brute pour le delegate d'édition, pas sa mise en forme.
            return "" if (isinstance(value, float) and np.isnan(value)) else value
        if isinstance(value, float) and np.isnan(value):
            return "nan"  # NaN stocké : valeur manquante légitime, visible.
        if column.display_format:
            try:
                return format(value, column.display_format)
            except (ValueError, TypeError):  # pragma: no cover - format invalide
                return str(value)
        return str(value)

    def headerData(
        self,
        section: int,
        orientation: Qt.Orientation,
        role: int = Qt.ItemDataRole.DisplayRole,
    ) -> Any:
        if orientation == Qt.Orientation.Vertical:
            if role == Qt.ItemDataRole.DisplayRole:
                return str(section + 1)
            return None
        if section < 0 or section >= self._column_count:
            return None
        column = self._sheet.column(section)
        if role == HEADER_LINES_ROLE:
            unit = f"({column.unit})" if column.unit else ""
            return (f"{column.name} [{column.role}]", unit, column.long_name)
        if role == Qt.ItemDataRole.DisplayRole:
            return column.name
        if role == Qt.ItemDataRole.ToolTipRole:
            parts = [column.name]
            if column.unit:
                parts.append(f"unité : {column.unit}")
            if column.long_name:
                parts.append(column.long_name)
            if column.comment:
                parts.append(column.comment)
            return "\n".join(parts)
        return None

    def flags(self, index: QModelIndex) -> Qt.ItemFlag:
        if not index.isValid():
            return Qt.ItemFlag.NoItemFlags
        return (
            Qt.ItemFlag.ItemIsEnabled
            | Qt.ItemFlag.ItemIsSelectable
            | Qt.ItemFlag.ItemIsEditable
        )

    # -- QAbstractTableModel : écriture --------------------------------------

    def setData(  # noqa: N802 - API Qt
        self, index: QModelIndex, value: Any, role: int = Qt.ItemDataRole.EditRole
    ) -> bool:
        if role != Qt.ItemDataRole.EditRole or not index.isValid():
            return False
        column = self._sheet.column(index.column())
        parsed = self._parse(column, value)
        if parsed is _INVALID:
            return False
        return self.set_cell(column, index.row(), parsed)

    def set_cell(self, column: Column, row: int, value: Any, text: str = "") -> bool:
        """Pousse une `SetCellsCommand` d'une seule cellule. Seul chemin d'écriture."""
        try:
            self._bus.push(SetCellsCommand(column, row, [value], text))
        except Exception:
            return False
        return True

    @staticmethod
    def _parse(column: Column, value: Any) -> Any:
        if column.column_type is ColumnType.TEXT:
            if value is None:
                return None
            text = str(value)
            return None if text == "" else text
        text = str(value).strip() if not isinstance(value, int | float) else value
        if text == "":
            return np.nan
        try:
            return float(text)
        except (TypeError, ValueError):
            return _INVALID

    # -- adaptateur de notifications core → Qt -------------------------------

    def _sync_rows(self) -> None:
        new_count = self._sheet.row_count
        old_count = self._row_count
        if new_count == old_count:
            return
        if new_count > old_count:
            self.beginInsertRows(QModelIndex(), old_count, new_count - 1)
            self._row_count = new_count
            self.endInsertRows()
        else:
            self.beginRemoveRows(QModelIndex(), new_count, old_count - 1)
            self._row_count = new_count
            self.endRemoveRows()

    def _column_index(self, obj: Any) -> int | None:
        if not isinstance(obj, Column):
            return None
        try:
            return self._sheet.index_of(obj)
        except (KeyError, ValueError):
            return None

    def _on_data_changed(self, event: DataChanged) -> None:
        self._sync_rows()
        col = self._column_index(event.source)
        if col is None:
            return
        start = max(0, event.start)
        stop = min(event.stop, self._row_count)
        if stop <= start:
            return
        top_left = self.index(start, col)
        bottom_right = self.index(stop - 1, col)
        self.dataChanged.emit(
            top_left, bottom_right, [Qt.ItemDataRole.DisplayRole, Qt.ItemDataRole.EditRole]
        )

    def _on_structure_changed(self, event: StructureChanged) -> None:
        kind = event.kind
        if kind == "column_inserted":
            index = int(event.details["index"])
            self.beginInsertColumns(QModelIndex(), index, index)
            self._column_count += 1
            self.endInsertColumns()
            self._sync_rows()
        elif kind == "column_removed":
            index = int(event.details["index"])
            self.beginRemoveColumns(QModelIndex(), index, index)
            self._column_count -= 1
            self.endRemoveColumns()
            self._sync_rows()
        elif kind == "column_moved":
            self.beginResetModel()
            self._row_count = self._sheet.row_count
            self._column_count = self._sheet.column_count
            self.endResetModel()
        elif kind in ("rows_inserted", "rows_deleted"):
            self._sync_rows()
        elif kind == "node_renamed" and self._column_index(event.source) is not None:
            col = self._column_index(event.source)
            self.headerDataChanged.emit(Qt.Orientation.Horizontal, col, col)
        elif kind in ("column_metadata_changed", "column_type_changed"):
            col = self._column_index(event.source)
            if col is not None:
                self.headerDataChanged.emit(Qt.Orientation.Horizontal, col, col)
