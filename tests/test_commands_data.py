"""Commandes de données : delta d'inversion, longueurs, métadonnées, type."""

from __future__ import annotations

import numpy as np
import pytest

from plotyx.commands import (
    DeleteRowsCommand,
    InsertRowsCommand,
    SetCellsCommand,
    SetColumnLengthCommand,
    SetColumnMetadataCommand,
    SetColumnTypeCommand,
)
from plotyx.core import ColumnRole, ColumnType


def test_set_cells_writes_and_undoes(bus, sheet):
    column = sheet.column("A")
    bus.push(SetCellsCommand(column, 1, [9.0, 9.5]))
    np.testing.assert_array_equal(column.values, [1.0, 9.0, 9.5, 4.0])

    bus.undo()
    np.testing.assert_array_equal(column.values, [1.0, 2.0, np.nan, 4.0])

    bus.redo()
    np.testing.assert_array_equal(column.values, [1.0, 9.0, 9.5, 4.0])


def test_set_cells_beyond_the_end_restores_the_previous_length(bus, sheet):
    column = sheet.column("B")
    bus.push(SetCellsCommand(column, 5, [1.0]))
    assert len(column) == 6

    bus.undo()
    assert len(column) == 2
    np.testing.assert_array_equal(column.values, [10.0, 20.0])


def test_set_cells_keeps_only_the_overwritten_cells(bus, sheet):
    """Le delta d'annulation ne dépend pas de la taille de la colonne."""
    column = sheet.column("A")
    bus.push(SetColumnLengthCommand(column, 10_000))
    command = bus.push(SetCellsCommand(column, 4_000, [1.0, 2.0, 3.0]))

    assert command._delta.previous.size == 3


def test_set_cells_copies_the_caller_array(bus, sheet):
    column = sheet.column("B")
    values = np.array([7.0, 8.0])
    bus.push(SetCellsCommand(column, 0, values))
    values[0] = 999.0

    bus.undo()
    bus.redo()
    np.testing.assert_array_equal(column.values, [7.0, 8.0])


def test_set_cells_refuses_a_negative_row(sheet):
    with pytest.raises(IndexError):
        SetCellsCommand(sheet.column("A"), -1, [0.0])


def test_set_column_length_truncates_and_restores(bus, sheet):
    column = sheet.column("A")
    bus.push(SetColumnLengthCommand(column, 2))
    np.testing.assert_array_equal(column.values, [1.0, 2.0])

    bus.undo()
    np.testing.assert_array_equal(column.values, [1.0, 2.0, np.nan, 4.0])


def test_set_column_length_extends_and_restores(bus, sheet):
    column = sheet.column("B")
    bus.push(SetColumnLengthCommand(column, 5))
    assert len(column) == 5

    bus.undo()
    np.testing.assert_array_equal(column.values, [10.0, 20.0])


def test_set_column_length_refuses_a_negative_length(sheet):
    with pytest.raises(ValueError, match="négative"):
        SetColumnLengthCommand(sheet.column("A"), -3)


def test_insert_rows_is_undone_by_parameters_only(bus, sheet):
    column = sheet.column("A")
    command = bus.push(InsertRowsCommand(column, 1, 2))
    assert len(column) == 6
    assert command.retained_objects() == ()

    bus.undo()
    np.testing.assert_array_equal(column.values, [1.0, 2.0, np.nan, 4.0])


def test_delete_rows_restores_the_removed_values(bus, sheet):
    column = sheet.column("A")
    bus.push(DeleteRowsCommand(column, 1, 2))
    np.testing.assert_array_equal(column.values, [1.0, 4.0])

    bus.undo()
    np.testing.assert_array_equal(column.values, [1.0, 2.0, np.nan, 4.0])


def test_delete_rows_clamped_at_the_end_still_round_trips(bus, sheet):
    column = sheet.column("B")
    bus.push(DeleteRowsCommand(column, 1, 99))
    np.testing.assert_array_equal(column.values, [10.0])

    bus.undo()
    np.testing.assert_array_equal(column.values, [10.0, 20.0])


def test_row_commands_accept_a_negative_index(bus, sheet):
    column = sheet.column("A")
    bus.push(DeleteRowsCommand(column, -1, 1))
    np.testing.assert_array_equal(column.values, [1.0, 2.0, np.nan])
    bus.undo()
    np.testing.assert_array_equal(column.values, [1.0, 2.0, np.nan, 4.0])


def test_row_commands_refuse_a_negative_count(sheet):
    with pytest.raises(ValueError, match="négatif"):
        InsertRowsCommand(sheet.column("A"), 0, -1)
    with pytest.raises(ValueError, match="négatif"):
        DeleteRowsCommand(sheet.column("A"), 0, -1)


def test_set_metadata_round_trip(bus, sheet):
    column = sheet.column("B")
    bus.push(SetColumnMetadataCommand(column, unit="V", role=ColumnRole.Y_ERROR))
    assert column.unit == "V"
    assert column.role is ColumnRole.Y_ERROR

    bus.undo()
    assert column.unit == ""
    assert column.role is ColumnRole.Y


def test_set_metadata_needs_at_least_one_field(sheet):
    with pytest.raises(ValueError, match="aucune métadonnée"):
        SetColumnMetadataCommand(sheet.column("A"))


def test_set_column_type_is_the_documented_snapshot_case(bus, project):
    """La conversion est destructrice : seul un snapshot peut l'annuler."""
    column = project.resolve_path("[Book1]Sheet2!C")
    assert column.column_type is ColumnType.TEXT

    command = bus.push(SetColumnTypeCommand(column, ColumnType.NUMERIC))
    assert column.column_type is ColumnType.NUMERIC
    assert np.isnan(column.values).all()  # "x", None, "z" ne se parsent pas
    assert command._previous_values is not None

    bus.undo()
    assert column.column_type is ColumnType.TEXT
    assert list(column.values) == ["x", None, "z"]


def test_set_column_type_releases_its_snapshot(bus, project):
    column = project.resolve_path("[Book1]Sheet2!C")
    command = bus.push(SetColumnTypeCommand(column, ColumnType.NUMERIC))
    command.release()
    assert command._previous_values is None


def test_undo_emits_notifications_for_the_view(bus, sheet):
    events = []
    sheet.data_changed.connect(events.append)
    bus.push(SetCellsCommand(sheet.column("A"), 0, [0.0]))
    assert len(events) == 1

    bus.undo()
    assert len(events) == 2  # la vue est prévenue de l'annulation aussi


def test_set_cells_exposes_its_column_and_the_execution_depth(bus, sheet):
    from plotyx.commands import execution_depth, is_executing

    command = SetCellsCommand(sheet.column("A"), 0, [1.0])
    assert command.column is sheet.column("A")
    assert execution_depth() == 0
    assert not is_executing()

    depths = []
    sheet.data_changed.connect(lambda _event: depths.append(execution_depth()))
    bus.push(command)
    assert depths == [1]


def test_deleting_zero_rows_still_round_trips(bus, sheet):
    column = sheet.column("A")
    bus.push(DeleteRowsCommand(column, 4, 0))
    np.testing.assert_array_equal(column.values, [1.0, 2.0, np.nan, 4.0])
    bus.undo()
    np.testing.assert_array_equal(column.values, [1.0, 2.0, np.nan, 4.0])
