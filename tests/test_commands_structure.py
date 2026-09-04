"""Commandes de structure : ajout, suppression, renommage, déplacement.

Elles enveloppent les **primitives** de conteneur (`insert_column`,
`remove_sheet`, `rename_child`, …), jamais les façades de `Project`.
"""

from __future__ import annotations

import pytest

from plotyx.commands import (
    AddColumnCommand,
    AddObjectCommand,
    DeleteColumnCommand,
    DeleteObjectCommand,
    MoveCommand,
    RenameCommand,
)
from plotyx.core import Column, Folder, Graph, StructureError, Worksheet


def test_add_column_round_trip(bus, sheet):
    column = Column("D", [1.0, 2.0])
    bus.push(AddColumnCommand(sheet, column))
    assert sheet.column_names() == ("A", "B", "D")
    assert bus.project.resolve_key(column.key) is column

    bus.undo()
    assert sheet.column_names() == ("A", "B")
    assert column.key not in bus.project.registry

    bus.redo()
    assert sheet.column_names() == ("A", "B", "D")
    assert bus.project.resolve_key(column.key) is column


def test_add_column_at_an_explicit_index(bus, sheet):
    bus.push(AddColumnCommand(sheet, Column("D"), index=0))
    assert sheet.column_names() == ("D", "A", "B")
    bus.undo()
    assert sheet.column_names() == ("A", "B")


def test_add_column_rejects_a_non_worksheet(project):
    with pytest.raises(StructureError):
        AddColumnCommand(project.root, Column("D"))


def test_a_failing_add_leaves_the_stack_untouched(bus, sheet):
    """La commande est appliquée avant d'être empilée : un échec ne l'empile pas."""
    with pytest.raises(Exception, match="existe déjà"):
        bus.push(AddColumnCommand(sheet, Column("A")))
    assert bus.count == 0
    assert not bus.can_undo


def test_delete_column_round_trip_keeps_the_registry_key(bus, sheet):
    column = sheet.column("A")
    key = column.key
    bus.push(DeleteColumnCommand(sheet, "A"))
    assert sheet.column_names() == ("B",)
    assert key not in bus.project.registry

    bus.undo()
    assert sheet.column_names() == ("A", "B")
    assert bus.project.resolve_key(key) is column


def test_delete_object_needs_a_container(project):
    with pytest.raises(StructureError, match="conteneur"):
        DeleteObjectCommand(project.root)


def test_add_and_delete_a_window(bus, project):
    graph = Graph("Graph1")
    bus.push(AddObjectCommand(project.root, graph))
    assert project.find_window("Graph1") is graph

    bus.push(DeleteObjectCommand(graph))
    assert project.find_window("Graph1") is None

    bus.undo()
    assert project.resolve_key(graph.key) is graph


def test_add_and_delete_a_sheet(bus, project):
    book = project.resolve_path("[Book1]")
    bus.push(AddObjectCommand(book, Worksheet("Sheet3")))
    assert book.sheet_names() == ("Sheet1", "Sheet2", "Sheet3")

    bus.push(DeleteObjectCommand(book.sheet("Sheet1")))
    assert book.sheet_names() == ("Sheet2", "Sheet3")

    bus.undo()
    bus.undo()
    assert book.sheet_names() == ("Sheet1", "Sheet2")


def test_rename_column_round_trip(bus, sheet):
    bus.push(RenameCommand(sheet.column("A"), "Time"))
    assert sheet.column_names() == ("Time", "B")
    bus.undo()
    assert sheet.column_names() == ("A", "B")


def test_rename_refuses_a_duplicate_without_touching_the_stack(bus, sheet):
    with pytest.raises(Exception, match="existe déjà"):
        bus.push(RenameCommand(sheet.column("A"), "B"))
    assert bus.count == 0
    assert sheet.column_names() == ("A", "B")


def test_rename_the_project_root(bus, project):
    bus.push(RenameCommand(project.root, "Autre"))
    assert project.name == "Autre"
    bus.undo()
    assert project.name == "Demo"


def test_move_reorders_inside_the_same_container(bus, sheet):
    bus.push(MoveCommand(sheet.column("B"), index=0))
    assert sheet.column_names() == ("B", "A")
    bus.undo()
    assert sheet.column_names() == ("A", "B")
    bus.redo()
    assert sheet.column_names() == ("B", "A")


def test_move_between_sheets_keeps_the_registry_key(bus, project):
    source = project.resolve_path("[Book1]Sheet1")
    target = project.resolve_path("[Book1]Sheet2")
    column = source.column("A")
    key = column.key

    bus.push(MoveCommand(column, target, 0))
    assert target.column_names() == ("A", "C")
    assert source.column_names() == ("B",)
    assert project.resolve_key(key) is column

    bus.undo()
    assert source.column_names() == ("A", "B")
    assert target.column_names() == ("C",)
    assert project.resolve_key(key) is column


def test_move_a_window_between_folders(bus, project):
    folder = project.add_folder("Sous")
    book = project.resolve_path("[Book1]")
    bus.push(MoveCommand(book, folder))
    assert project.path_of(book) == "/Sous/[Book1]"

    bus.undo()
    assert project.path_of(book) == "/[Book1]"


def test_move_validates_before_detaching(bus, project):
    """Un déplacement invalide ne doit pas laisser l'objet orphelin."""
    column = project.resolve_path("[Book1]Sheet1!A")
    with pytest.raises(StructureError):
        MoveCommand(column, project.root)
    assert column.parent is not None
    assert bus.count == 0


def test_move_refuses_a_detached_object():
    with pytest.raises(StructureError, match="conteneur"):
        MoveCommand(Folder("Orphelin"))


def test_move_refuses_an_object_outside_any_project():
    sheet = Worksheet("S")
    column = sheet.add_column("A")
    other = Worksheet("T")
    with pytest.raises(StructureError, match="projet"):
        MoveCommand(column, other)


def test_move_retains_nothing(bus, sheet):
    command = bus.push(MoveCommand(sheet.column("B"), index=0))
    assert command.retained_objects() == ()


def test_dispatch_refuses_a_non_container(project):
    from plotyx.commands.dispatch import child_index, insert_primitive

    column = project.resolve_path("[Book1]Sheet1!A")
    with pytest.raises(StructureError, match="conteneur du modèle"):
        insert_primitive(column, 0, Column("D"))
    with pytest.raises(StructureError, match="pas un enfant"):
        child_index(project.root, column)


def test_structure_commands_expose_the_object_they_carry(bus, sheet):
    command = bus.push(DeleteColumnCommand(sheet, "B"))
    assert command.object is not None
    assert command.object.name == "B"
    command.release()
    assert command.object is None
