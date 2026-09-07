"""`ProjectTreeModel` : arbre Qt sur `Project`, structure vue sans copie."""

from __future__ import annotations

import pytest
from PySide6.QtCore import QModelIndex, Qt

from plotyx.commands import AddObjectCommand, CommandBus, DeleteObjectCommand, mutation_guard
from plotyx.core.nodes import Folder, Workbook
from plotyx.core.worksheet import Worksheet
from plotyx.ui.project_tree_model import ProjectTreeModel


@pytest.fixture
def tree_model(project):
    model = ProjectTreeModel(project)
    yield model
    model.disconnect_signals()


def test_top_level_rows_are_the_root_folder_children_not_the_root_itself(tree_model, project):
    assert tree_model.rowCount() == len(project.root.children())
    first = tree_model.index(0, 0)
    assert tree_model.node_at(first) is project.root.children()[0]


def test_workbook_children_are_its_worksheets(tree_model, project):
    book_index = tree_model.index(0, 0)
    assert isinstance(tree_model.node_at(book_index), Workbook)
    assert tree_model.rowCount(book_index) == 2  # Sheet1, Sheet2 (fixture conftest)


def test_worksheet_children_are_its_columns(tree_model, project):
    book_index = tree_model.index(0, 0)
    sheet_index = tree_model.index(0, 0, book_index)
    assert tree_model.rowCount(sheet_index) == 2  # colonnes A, B


def test_parent_of_a_column_is_its_worksheet(tree_model):
    book_index = tree_model.index(0, 0)
    sheet_index = tree_model.index(0, 0, book_index)
    column_index = tree_model.index(0, 0, sheet_index)
    assert tree_model.parent(column_index) == sheet_index


def test_parent_of_a_top_level_node_is_invalid(tree_model):
    book_index = tree_model.index(0, 0)
    assert tree_model.parent(book_index) == QModelIndex()


def test_data_display_role_shows_the_name(tree_model, project):
    book_index = tree_model.index(0, 0)
    assert tree_model.data(book_index, Qt.ItemDataRole.DisplayRole) == "Book1"


def test_adding_a_workbook_via_a_command_inserts_a_row(qtbot, project, tree_model):
    bus = CommandBus(project)
    before = tree_model.rowCount()
    with qtbot.waitSignal(tree_model.modelReset, timeout=1000), mutation_guard():
        bus.push(AddObjectCommand(project.root, Workbook("Book9", sheets=[])))
    assert tree_model.rowCount() == before + 1


def test_removing_a_folder_via_a_command_removes_a_row(qtbot, project, tree_model):
    bus = CommandBus(project)
    folder = Folder("ARemoveTest")
    with mutation_guard():
        bus.push(AddObjectCommand(project.root, folder))
    before = tree_model.rowCount()
    with qtbot.waitSignal(tree_model.modelReset, timeout=1000), mutation_guard():
        bus.push(DeleteObjectCommand(folder))
    assert tree_model.rowCount() == before - 1


def test_adding_a_worksheet_to_a_workbook_is_visible_under_it(qtbot, project, tree_model):
    bus = CommandBus(project)
    book_index = tree_model.index(0, 0)
    before = tree_model.rowCount(book_index)
    with mutation_guard():
        bus.push(AddObjectCommand(project.resolve_path("[Book1]"), Worksheet("SheetX")))
    assert tree_model.rowCount(book_index) == before + 1


def test_renaming_a_node_emits_data_changed(qtbot, project, tree_model):
    from plotyx.commands import RenameCommand

    bus = CommandBus(project)
    book_index = tree_model.index(0, 0)
    with qtbot.waitSignal(tree_model.dataChanged, timeout=1000), mutation_guard():
        bus.push(RenameCommand(project.resolve_path("[Book1]"), "BookRenamed"))
    assert tree_model.data(book_index, Qt.ItemDataRole.DisplayRole) == "BookRenamed"


def test_column_metadata_change_emits_data_changed(qtbot, project, tree_model):
    from plotyx.commands import SetColumnMetadataCommand

    bus = CommandBus(project)
    book_index = tree_model.index(0, 0)
    sheet_index = tree_model.index(0, 0, book_index)
    column = project.resolve_path("[Book1]Sheet1!A")
    with qtbot.waitSignal(tree_model.dataChanged, timeout=1000), mutation_guard():
        bus.push(SetColumnMetadataCommand(column, role="Y"))
    column_index = tree_model.index(0, 0, sheet_index)
    assert "[Y]" in tree_model.data(column_index, Qt.ItemDataRole.DisplayRole)


def test_flags_and_header_data(tree_model):
    assert tree_model.flags(QModelIndex()) == Qt.ItemFlag.NoItemFlags
    book_index = tree_model.index(0, 0)
    assert tree_model.flags(book_index) & Qt.ItemFlag.ItemIsSelectable
    assert tree_model.headerData(0, Qt.Orientation.Horizontal) == "Projet"


def test_a_full_edit_scenario_never_mutates_core_directly(project, tree_model):
    bus = CommandBus(project)
    with mutation_guard():
        book = project.resolve_path("[Book1]")
        bus.push(AddObjectCommand(book, Worksheet("SheetY")))
        bus.push(DeleteObjectCommand(book.sheet("SheetY")))
        bus.undo()
        bus.redo()
