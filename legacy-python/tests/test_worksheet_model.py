"""`WorksheetTableModel` : lecture sans copie, écriture par commande, adaptateur.

Trois axes, dans l'esprit du reste de la suite :

* la règle d'or — `setData()` ne mute jamais `core` en direct, vérifié sous
  `mutation_guard()` ;
* les colonnes de longueur variable — une cellule au-delà de la longueur
  d'une colonne est vide, un `NaN` stocké dans la plage reste visible ;
* l'appariement `begin*/end*` — colonnes et lignes insérées/supprimées par
  une commande se reflètent dans le modèle Qt sans corrompre les compteurs.
"""

from __future__ import annotations

import numpy as np
import pytest
from PySide6.QtCore import QModelIndex, Qt

from plotyx.commands import AddColumnCommand, DeleteColumnCommand, SetCellsCommand, mutation_guard
from plotyx.core import Column
from plotyx.ui.worksheet_model import HEADER_LINES_ROLE, WorksheetTableModel


@pytest.fixture
def model(bus, sheet):
    m = WorksheetTableModel(sheet, bus)
    yield m
    m.disconnect_signals()


def test_row_and_column_counts_match_the_sheet(model, sheet):
    assert model.rowCount() == sheet.row_count == 4
    assert model.columnCount() == sheet.column_count == 2
    assert model.rowCount(model.index(0, 0)) == 0  # pas d'enfants sous une cellule


def test_cell_beyond_column_length_is_empty_not_nan(model, sheet):
    # B n'a que 2 valeurs, la feuille a 4 lignes (à cause de A) : la ligne 3
    # de B est hors de sa longueur propre, donc vide.
    assert len(sheet.column("B")) == 2
    index = model.index(3, 1)
    assert model.data(index, Qt.ItemDataRole.DisplayRole) == ""
    assert model.data(index, Qt.ItemDataRole.EditRole) is None


def test_a_stored_nan_is_shown_not_hidden(model, sheet):
    # A[2] est un NaN réellement stocké (fixture conftest), donc dans la
    # longueur de la colonne : il doit s'afficher, contrairement à la ligne
    # hors bornes ci-dessus.
    assert np.isnan(sheet.column("A").values[2])
    index = model.index(2, 0)
    assert model.data(index, Qt.ItemDataRole.DisplayRole) == "nan"


def test_header_data_rich_lines_expose_name_unit_and_long_name(model, sheet):
    lines = model.headerData(0, Qt.Orientation.Horizontal, HEADER_LINES_ROLE)
    assert lines[0].startswith("A")
    assert "(s)" in lines[1]
    assert model.headerData(0, Qt.Orientation.Horizontal) == "A"
    assert model.headerData(0, Qt.Orientation.Vertical) == "1"  # 1-based, façon tableur
    assert model.headerData(1, Qt.Orientation.Vertical) == "2"


def test_flags_are_editable_and_selectable(model):
    index = model.index(0, 0)
    flags = model.flags(index)
    assert flags & Qt.ItemFlag.ItemIsEditable
    assert flags & Qt.ItemFlag.ItemIsSelectable
    assert model.flags(QModelIndex()) == Qt.ItemFlag.NoItemFlags


# -- écriture : uniquement via le bus ---------------------------------------


def test_set_data_pushes_a_command_rather_than_mutating_directly(model, bus, sheet):
    with mutation_guard():
        ok = model.setData(model.index(0, 0), "9.5")
    assert ok
    assert bus.count == 1
    assert sheet.column("A").values[0] == 9.5
    assert bus.can_undo


def test_set_data_with_unparseable_text_is_refused_without_pushing(model, bus):
    ok = model.setData(model.index(0, 0), "pas un nombre")
    assert not ok
    assert bus.count == 0


def test_set_data_empty_text_writes_missing_value(model, bus, sheet):
    ok = model.setData(model.index(0, 1), "")
    assert ok
    assert np.isnan(sheet.column("B").values[0])


def test_undo_redo_are_reflected_through_the_model(model, bus, sheet):
    model.setData(model.index(0, 0), "42.0")
    assert model.data(model.index(0, 0)) == "42.0"
    bus.undo()
    assert sheet.column("A").values[0] == 1.0
    bus.redo()
    assert sheet.column("A").values[0] == 42.0


# -- adaptateur de notifications : appariement begin*/end* -------------------


def test_extending_a_column_via_a_command_grows_row_count(model, bus, sheet):
    assert model.rowCount() == 4
    bus.push(SetCellsCommand(sheet.column("B"), 5, [1.0]))  # étend B à 6 lignes
    assert sheet.row_count == 6
    assert model.rowCount() == 6


def test_undo_of_an_extension_shrinks_row_count_back(model, bus, sheet):
    bus.push(SetCellsCommand(sheet.column("B"), 5, [1.0]))
    assert model.rowCount() == 6
    bus.undo()
    assert model.rowCount() == 4


def test_adding_a_column_via_a_command_inserts_it_in_the_model(qtbot, model, bus, sheet):
    with qtbot.waitSignal(model.columnsInserted, timeout=1000):
        bus.push(AddColumnCommand(sheet, Column("D", [1.0, 2.0])))
    assert model.columnCount() == 3
    assert model.headerData(2, Qt.Orientation.Horizontal) == "D"


def test_removing_a_column_via_a_command_removes_it_from_the_model(qtbot, model, bus, sheet):
    with qtbot.waitSignal(model.columnsRemoved, timeout=1000):
        bus.push(DeleteColumnCommand(sheet, "B"))
    assert model.columnCount() == 1
    assert model.headerData(0, Qt.Orientation.Horizontal) == "A"


def test_renaming_a_column_emits_header_data_changed(qtbot, model, bus, sheet):
    from plotyx.commands import RenameCommand

    with qtbot.waitSignal(model.headerDataChanged, timeout=1000):
        bus.push(RenameCommand(sheet.column("B"), "Bis"))
    assert model.headerData(1, Qt.Orientation.Horizontal) == "Bis"


def test_column_metadata_change_emits_header_data_changed(qtbot, model, bus, sheet):
    from plotyx.commands import SetColumnMetadataCommand

    with qtbot.waitSignal(model.headerDataChanged, timeout=1000):
        bus.push(SetColumnMetadataCommand(sheet.column("A"), unit="ms"))
    lines = model.headerData(0, Qt.Orientation.Horizontal, HEADER_LINES_ROLE)
    assert "(ms)" in lines[1]


def test_disconnecting_stops_the_model_from_reacting(bus, sheet):
    m = WorksheetTableModel(sheet, bus)
    m.disconnect_signals()
    bus.push(SetCellsCommand(sheet.column("B"), 5, [1.0]))
    # Le modèle n'a plus été notifié : son compteur de lignes est resté figé.
    assert m.rowCount() == 4
    assert sheet.row_count == 6


def test_a_second_model_never_mutates_core_directly_end_to_end(bus, sheet):
    """Un scénario complet (édition, ajout, suppression) reste sous le radar
    du garde-fou : tout passe par le bus."""
    model = WorksheetTableModel(sheet, bus)
    try:
        with mutation_guard():
            model.setData(model.index(0, 0), "1.0")
            bus.push(AddColumnCommand(sheet, Column("Z", [1.0])))
            bus.push(DeleteColumnCommand(sheet, "Z"))
            bus.undo()
            bus.undo()
            bus.redo()
    finally:
        model.disconnect_signals()
