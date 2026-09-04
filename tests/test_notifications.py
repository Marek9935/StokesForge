"""Notifications du modèle : `dataChanged` / `structureChanged`.

Le modèle émet des signaux **Python purs** qui remontent l'arbre jusqu'au
projet : la vue pourra s'abonner au niveau qui l'arrange (une colonne, une
feuille, ou le projet entier). La phase 3 branchera un pont Qt là-dessus.
"""

from __future__ import annotations

import pytest

from plotyx.core import (
    Column,
    ColumnRole,
    ColumnType,
    DataChanged,
    DuplicateNameError,
    Project,
    StructureChanged,
)


class Recorder:
    """Collecte les événements reçus, dans l'ordre."""

    def __init__(self) -> None:
        self.data: list[DataChanged] = []
        self.structure: list[StructureChanged] = []

    def kinds(self) -> list[str]:
        return [event.kind for event in self.structure]


@pytest.fixture
def fixture():
    project = Project("Demo")
    book = project.add_workbook("Book1")
    sheet = book.sheet("Sheet1")
    sheet.add_column("A", [1.0, 2.0, 3.0])
    recorder = Recorder()
    project.observe(on_data=recorder.data.append, on_structure=recorder.structure.append)
    return project, sheet, recorder


# -- dataChanged ----------------------------------------------------------


def test_set_values_emits_one_data_event_with_the_touched_range(fixture):
    project, sheet, recorder = fixture

    sheet.set_values("A", 1, [20.0, 30.0])

    assert len(recorder.data) == 1
    event = recorder.data[0]
    assert event.source is sheet.column("A")
    assert (event.start, event.stop) == (1, 3)
    assert not event.length_changed
    assert recorder.structure == []


def test_extending_a_column_flags_the_length_change(fixture):
    project, sheet, recorder = fixture

    sheet.set_cell("A", 10, 1.0)

    assert len(recorder.data) == 1
    assert recorder.data[0].length_changed


def test_row_insertion_emits_data_then_one_structure_event(fixture):
    project, sheet, recorder = fixture

    sheet.insert_rows(0, 2)

    assert len(recorder.data) == 1  # une colonne dans la feuille
    assert recorder.kinds() == ["rows_inserted"]
    assert recorder.structure[0].details == {"at": 0, "count": 2}


def test_row_deletion_emits_data_then_one_structure_event(fixture):
    project, sheet, recorder = fixture

    sheet.delete_rows(0, 1)

    assert len(recorder.data) == 1
    assert recorder.kinds() == ["rows_deleted"]


def test_set_length_emits_a_data_event(fixture):
    project, sheet, recorder = fixture
    sheet.column("A").set_length(1)
    assert recorder.data[0].length_changed


# -- structureChanged -----------------------------------------------------


def test_column_lifecycle_events(fixture):
    project, sheet, recorder = fixture

    column = sheet.add_column("B", [1.0])
    sheet.move_column("B", 0)
    sheet.rename_column("B", "C")
    sheet.remove_column("C")

    assert recorder.kinds() == [
        "column_inserted",
        "column_moved",
        "node_renamed",
        "column_removed",
    ]
    assert recorder.structure[0].details["column"] is column
    assert recorder.structure[1].details == {"from_index": 1, "to_index": 0}
    assert recorder.structure[2].details == {"previous": "B", "name": "C"}
    assert recorder.structure[3].details["index"] == 0


def test_metadata_and_type_changes_are_structural(fixture):
    project, sheet, recorder = fixture
    column = sheet.column("A")

    column.set_metadata(unit="s", role=ColumnRole.X)
    assert recorder.kinds() == ["column_metadata_changed"]
    assert recorder.structure[0].details["fields"]["unit"] == "s"

    column.set_column_type(ColumnType.TEXT)
    assert recorder.kinds()[-1] == "column_type_changed"
    assert recorder.data[-1].stop == 3  # la conversion réécrit toute la colonne


def test_tree_events_reach_the_project(fixture):
    project, sheet, recorder = fixture

    folder = project.add_folder("Manips")
    book = project.find_window("Book1")
    project.move(book, folder)
    project.rename(book, "Mesures")

    assert recorder.kinds() == [
        "child_inserted",  # le dossier
        "child_removed",  # départ du classeur
        "child_inserted",  # arrivée dans le dossier
        "node_renamed",
    ]


def test_a_move_within_a_workbook_emits_sheet_events(fixture):
    project, sheet, recorder = fixture
    book = project.find_window("Book1")
    book.add_sheet("Sheet2")
    book.move_sheet("Sheet2", 0)
    book.rename_sheet("Sheet2", "Autre")
    book.remove_sheet("Autre")
    assert recorder.kinds() == [
        "sheet_inserted",
        "sheet_moved",
        "node_renamed",
        "sheet_removed",
    ]


# -- propagation ----------------------------------------------------------


def test_the_same_event_reaches_every_level_of_the_tree(fixture):
    project, sheet, recorder = fixture
    column = sheet.column("A")
    book = project.find_window("Book1")

    seen: dict[str, list[DataChanged]] = {}
    for label, node in [("column", column), ("sheet", sheet), ("book", book)]:
        seen[label] = []
        node.data_changed.connect(seen[label].append)

    column.set_values(0, [9.0])

    assert [len(events) for events in seen.values()] == [1, 1, 1]
    assert seen["column"][0] is seen["book"][0] is recorder.data[0]


def test_a_detached_object_notifies_only_itself():
    column = Column("A", [1.0])
    received = []
    column.data_changed.connect(received.append)
    column.set_values(0, [2.0])
    assert len(received) == 1


def test_a_removed_column_stops_notifying_the_project(fixture):
    project, sheet, recorder = fixture
    column = sheet.column("A")
    sheet.remove_column(column)
    recorder.data.clear()

    column.set_values(0, [9.0])

    assert recorder.data == []


def test_unsubscribing_stops_delivery(fixture):
    project, sheet, recorder = fixture
    received = []
    connection = project.data_changed.connect(received.append)

    sheet.set_cell("A", 0, 1.0)
    connection.disconnect()
    sheet.set_cell("A", 0, 2.0)

    assert len(received) == 1
    assert len(recorder.data) == 2  # l'autre abonné reçoit toujours


def test_observe_without_callbacks_subscribes_nothing():
    project = Project()
    assert project.observe() == []


def test_a_failed_mutation_emits_nothing(fixture):
    project, sheet, recorder = fixture
    with pytest.raises(DuplicateNameError):
        sheet.add_column("A")  # nom déjà pris
    assert recorder.structure == []
    assert recorder.data == []
