"""`MainWindow` phase 3 : projet par défaut, explorateur, MDI, undo/redo, titre.

Complète (sans les dupliquer) les tests phase 0 de `test_smoke.py`, qui
restent inchangés et doivent continuer à passer tels quels.

Chaque test ferme explicitement la fenêtre qu'il crée (voir le
commentaire de `tests/test_worksheet_view.py` sur l'accumulation de
widgets montrés jusqu'au `gc.collect()` forcé de fin de session pytest).
"""

from __future__ import annotations

import pytest
from PySide6.QtCore import QTimer
from PySide6.QtGui import QKeySequence
from PySide6.QtWidgets import QApplication, QTreeView

from plotyx.commands import AddObjectCommand, SetCellsCommand, mutation_guard
from plotyx.core.project import Project
from plotyx.core.worksheet import Worksheet
from plotyx.ui.main_window import MainWindow
from plotyx.ui.worksheet_view import WorksheetView


@pytest.fixture
def window(qtbot):
    win = MainWindow()
    qtbot.addWidget(win)
    win.show()
    qtbot.waitExposed(win)
    yield win
    win.close()
    qtbot.wait(0)


def test_default_project_opens_with_a_visible_worksheet(window):
    assert len(window.mdi_area.subWindowList()) == 1
    view = window.mdi_area.subWindowList()[0].widget()
    assert isinstance(view, WorksheetView)


def test_project_explorer_dock_holds_a_tree_view_on_the_project(window):
    tree = window.findChild(QTreeView, "projectExplorerTree")
    assert tree is not None
    assert tree.model() is window._project_tree_model


def test_double_clicking_a_worksheet_in_the_explorer_opens_it(window):
    book = window.project.resolve_path("[Book1]")
    with mutation_guard():
        window.bus.push(AddObjectCommand(book, Worksheet("Sheet2")))
    sheet = book.sheet("Sheet2")
    before = len(window.mdi_area.subWindowList())
    window.open_worksheet(sheet)
    assert len(window.mdi_area.subWindowList()) == before + 1
    window.open_worksheet(sheet)  # deuxième appel : ne rouvre pas de doublon
    assert len(window.mdi_area.subWindowList()) == before + 1


def test_explorer_double_click_dispatches_by_node_type(window):
    tree_model = window._project_tree_model
    book_index = tree_model.index(0, 0)  # Book1
    before = len(window.mdi_area.subWindowList())
    window._on_explorer_double_clicked(book_index)  # classeur → sa 1ère feuille
    assert len(window.mdi_area.subWindowList()) == before

    sheet_index = tree_model.index(0, 0, book_index)
    window._on_explorer_double_clicked(sheet_index)
    assert len(window.mdi_area.subWindowList()) == before  # déjà ouverte


def test_undo_redo_actions_use_standard_key_shortcuts(window):
    assert window._undo_action.shortcut() == QKeySequence(QKeySequence.StandardKey.Undo)
    assert window._redo_action.shortcut() == QKeySequence(QKeySequence.StandardKey.Redo)


def test_window_title_gets_an_asterisk_when_modified(window):
    assert window.windowTitle() == "PlotYx"
    sheet = window._first_worksheet()
    with mutation_guard():
        window.bus.push(SetCellsCommand(sheet.column("A"), 0, [1.0]))
    assert window.windowTitle() == "PlotYx*"
    window.bus.undo()
    assert window.windowTitle() == "PlotYx"


def test_undo_action_actually_undoes_a_cell_edit(window):
    sheet = window._first_worksheet()
    column = sheet.column(0)
    with mutation_guard():
        window.bus.push(SetCellsCommand(column, 0, [5.0]))
    assert column.values[0] == 5.0
    window._undo_action.trigger()
    assert len(column) == 0 or column.get(0) != 5.0


def test_constructing_with_an_explicit_project_uses_it(qtbot):
    project = Project("Explicit")
    book = project.add_workbook("Book1")
    book.sheet("Sheet1").add_column("X", [1.0])
    win = MainWindow(project)
    qtbot.addWidget(win)
    try:
        assert win.project is project
    finally:
        win.close()
        qtbot.wait(0)


def _trigger_and_close(text_substring):
    def _do():
        popup = QApplication.activePopupWidget()
        assert popup is not None, "aucun menu contextuel ouvert"
        for action in popup.actions():
            if text_substring in action.text():
                action.trigger()
                popup.close()
                return
        popup.close()
        raise AssertionError(f"aucune action ne contient {text_substring!r}")

    QTimer.singleShot(0, _do)


def test_explorer_context_menu_add_sheet(window):
    tree_model = window._project_tree_model
    book_index = tree_model.index(0, 0)
    pos = window._project_tree.visualRect(book_index).center()
    book = tree_model.node_at(book_index)
    before = len(book.sheets)
    _trigger_and_close("Nouvelle feuille")
    with mutation_guard():
        window._on_explorer_context_menu(pos)
    assert len(book.sheets) == before + 1


def test_explorer_context_menu_delete(window):
    from plotyx.core.nodes import Folder

    folder = Folder("Jetable")
    with mutation_guard():
        window.bus.push(AddObjectCommand(window.project.root, folder))
    tree_model = window._project_tree_model
    folder_row = window.project.root.children().index(folder)
    folder_index = tree_model.index(folder_row, 0)
    pos = window._project_tree.visualRect(folder_index).center()
    before = len(window.project.root.children())
    _trigger_and_close("Supprimer")
    with mutation_guard():
        window._on_explorer_context_menu(pos)
    assert len(window.project.root.children()) == before - 1


def test_explorer_context_menu_rename(window, monkeypatch):
    from PySide6.QtWidgets import QInputDialog

    tree_model = window._project_tree_model
    book_index = tree_model.index(0, 0)
    pos = window._project_tree.visualRect(book_index).center()
    monkeypatch.setattr(
        QInputDialog, "getText", staticmethod(lambda *a, **k: ("BookRenamed", True))
    )
    _trigger_and_close("Renommer")
    with mutation_guard():
        window._on_explorer_context_menu(pos)
    assert window.project.find_window("BookRenamed") is not None


def test_explorer_context_menu_on_empty_area_does_nothing(window):
    from PySide6.QtCore import QPoint

    before = len(window.project.root.children())
    window._on_explorer_context_menu(QPoint(-10, -10))  # hors de tout item
    assert len(window.project.root.children()) == before
