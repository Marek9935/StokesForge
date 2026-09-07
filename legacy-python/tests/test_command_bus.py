"""Le bus : pile, macros, limite d'historique, état « document modifié »."""

from __future__ import annotations

import pytest

from plotyx.commands import (
    AddColumnCommand,
    CommandBus,
    DeleteColumnCommand,
    SetCellsCommand,
)
from plotyx.core import Column


def test_push_applies_then_stacks(bus, sheet):
    assert bus.count == 0
    bus.push(SetCellsCommand(sheet.column("A"), 0, [0.0]))
    assert bus.count == 1
    assert bus.index == 1
    assert bus.can_undo and not bus.can_redo
    assert bus.undo_text == "Saisie dans A"

    bus.undo()
    assert bus.can_redo
    assert bus.redo_text == "Saisie dans A"


def test_push_refuses_a_foreign_command(bus):
    with pytest.raises(TypeError):
        bus.push(object())


def test_clear_empties_the_history(bus, sheet):
    bus.push(SetCellsCommand(sheet.column("A"), 0, [0.0]))
    bus.clear()
    assert bus.count == 0
    assert not bus.can_undo


# -- macros -----------------------------------------------------------------


def test_a_macro_is_a_single_history_entry(bus, sheet):
    with bus.macro("Import"):
        bus.push(AddColumnCommand(sheet, Column("D", [1.0])))
        bus.push(AddColumnCommand(sheet, Column("E", [2.0])))
        bus.push(SetCellsCommand(sheet.column("A"), 0, [42.0]))

    assert bus.count == 1
    assert bus.undo_text == "Import"

    bus.undo()
    assert sheet.column_names() == ("A", "B")
    assert sheet.column("A").values[0] == 1.0

    bus.redo()
    assert sheet.column_names() == ("A", "B", "D", "E")


def test_nested_macros_collapse_into_one_entry(bus, sheet):
    with bus.macro("Externe"):
        bus.push(AddColumnCommand(sheet, Column("D")))
        with bus.macro("Interne"):
            bus.push(AddColumnCommand(sheet, Column("E")))
    assert bus.count == 1
    bus.undo()
    assert sheet.column_names() == ("A", "B")


def test_an_aborted_macro_leaves_no_trace(bus, sheet):
    bus.push(AddColumnCommand(sheet, Column("Z")))
    with pytest.raises(ValueError, match="boum"):
        with bus.macro("Import raté"):
            bus.push(AddColumnCommand(sheet, Column("D")))
            raise ValueError("boum")

    assert sheet.column_names() == ("A", "B", "Z")
    assert bus.count == 1
    assert bus.index == 1
    assert not bus.can_redo
    assert bus.retained_objects() == ()


def test_end_macro_without_begin(bus):
    with pytest.raises(RuntimeError, match="end_macro"):
        bus.end_macro()


def test_abort_macro_without_begin(bus):
    with pytest.raises(RuntimeError, match="abort_macro"):
        bus.abort_macro()


def test_in_macro_flag(bus):
    assert not bus.in_macro
    bus.begin_macro("M")
    assert bus.in_macro
    bus.end_macro()
    assert not bus.in_macro


# -- limite d'historique ----------------------------------------------------


def test_undo_limit_is_configurable(qapp, project):
    bus = CommandBus(project, undo_limit=3)
    assert bus.undo_limit == 3
    sheet = project.resolve_path("[Book1]Sheet1")
    for row in range(5):
        bus.push(SetCellsCommand(sheet.column("A"), 0, [float(row)]))
    assert bus.count == 3


def test_undo_limit_only_changes_on_an_empty_stack(bus, sheet):
    bus.undo_limit = 5
    assert bus.undo_limit == 5
    bus.push(SetCellsCommand(sheet.column("A"), 0, [0.0]))
    with pytest.raises(RuntimeError, match="pile vide"):
        bus.undo_limit = 2


def test_undo_limit_refuses_a_negative_value(bus):
    with pytest.raises(ValueError, match="négative"):
        bus.undo_limit = -1


# -- état « document modifié » ----------------------------------------------


def test_clean_state_drives_the_modified_flag(bus, sheet):
    seen = []
    bus.modified_changed.connect(seen.append)
    bus.clean_changed.connect(lambda clean: seen.append(("clean", clean)))

    assert not bus.is_modified
    bus.push(SetCellsCommand(sheet.column("A"), 0, [0.0]))
    assert bus.is_modified
    assert seen == [("clean", False), True] or seen == [True, ("clean", False)]

    bus.undo()
    assert not bus.is_modified  # retour au point de sauvegarde


def test_set_clean_marks_the_current_state_as_saved(bus, sheet):
    bus.push(SetCellsCommand(sheet.column("A"), 0, [0.0]))
    bus.set_clean()
    assert not bus.is_modified

    bus.undo()
    assert bus.is_modified  # on s'éloigne du point de sauvegarde


def test_bus_exposes_the_project_and_the_qt_stack(bus, project):
    from PySide6.QtGui import QUndoStack

    assert bus.project is project
    assert isinstance(bus.stack, QUndoStack)


def test_deleting_a_column_then_pushing_truncates_the_redo_branch(bus, sheet):
    bus.push(DeleteColumnCommand(sheet, "B"))
    bus.undo()
    assert bus.can_redo

    bus.push(SetCellsCommand(sheet.column("A"), 0, [0.0]))
    assert not bus.can_redo
    assert bus.count == 1


def test_a_sheet_wide_row_insert_is_a_macro_of_column_commands(bus, sheet):
    """Les commandes de lignes visent une colonne ; la feuille est une macro.

    Les primitives de feuille `Worksheet.insert_rows`/`delete_rows` sont
    désormais des inverses strictes et pourraient être enveloppées telles
    quelles. La macro par colonne reste le chemin retenu : elle tient en
    une seule entrée d'historique et garde le jeu de commandes à l'échelle
    de la colonne, plus fin.
    """
    import numpy as np

    from plotyx.commands import InsertRowsCommand

    before = {column.name: np.array(column.values) for column in sheet}
    with bus.macro("Insertion de lignes"):
        for column in sheet:
            if 3 < len(column):  # colonnes trop courtes : rien à décaler
                bus.push(InsertRowsCommand(column, 3, 2))

    assert bus.count == 1
    assert len(sheet.column("A")) == 6
    assert len(sheet.column("B")) == 2

    bus.undo()
    for name, values in before.items():
        np.testing.assert_array_equal(sheet.column(name).values, values)
