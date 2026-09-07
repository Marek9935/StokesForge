"""`ColumnPropertiesDialog` : accept() ne fait que pousser des commandes."""

from __future__ import annotations

from plotyx.commands import mutation_guard
from plotyx.core.column import ColumnRole
from plotyx.ui.column_properties_dialog import ColumnPropertiesDialog


def test_dialog_is_prefilled_with_the_column_metadata(qtbot, bus, sheet):
    column = sheet.column("A")
    dialog = ColumnPropertiesDialog(column, bus)
    qtbot.addWidget(dialog)
    assert dialog._name_edit.text() == "A"
    assert dialog._unit_edit.text() == "s"
    assert dialog._role_combo.currentData() == ColumnRole.X


def test_accepting_a_rename_pushes_a_rename_command(qtbot, bus, sheet):
    column = sheet.column("A")
    dialog = ColumnPropertiesDialog(column, bus)
    qtbot.addWidget(dialog)
    dialog._name_edit.setText("Temps")
    with mutation_guard():
        dialog._on_accept()
    assert column.name == "Temps"
    assert bus.can_undo


def test_accepting_metadata_changes_pushes_a_metadata_command(qtbot, bus, sheet):
    column = sheet.column("A")
    dialog = ColumnPropertiesDialog(column, bus)
    qtbot.addWidget(dialog)
    dialog._unit_edit.setText("ms")
    dialog._long_name_edit.setText("Temps écoulé")
    with mutation_guard():
        dialog._on_accept()
    assert column.unit == "ms"
    assert column.long_name == "Temps écoulé"


def test_accepting_a_rename_and_metadata_together_is_a_single_macro(qtbot, bus, sheet):
    column = sheet.column("A")
    dialog = ColumnPropertiesDialog(column, bus)
    qtbot.addWidget(dialog)
    before = bus.count
    dialog._name_edit.setText("Temps")
    dialog._unit_edit.setText("ms")
    with mutation_guard():
        dialog._on_accept()
    assert bus.count == before + 1  # une seule entrée d'historique
    bus.undo()
    assert column.name == "A"
    assert column.unit == "s"


def test_no_change_does_not_push_any_command(qtbot, bus, sheet):
    column = sheet.column("A")
    dialog = ColumnPropertiesDialog(column, bus)
    qtbot.addWidget(dialog)
    before = bus.count
    with mutation_guard():
        dialog._on_accept()
    assert bus.count == before


def test_a_duplicate_name_is_refused_and_reported_without_crashing(qtbot, bus, sheet):
    column = sheet.column("A")
    dialog = ColumnPropertiesDialog(column, bus)
    qtbot.addWidget(dialog)
    dialog._name_edit.setText("B")  # déjà pris par l'autre colonne
    with mutation_guard():
        dialog._on_accept()
    assert dialog.error is not None
    assert column.name == "A"  # inchangé : la commande a échoué avant d'empiler
