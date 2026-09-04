"""Dialogue « Column Properties » : nom, unité, rôle, format d'affichage.

`accept()` est le seul chemin d'écriture, et il ne fait que pousser des
commandes sur le bus (`RenameCommand` pour le nom, `SetColumnMetadataCommand`
pour le reste), groupées en une macro si les deux changent : `core` n'est
jamais touché directement.
"""

from __future__ import annotations

from PySide6.QtWidgets import (
    QComboBox,
    QDialog,
    QDialogButtonBox,
    QFormLayout,
    QLineEdit,
    QWidget,
)

from plotyx.commands import CommandBus, RenameCommand, SetColumnMetadataCommand
from plotyx.core.column import Column, ColumnRole
from plotyx.core.errors import PlotyxError

__all__ = ["ColumnPropertiesDialog"]


class ColumnPropertiesDialog(QDialog):
    """Édite les métadonnées d'une colonne. Chaque validation pousse une commande."""

    def __init__(self, column: Column, bus: CommandBus, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self._column = column
        self._bus = bus
        self.setWindowTitle(f"Propriétés de la colonne « {column.name} »")

        self._name_edit = QLineEdit(column.name)
        self._unit_edit = QLineEdit(column.unit)
        self._long_name_edit = QLineEdit(column.long_name)
        self._comment_edit = QLineEdit(column.comment)
        self._format_edit = QLineEdit(column.display_format)
        self._role_combo = QComboBox()
        for role in ColumnRole:
            self._role_combo.addItem(role.value, role)
        self._role_combo.setCurrentIndex(self._role_combo.findData(column.role))

        form = QFormLayout()
        form.addRow("Nom", self._name_edit)
        form.addRow("Unité", self._unit_edit)
        form.addRow("Label long", self._long_name_edit)
        form.addRow("Commentaire", self._comment_edit)
        form.addRow("Format d'affichage", self._format_edit)
        form.addRow("Rôle", self._role_combo)

        buttons = QDialogButtonBox(
            QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel
        )
        buttons.accepted.connect(self._on_accept)
        buttons.rejected.connect(self.reject)

        layout = QFormLayout(self)
        layout.addRow(form)
        layout.addRow(buttons)

        self._error: str | None = None

    def _on_accept(self) -> None:
        self._error = None
        new_name = self._name_edit.text().strip()
        metadata = {
            "unit": self._unit_edit.text(),
            "long_name": self._long_name_edit.text(),
            "comment": self._comment_edit.text(),
            "display_format": self._format_edit.text(),
            "role": self._role_combo.currentData(),
        }
        changed_meta = {
            key: value
            for key, value in metadata.items()
            if str(value) != str(getattr(self._column, key))
        }
        renaming = new_name and new_name != self._column.name
        try:
            if renaming and changed_meta:
                with self._bus.macro(f"Propriétés de {self._column.name}"):
                    self._bus.push(RenameCommand(self._column, new_name))
                    self._bus.push(SetColumnMetadataCommand(self._column, **changed_meta))
            elif renaming:
                self._bus.push(RenameCommand(self._column, new_name))
            elif changed_meta:
                self._bus.push(SetColumnMetadataCommand(self._column, **changed_meta))
        except PlotyxError as error:
            self._error = str(error)
            return
        self.accept()

    @property
    def error(self) -> str | None:
        """Dernier message d'erreur métier (nom en double, etc.), ou None."""
        return self._error
