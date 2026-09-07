"""Compléments de couverture pour `WorksheetTableModel` et `commands_helpers`.

Branches non touchées par le scénario "utilisateur" de
`test_worksheet_model.py` : colonnes texte, rôles Qt secondaires, entrées
invalides, déplacement de colonne, et la macro de suppression de lignes.
"""

from __future__ import annotations

import pytest
from PySide6.QtCore import QModelIndex, Qt

from plotyx.commands import SetColumnMetadataCommand, mutation_guard
from plotyx.core import Column
from plotyx.core.worksheet import Worksheet
from plotyx.ui.commands_helpers import delete_rows_macro, insert_rows_macro
from plotyx.ui.worksheet_model import WorksheetTableModel


@pytest.fixture
def text_sheet(project):
    sheet = project.resolve_path("[Book1]Sheet2")
    return sheet  # colonne texte "C" : ["x", None, "z"] (fixture conftest)


@pytest.fixture
def text_model(bus, text_sheet):
    m = WorksheetTableModel(text_sheet, bus)
    yield m
    m.disconnect_signals()


def test_column_at_returns_the_column_object(bus, sheet):
    model = WorksheetTableModel(sheet, bus)
    try:
        assert model.column_at(0) is sheet.column("A")
    finally:
        model.disconnect_signals()


def test_row_and_column_count_are_zero_for_a_child_index(bus, sheet):
    model = WorksheetTableModel(sheet, bus)
    try:
        child = model.index(0, 0)
        assert model.rowCount(child) == 0
        assert model.columnCount(child) == 0
    finally:
        model.disconnect_signals()


def test_text_column_shows_empty_string_for_a_missing_value(text_model, text_sheet):
    # "C"[1] est None (valeur manquante légitime pour du texte).
    index = text_model.index(1, 0)
    assert text_model.data(index, Qt.ItemDataRole.DisplayRole) == ""
    assert text_model.data(index, Qt.ItemDataRole.EditRole) == ""


def test_text_column_alignment_is_left(text_model):
    index = text_model.index(0, 0)
    align = text_model.data(index, Qt.ItemDataRole.TextAlignmentRole)
    assert align & Qt.AlignmentFlag.AlignLeft


def test_setting_a_text_cell_to_empty_string_writes_missing(text_model, bus, text_sheet):
    with mutation_guard():
        ok = text_model.setData(text_model.index(0, 0), "")
    assert ok
    assert text_sheet.column("C").values[0] is None


def test_display_format_is_applied_when_present(bus, sheet):
    model = WorksheetTableModel(sheet, bus)
    try:
        with mutation_guard():
            bus.push(SetColumnMetadataCommand(sheet.column("A"), display_format=".2f"))
        text = model.data(model.index(0, 0), Qt.ItemDataRole.DisplayRole)
        assert text == "1.00"
    finally:
        model.disconnect_signals()


def test_an_invalid_display_format_falls_back_to_str(bus, sheet):
    model = WorksheetTableModel(sheet, bus)
    try:
        with mutation_guard():
            bus.push(SetColumnMetadataCommand(sheet.column("A"), display_format="not-a-format"))
        text = model.data(model.index(0, 0), Qt.ItemDataRole.DisplayRole)
        assert text == "1.0"
    finally:
        model.disconnect_signals()


def test_header_data_returns_none_for_unknown_role_or_out_of_range(bus, sheet):
    model = WorksheetTableModel(sheet, bus)
    try:
        assert model.headerData(0, Qt.Orientation.Vertical, Qt.ItemDataRole.ToolTipRole) is None
        assert model.headerData(99, Qt.Orientation.Horizontal) is None
    finally:
        model.disconnect_signals()


def test_data_returns_none_for_an_invalid_index_or_unsupported_role(bus, sheet):
    model = WorksheetTableModel(sheet, bus)
    try:
        assert model.data(QModelIndex()) is None
        assert model.data(model.index(0, 0), Qt.ItemDataRole.DecorationRole) is None
    finally:
        model.disconnect_signals()


def test_set_data_is_refused_outside_the_edit_role_or_on_an_invalid_index(bus, sheet):
    model = WorksheetTableModel(sheet, bus)
    try:
        assert model.setData(model.index(0, 0), "1.0", Qt.ItemDataRole.DisplayRole) is False
        assert model.setData(QModelIndex(), "1.0") is False
    finally:
        model.disconnect_signals()


def test_moving_a_column_resets_the_model(qtbot, bus, sheet):
    model = WorksheetTableModel(sheet, bus)
    try:
        from plotyx.commands import MoveCommand

        with qtbot.waitSignal(model.modelReset, timeout=1000), mutation_guard():
            bus.push(MoveCommand(sheet.column("B"), index=0))
        assert model.headerData(0, Qt.Orientation.Horizontal) == "B"
    finally:
        model.disconnect_signals()


def test_column_index_returns_none_for_a_foreign_column(bus, sheet):
    model = WorksheetTableModel(sheet, bus)
    try:
        foreign = Column("Foreign", [1.0])
        assert model._column_index(foreign) is None
        assert model._column_index("not a column") is None
    finally:
        model.disconnect_signals()


# -- commands_helpers ---------------------------------------------------------


def test_delete_rows_macro_removes_rows_from_every_long_enough_column(bus, sheet):
    with mutation_guard():
        delete_rows_macro(bus, sheet, 0, 1)
    assert sheet.column("A").values.size == 3
    assert sheet.column("B").values.size == 1


def test_delete_rows_macro_is_the_inverse_of_insert_rows_macro(bus, sheet):
    from tests.model_state import assert_same_state, snapshot

    before = snapshot(sheet.project)
    with mutation_guard():
        insert_rows_macro(bus, sheet, 0, 1)
        delete_rows_macro(bus, sheet, 0, 1)
    assert_same_state(before, snapshot(sheet.project))


def test_insert_rows_macro_on_a_worksheet_without_columns_is_a_no_op(bus):
    empty = Worksheet("Empty")
    with mutation_guard():
        insert_rows_macro(bus, empty, 0, 1)
    assert empty.row_count == 0
