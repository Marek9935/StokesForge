"""`Column` : stockage float64 + NaN, longueur libre, mutations par delta."""

from __future__ import annotations

import numpy as np
import pytest

from plotyx.core import Column, ColumnRole, ColumnType, InvalidNameError


def test_default_column_is_float64_and_empty():
    column = Column("A")
    assert column.column_type is ColumnType.NUMERIC
    assert column.values.dtype == np.float64
    assert len(column) == 0
    assert column.role is ColumnRole.Y


def test_metadata_round_trip():
    column = Column(
        "T",
        [1.0],
        role=ColumnRole.X,
        unit="K",
        long_name="Température",
        comment="thermocouple",
        display_format="%.2f",
    )
    assert (column.unit, column.long_name, column.comment) == ("K", "Température", "thermocouple")
    assert column.display_format == "%.2f"
    assert column.role is ColumnRole.X


def test_invalid_names_are_refused():
    for name in ["", "  ", "A!B", "[A]", "a/b", " A"]:
        with pytest.raises(InvalidNameError):
            Column(name)


# -- valeurs manquantes : NaN, pas np.ma ---------------------------------


def test_missing_values_are_nan_not_masked():
    column = Column("A", [1.0, np.nan, 3.0])
    assert not isinstance(column.values, np.ma.MaskedArray)
    assert column.is_missing(1)
    assert not column.is_missing(0)
    assert np.nanmean(column.values) == 2.0


def test_none_becomes_nan_on_a_numeric_column():
    column = Column("A", [1.0, None, 3.0])
    assert np.isnan(column.get(1))


def test_reading_past_the_end_yields_nan_without_error():
    column = Column("A", [1.0, 2.0])
    assert np.isnan(column.get(99))
    assert column.is_missing(99)
    assert len(column) == 2  # la lecture n'étend pas la colonne


def test_negative_row_is_an_error():
    with pytest.raises(IndexError):
        Column("A", [1.0]).get(-1)


def test_values_are_read_only():
    column = Column("A", [1.0, 2.0])
    with pytest.raises(ValueError):
        column.values[0] = 9.0


def test_input_sequence_is_copied_not_aliased():
    source = np.array([1.0, 2.0])
    column = Column("A", source)
    source[0] = 99.0
    assert column.get(0) == 1.0


# -- primitives de mutation ----------------------------------------------


def test_set_values_returns_the_delta_needed_to_undo():
    column = Column("A", [1.0, 2.0, 3.0])
    delta = column.set_values(1, [20.0, 30.0])

    assert list(column.values) == [1.0, 20.0, 30.0]
    assert delta.start == 1
    assert list(delta.previous) == [2.0, 3.0]
    assert delta.previous_length == 3

    # rejouer le delta restaure l'état initial, sans snapshot complet
    column.set_values(delta.start, delta.previous)
    assert list(column.values) == [1.0, 2.0, 3.0]


def test_set_values_beyond_the_end_extends_with_nan():
    column = Column("A", [1.0])
    delta = column.set_values(3, [4.0])

    assert len(column) == 4
    assert np.isnan(column.get(1)) and np.isnan(column.get(2))
    assert column.get(3) == 4.0
    assert delta.previous.size == 0
    assert delta.previous_length == 1


def test_set_values_rejects_negative_start():
    with pytest.raises(IndexError):
        Column("A").set_values(-1, [1.0])


def test_set_length_truncates_and_returns_removed_tail():
    column = Column("A", [1.0, 2.0, 3.0])
    delta = column.set_length(1)
    assert list(column.values) == [1.0]
    assert list(delta.previous) == [2.0, 3.0]
    assert delta.previous_length == 3


def test_set_length_extends_with_nan():
    column = Column("A", [1.0])
    column.set_length(3)
    assert len(column) == 3
    assert np.isnan(column.values[1:]).all()


def test_set_length_unchanged_is_a_noop():
    column = Column("A", [1.0, 2.0])
    delta = column.set_length(2)
    assert delta.previous.size == 0
    assert len(column) == 2


def test_set_length_rejects_negative():
    with pytest.raises(ValueError):
        Column("A").set_length(-1)


def test_insert_and_delete_rows():
    column = Column("A", [1.0, 2.0, 3.0])
    column.insert_rows(1, 2)
    assert len(column) == 5
    assert np.isnan(column.get(1)) and np.isnan(column.get(2))
    assert column.get(3) == 2.0

    removed = column.delete_rows(1, 2)
    assert np.isnan(removed).all()
    assert list(column.values) == [1.0, 2.0, 3.0]


def test_delete_rows_clamps_at_the_end():
    column = Column("A", [1.0, 2.0])
    removed = column.delete_rows(1, 99)
    assert list(removed) == [2.0]
    assert list(column.values) == [1.0]


def test_row_operations_reject_negative_counts():
    column = Column("A", [1.0])
    with pytest.raises(ValueError):
        column.insert_rows(0, -1)
    with pytest.raises(ValueError):
        column.delete_rows(0, -1)


def test_out_of_range_row_index_raises():
    with pytest.raises(IndexError):
        Column("A", [1.0]).insert_rows(5, 1)


def test_set_metadata_returns_previous_values():
    column = Column("A", unit="s")
    previous = column.set_metadata(unit="ms", role=ColumnRole.X)
    assert column.unit == "ms"
    assert column.role is ColumnRole.X
    assert previous == {"unit": "s", "role": ColumnRole.Y}


def test_set_metadata_rejects_unknown_field():
    with pytest.raises(AttributeError):
        Column("A").set_metadata(couleur="rouge")


# -- colonnes texte -------------------------------------------------------


def test_text_column_uses_none_as_missing_value():
    column = Column("Éch", ["a", "b"], column_type=ColumnType.TEXT)
    assert column.get(0) == "a"
    assert column.get(9) is None
    assert column.is_missing(9)
    with pytest.raises(TypeError):
        column.numeric_values()


def test_type_conversion_text_to_numeric_keeps_what_it_can():
    column = Column("A", ["1.5", "abc", "3"], column_type=ColumnType.TEXT)
    previous_type, previous_values = column.set_column_type(ColumnType.NUMERIC)

    assert previous_type is ColumnType.TEXT
    assert list(previous_values) == ["1.5", "abc", "3"]
    assert column.get(0) == 1.5
    assert np.isnan(column.get(1))
    assert column.get(2) == 3.0


def test_type_conversion_numeric_to_text_maps_nan_to_none():
    column = Column("A", [1.0, np.nan])
    column.set_column_type(ColumnType.TEXT)
    assert column.get(0) == "1.0"
    assert column.get(1) is None


def test_type_conversion_to_the_same_type_is_a_noop():
    column = Column("A", [1.0])
    previous_type, previous_values = column.set_column_type(ColumnType.NUMERIC)
    assert previous_type is ColumnType.NUMERIC
    assert list(previous_values) == [1.0]


def test_getitem_and_len_expose_the_array():
    column = Column("A", [1.0, 2.0])
    assert column[0] == 1.0
    assert list(column[:2]) == [1.0, 2.0]
    assert len(column) == 2
