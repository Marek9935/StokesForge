"""`WorksheetView` : `QTableView` façon Origin sur un `WorksheetTableModel`.

Toute écriture — édition de cellule, collage, effacement — passe par
`WorksheetTableModel.set_cell()` (donc par le bus de commandes) ou par une
`ModelCommand` poussée directement ici. Ce module ne contient **aucun**
appel à une primitive de `core`.
"""

from __future__ import annotations

from PySide6.QtCore import Qt
from PySide6.QtGui import QKeySequence
from PySide6.QtWidgets import QAbstractItemView, QApplication, QMenu, QTableView, QWidget

from plotyx.commands import DeleteColumnCommand, SetCellsCommand
from plotyx.ui.commands_helpers import delete_rows_macro, insert_rows_macro
from plotyx.ui.rich_header_view import RichHorizontalHeader
from plotyx.ui.worksheet_model import WorksheetTableModel, parse_cell_text

__all__ = ["WorksheetView"]


class WorksheetView(QTableView):
    """Table virtualisée, sélection multi-cellules, copier/coller."""

    def __init__(self, model: WorksheetTableModel, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.setModel(model)
        self.setHorizontalHeader(RichHorizontalHeader(self))
        self.horizontalHeader().sectionClicked.connect(self.selectColumn)
        self.verticalHeader().sectionClicked.connect(self.selectRow)
        self.setSelectionMode(QAbstractItemView.SelectionMode.ContiguousSelection)
        self.setSelectionBehavior(QAbstractItemView.SelectionBehavior.SelectItems)

        self.horizontalHeader().setContextMenuPolicy(Qt.ContextMenuPolicy.CustomContextMenu)
        self.horizontalHeader().customContextMenuRequested.connect(self._column_menu)
        self.verticalHeader().setContextMenuPolicy(Qt.ContextMenuPolicy.CustomContextMenu)
        self.verticalHeader().customContextMenuRequested.connect(self._row_menu)

        self.horizontalHeader().sectionDoubleClicked.connect(self._open_column_properties)

    @property
    def worksheet_model(self) -> WorksheetTableModel:
        model = self.model()
        assert isinstance(model, WorksheetTableModel)
        return model

    # -- clavier --------------------------------------------------------

    def keyPressEvent(self, event) -> None:  # noqa: N802 - API Qt
        if event.matches(QKeySequence.StandardKey.Copy):
            self.copy_selection()
            return
        if event.matches(QKeySequence.StandardKey.Paste):
            self.paste_at_current()
            return
        if event.key() in (Qt.Key.Key_Delete, Qt.Key.Key_Backspace):
            self.clear_selection()
            return
        super().keyPressEvent(event)

    # -- copier / coller --------------------------------------------------

    def copy_selection(self) -> None:
        indexes = self.selectedIndexes()
        if not indexes:
            return
        rows = sorted({index.row() for index in indexes})
        cols = sorted({index.column() for index in indexes})
        model = self.worksheet_model
        lines = []
        for row in rows:
            cells = [str(model.data(model.index(row, col)) or "") for col in cols]
            lines.append("\t".join(cells))
        QApplication.clipboard().setText("\n".join(lines))

    def paste_at_current(self) -> None:
        index = self.currentIndex()
        if not index.isValid():
            return
        text = QApplication.clipboard().text()
        if not text:
            return
        rows = [line.split("\t") for line in text.splitlines()]
        if not rows:
            return
        model = self.worksheet_model
        sheet = model.sheet
        start_row, start_col = index.row(), index.column()
        n_cols = max(len(row) for row in rows)
        with model.bus.macro("Coller"):
            for col_offset in range(n_cols):
                col_index = start_col + col_offset
                if col_index >= sheet.column_count:
                    break
                column = sheet.column(col_index)
                values = [
                    parse_cell_text(
                        column, row[col_offset] if col_offset < len(row) else ""
                    )
                    for row in rows
                ]
                model.bus.push(SetCellsCommand(column, start_row, values, "Coller"))

    def clear_selection(self) -> None:
        indexes = self.selectedIndexes()
        if not indexes:
            return
        model = self.worksheet_model
        sheet = model.sheet
        by_column: dict[int, list[int]] = {}
        for index in indexes:
            by_column.setdefault(index.column(), []).append(index.row())
        # Ne pousse une macro que s'il y a réellement quelque chose à
        # effacer : une macro vide s'empile quand même côté Qt (une entrée
        # d'historique sans effet), inutile de polluer l'undo pour ça.
        to_clear = [
            (sheet.column(col), row)
            for col, rows in by_column.items()
            for row in sorted(set(rows))
            if row < len(sheet.column(col))
        ]
        if not to_clear:
            return
        with model.bus.macro("Effacement"):
            for column, row in to_clear:
                missing = column.column_type.missing
                model.bus.push(SetCellsCommand(column, row, [missing], "Effacement"))

    # -- menus contextuels --------------------------------------------------

    def _column_menu(self, pos) -> QMenu | None:
        """Construit le menu et connecte chaque action à son geste.

        Les actions sont câblées par `triggered`, pas lues sur la valeur de
        retour de `exec()` : ça reste correct que le menu soit affiché pour
        de vrai ou déclenché par du code de test (`action.trigger()`), sans
        dépendre d'une boucle d'évènements modale.
        """
        section = self.horizontalHeader().logicalIndexAt(pos)
        if section < 0:
            return None
        model = self.worksheet_model
        sheet = model.sheet
        column = sheet.column(section)
        menu = QMenu()
        act_props = menu.addAction("Propriétés de la colonne…")
        act_props.triggered.connect(lambda: self._open_column_properties(section))
        act_delete = menu.addAction(f"Supprimer la colonne « {column.name} »")
        act_delete.triggered.connect(lambda: model.bus.push(DeleteColumnCommand(sheet, column)))
        menu.exec(self.horizontalHeader().mapToGlobal(pos))
        return menu

    def _row_menu(self, pos) -> QMenu | None:
        row = self.verticalHeader().logicalIndexAt(pos)
        if row < 0:
            return None
        model = self.worksheet_model
        sheet = model.sheet
        menu = QMenu()
        act_insert = menu.addAction("Insérer une ligne")
        act_insert.triggered.connect(lambda: insert_rows_macro(model.bus, sheet, row, 1))
        act_delete = menu.addAction("Supprimer la ligne")
        act_delete.triggered.connect(lambda: delete_rows_macro(model.bus, sheet, row, 1))
        menu.exec(self.verticalHeader().mapToGlobal(pos))
        return menu

    def _open_column_properties(self, section: int) -> None:
        from plotyx.ui.column_properties_dialog import ColumnPropertiesDialog

        model = self.worksheet_model
        column = model.sheet.column(section)
        dialog = ColumnPropertiesDialog(column, model.bus, self)
        dialog.exec()
