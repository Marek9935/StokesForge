"""`Worksheet` : colonnes de longueurs hétérogènes, insert/delete/move/rename."""

from __future__ import annotations

import numpy as np
import pytest

from plotyx.core import Column, ColumnRole, DuplicateNameError, StructureError, Worksheet


def sheet_with(*lengths: int) -> Worksheet:
    sheet = Worksheet("Sheet1")
    for length in lengths:
        sheet.add_column(values=list(range(length)))
    return sheet


# -- longueurs hétérogènes ------------------------------------------------


def test_columns_keep_their_own_length():
    sheet = sheet_with(3, 7, 0)
    assert [len(column) for column in sheet] == [3, 7, 0]
    assert sheet.row_count == 7
    assert sheet.column_count == 3


def test_row_count_of_an_empty_sheet_is_zero():
    assert Worksheet().row_count == 0


def test_cells_beyond_a_short_column_read_as_nan():
    sheet = sheet_with(2, 5)
    assert sheet.get_cell("A", 4) is not None
    assert np.isnan(sheet.get_cell("A", 4))
    assert sheet.get_cell("B", 4) == 4
    assert len(sheet.column("A")) == 2  # aucun padding n'est stocké


def test_writing_a_cell_below_the_end_extends_only_that_column():
    sheet = sheet_with(2, 2)
    sheet.set_cell("A", 5, 42.0)
    assert len(sheet.column("A")) == 6
    assert len(sheet.column("B")) == 2
    assert sheet.row_count == 6
    assert np.isnan(sheet.get_cell("A", 3))


def test_row_operations_span_all_columns_but_respect_short_ones():
    sheet = Worksheet()
    sheet.add_column("A", [1.0, 2.0, 3.0, 4.0])
    sheet.add_column("B", [1.0, 2.0])

    sheet.insert_rows(1, 1)
    assert len(sheet.column("A")) == 5
    assert len(sheet.column("B")) == 3
    assert list(sheet.column("B").values[:1]) == [1.0]

    removed = sheet.delete_rows(1, 1)
    assert set(removed) == {"A", "B"}
    assert list(sheet.column("A").values) == [1.0, 2.0, 3.0, 4.0]
    assert list(sheet.column("B").values) == [1.0, 2.0]


def test_row_insert_and_delete_are_strict_inverses_over_short_columns():
    """Régression : `insert_rows` rabattait `at` en `min(at, len)`.

    Une colonne trop courte pour atteindre `at` se voyait donc allonger
    d'un NaN à sa fin, alors que `delete_rows` l'ignorait — les deux
    primitives n'étaient pas inverses. Elles sautent désormais le même
    ensemble de colonnes.
    """
    sheet = Worksheet()
    sheet.add_column("longue", [1.0, 2.0, 3.0, 4.0, 5.0])
    sheet.add_column("courte", [1.0, 2.0])
    before = {column.name: np.array(column.values) for column in sheet}

    sheet.insert_rows(3, 2)
    assert len(sheet.column("longue")) == 7
    assert len(sheet.column("courte")) == 2, "la colonne courte n'a pas de ligne 3"

    removed = sheet.delete_rows(3, 2)
    assert list(removed) == ["longue"]
    for name, values in before.items():
        np.testing.assert_array_equal(sheet.column(name).values, values)


def test_inserting_rows_beyond_every_column_is_a_no_op():
    sheet = Worksheet()
    sheet.add_column("A", [1.0, 2.0])
    sheet.insert_rows(10, 3)
    assert len(sheet.column("A")) == 2


def test_delete_rows_skips_columns_that_are_too_short():
    sheet = Worksheet()
    sheet.add_column("A", [1.0, 2.0, 3.0])
    sheet.add_column("B", [1.0])
    removed = sheet.delete_rows(2, 1)
    assert list(removed) == ["A"]
    assert len(sheet.column("B")) == 1


# -- résolution de colonne ------------------------------------------------


def test_column_lookup_by_name_index_and_object():
    sheet = sheet_with(1, 1)
    column = sheet.column("B")
    assert sheet.column(1) is column
    assert sheet.column(column) is column
    assert sheet.index_of("B") == 1
    assert "B" in sheet and "Z" not in sheet


def test_unknown_column_name_raises():
    with pytest.raises(KeyError):
        Worksheet().column("A")


def test_column_from_another_sheet_is_rejected():
    other = sheet_with(1)
    with pytest.raises(KeyError):
        Worksheet().column(other.column(0))


# -- structure ------------------------------------------------------------


def test_generated_names_follow_the_spreadsheet_sequence():
    sheet = Worksheet()
    names = [sheet.add_column().name for _ in range(28)]
    assert names[:3] == ["A", "B", "C"]
    assert names[25:28] == ["Z", "AA", "AB"]


def test_generated_name_skips_taken_names():
    sheet = Worksheet()
    sheet.add_column("A")
    assert sheet.next_free_name() == "B"


def test_insert_column_at_position():
    sheet = sheet_with(1, 1)
    sheet.insert_column(0, Column("Z"))
    assert sheet.column_names() == ("Z", "A", "B")


def test_negative_insert_index_counts_from_the_end():
    """-1 désigne la fin de la feuille (position d'ajout), pas l'avant-dernière."""
    sheet = sheet_with(1, 1)
    sheet.insert_column(-1, Column("Z"))
    assert sheet.column_names() == ("A", "B", "Z")
    sheet.insert_column(-2, Column("Y"))
    assert sheet.column_names() == ("A", "B", "Y", "Z")


def test_duplicate_column_name_is_refused():
    sheet = sheet_with(1)
    with pytest.raises(DuplicateNameError):
        sheet.add_column("A")


def test_a_column_belongs_to_a_single_sheet():
    sheet = sheet_with(1)
    other = Worksheet("Sheet2")
    with pytest.raises(StructureError):
        other.insert_column(0, sheet.column("A"))


def test_only_columns_go_into_a_sheet():
    with pytest.raises(StructureError):
        Worksheet().insert_column(0, "pas une colonne")


def test_remove_column_returns_index_and_object_for_undo():
    sheet = sheet_with(1, 1, 1)
    index, column = sheet.remove_column("B")

    assert (index, column.name) == (1, "B")
    assert sheet.column_names() == ("A", "C")
    assert column.parent is None

    sheet.insert_column(index, column)  # annulation par delta
    assert sheet.column_names() == ("A", "B", "C")


def test_move_column_returns_previous_index():
    sheet = sheet_with(1, 1, 1)
    previous = sheet.move_column("C", 0)
    assert previous == 2
    assert sheet.column_names() == ("C", "A", "B")

    sheet.move_column("C", previous)  # annulation
    assert sheet.column_names() == ("A", "B", "C")


def test_move_column_to_its_own_position_is_a_noop():
    sheet = sheet_with(1, 1)
    assert sheet.move_column("A", 0) == 0
    assert sheet.column_names() == ("A", "B")


def test_move_column_out_of_range_raises():
    sheet = sheet_with(1)
    with pytest.raises(IndexError):
        sheet.move_column("A", 5)


def test_rename_column_checks_uniqueness():
    sheet = sheet_with(1, 1)
    previous = sheet.rename_column("A", "Temps")
    assert previous == "A"
    assert sheet.column_names() == ("Temps", "B")

    with pytest.raises(DuplicateNameError):
        sheet.rename_column("Temps", "B")


def test_renaming_a_column_to_its_own_name_is_allowed():
    sheet = sheet_with(1)
    assert sheet.rename_column("A", "A") == "A"


def test_x_column_returns_the_first_x_role():
    sheet = sheet_with(1, 1)
    assert sheet.x_column() is None
    sheet.column("B").set_metadata(role=ColumnRole.X)
    assert sheet.x_column() is sheet.column("B")


def test_to_arrays_gives_independent_copies():
    sheet = sheet_with(2)
    arrays = sheet.to_arrays()
    arrays["A"][0] = 99.0
    assert sheet.get_cell("A", 0) == 0.0


def test_constructor_accepts_initial_columns():
    sheet = Worksheet("S", [Column("A", [1.0]), Column("B", [2.0])])
    assert sheet.column_names() == ("A", "B")
    assert sheet.column("A").parent is sheet
