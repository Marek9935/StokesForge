"""Fenêtre principale de PlotYx.

Phase 3 : la fenêtre porte désormais un `Project` et le `CommandBus` qui va
avec. Le dock « Project Explorer » est rempli (arbre du projet, double-clic
pour ouvrir une feuille) ; les docks « Properties » et « Script Console »
restent des stubs, hors périmètre de cette phase (6/9). Les menus
Undo/Redo utilisent `QKeySequence.StandardKey`, jamais un raccourci en
dur — voir la Politique macOS de la roadmap, qui s'applique dès
maintenant même si macOS est hors release.
"""

from __future__ import annotations

from PySide6.QtCore import Qt
from PySide6.QtGui import QAction, QKeySequence
from PySide6.QtWidgets import (
    QDockWidget,
    QLabel,
    QMainWindow,
    QMdiArea,
    QMdiSubWindow,
    QMessageBox,
    QToolBar,
    QTreeView,
)

from plotyx.commands import AddObjectCommand, CommandBus, DeleteObjectCommand, RenameCommand
from plotyx.core.nodes import Workbook
from plotyx.core.project import Project
from plotyx.core.worksheet import Worksheet
from plotyx.ui.project_tree_model import ProjectTreeModel
from plotyx.ui.worksheet_model import WorksheetTableModel
from plotyx.ui.worksheet_view import WorksheetView

__all__ = ["MainWindow"]


def _make_stub_dock(title: str, object_name: str, placeholder_text: str) -> QDockWidget:
    """Crée un dock stub : juste un titre et un texte de substitution.

    Encore utilisé par « Properties » et « Script Console », hors périmètre
    de la phase 3 (respectivement phases 6/8 et 9).
    """
    dock = QDockWidget(title)
    dock.setObjectName(object_name)
    label = QLabel(placeholder_text)
    label.setAlignment(Qt.AlignmentFlag.AlignCenter)
    label.setEnabled(False)
    dock.setWidget(label)
    return dock


def _next_sheet_name(book: Workbook) -> str:
    """Prochain nom de feuille libre (`Sheet1`, `Sheet2`, ...), sans toucher
    à l'attribut privé de `Workbook` — la logique est petite, on la duplique
    plutôt que de percer l'encapsulation.
    """
    taken = set(book.sheet_names())
    index = 1
    while f"Sheet{index}" in taken:
        index += 1
    return f"Sheet{index}"


def _default_project() -> Project:
    """Projet de démonstration : un classeur, une feuille, quelques colonnes.

    Construit hors de tout `CommandBus` — c'est la construction du projet
    lui-même, pas une mutation qu'une commande devrait pouvoir annuler (il
    n'y a encore rien à annuler vers). Une fois la fenêtre affichée, toute
    mutation ultérieure passe par le bus.
    """
    project = Project("Project1")
    book = Workbook("Book1")
    project.root.add_child(book)
    sheet = book.sheet("Sheet1")
    sheet.add_column("A", [], role="X", unit="s", long_name="Temps")
    sheet.add_column("B", [], role="Y", unit="V", long_name="Tension")
    return project


class MainWindow(QMainWindow):
    """Fenêtre principale : zone MDI centrale + docks ancrables."""

    def __init__(self, project: Project | None = None) -> None:
        super().__init__()
        self.setObjectName("PlotYxMainWindow")
        self._base_title = "PlotYx"
        self.setWindowTitle(self._base_title)
        self.resize(1200, 800)

        self._project = project if project is not None else _default_project()
        self._bus = CommandBus(self._project, undo_limit=1000, parent=self)
        self._bus.modified_changed.connect(self._update_title)

        self._mdi_area = QMdiArea(self)
        self.setCentralWidget(self._mdi_area)
        self._open_sheets: dict[str, QMdiSubWindow] = {}

        self._build_project_explorer_dock()
        self._properties_dock = _make_stub_dock(
            "Properties", "propertiesDock", "(à venir — phase 3/6)"
        )
        self._script_console_dock = _make_stub_dock(
            "Script Console", "scriptConsoleDock", "(à venir — phase 9)"
        )

        self.addDockWidget(Qt.DockWidgetArea.LeftDockWidgetArea, self._project_explorer_dock)
        self.addDockWidget(Qt.DockWidgetArea.RightDockWidgetArea, self._properties_dock)
        self.addDockWidget(Qt.DockWidgetArea.BottomDockWidgetArea, self._script_console_dock)

        self._build_actions()
        self._build_menus()
        self._build_toolbar()
        self._update_title(self._bus.is_modified)

        # Ouvre la première feuille trouvée, pour que l'appli affiche
        # immédiatement une table plutôt qu'une zone MDI vide.
        first_sheet = self._first_worksheet()
        if first_sheet is not None:
            self.open_worksheet(first_sheet)

    # -- accès ---------------------------------------------------------------

    @property
    def mdi_area(self) -> QMdiArea:
        return self._mdi_area

    @property
    def project(self) -> Project:
        return self._project

    @property
    def bus(self) -> CommandBus:
        return self._bus

    def _first_worksheet(self) -> Worksheet | None:
        for node in self._project.walk():
            if isinstance(node, Worksheet):
                return node
        return None

    # -- Project Explorer ------------------------------------------------------

    def _build_project_explorer_dock(self) -> None:
        self._project_tree_model = ProjectTreeModel(self._project, self)
        tree = QTreeView()
        tree.setObjectName("projectExplorerTree")
        tree.setModel(self._project_tree_model)
        tree.setHeaderHidden(True)
        tree.doubleClicked.connect(self._on_explorer_double_clicked)
        tree.setContextMenuPolicy(Qt.ContextMenuPolicy.CustomContextMenu)
        tree.customContextMenuRequested.connect(self._on_explorer_context_menu)
        self._project_tree = tree

        dock = QDockWidget("Project Explorer")
        dock.setObjectName("projectExplorerDock")
        dock.setWidget(tree)
        self._project_explorer_dock = dock

    def _on_explorer_double_clicked(self, index) -> None:
        node = self._project_tree_model.node_at(index)
        if isinstance(node, Worksheet):
            self.open_worksheet(node)
        elif isinstance(node, Workbook) and node.sheets:
            self.open_worksheet(node.sheets[0])

    def _on_explorer_context_menu(self, pos) -> None:
        from PySide6.QtWidgets import QMenu

        index = self._project_tree.indexAt(pos)
        node = self._project_tree_model.node_at(index)
        if node is None:
            return
        menu = QMenu()
        act_rename = menu.addAction("Renommer…")
        act_rename.triggered.connect(lambda: self._rename_node(node))
        act_delete = menu.addAction("Supprimer")
        act_delete.triggered.connect(lambda: self._delete_node(node))
        if isinstance(node, Workbook):
            act_add_sheet = menu.addAction("Nouvelle feuille")
            act_add_sheet.triggered.connect(lambda: self._add_sheet(node))
        menu.exec(self._project_tree.viewport().mapToGlobal(pos))

    def _rename_node(self, node) -> None:
        from PySide6.QtWidgets import QInputDialog

        name, ok = QInputDialog.getText(self, "Renommer", "Nouveau nom :", text=node.name)
        if not (ok and name.strip()):
            return
        try:
            self._bus.push(RenameCommand(node, name.strip()))
        except Exception as error:  # noqa: BLE001 - remonté à l'utilisateur
            QMessageBox.warning(self, "Renommage impossible", str(error))

    def _delete_node(self, node) -> None:
        try:
            self._bus.push(DeleteObjectCommand(node))
        except Exception as error:  # noqa: BLE001 - remonté à l'utilisateur
            QMessageBox.warning(self, "Suppression impossible", str(error))

    def _add_sheet(self, book: Workbook) -> None:
        self._bus.push(AddObjectCommand(book, Worksheet(_next_sheet_name(book))))

    # -- fenêtres MDI ---------------------------------------------------------

    def open_worksheet(self, sheet: Worksheet) -> QMdiSubWindow:
        """Ouvre (ou ramène au premier plan) la sous-fenêtre d'une feuille."""
        key = sheet.key
        existing = self._open_sheets.get(key)
        if existing is not None:
            self._mdi_area.setActiveSubWindow(existing)
            return existing

        model = WorksheetTableModel(sheet, self._bus, self)
        view = WorksheetView(model)
        sub_window = self._mdi_area.addSubWindow(view)
        sub_window.setWindowTitle(self._project.path_of(sheet))
        sub_window.setAttribute(Qt.WidgetAttribute.WA_DeleteOnClose)
        self._open_sheets[key] = sub_window

        def _on_destroyed(_obj=None, model=model, key=key) -> None:
            model.disconnect_signals()
            self._open_sheets.pop(key, None)

        sub_window.destroyed.connect(_on_destroyed)
        sub_window.show()
        return sub_window

    # -- actions / menus / toolbar ---------------------------------------------

    def _build_actions(self) -> None:
        self._undo_action: QAction = self._bus.stack.createUndoAction(self, "Annuler")
        self._undo_action.setShortcut(QKeySequence(QKeySequence.StandardKey.Undo))
        self._redo_action: QAction = self._bus.stack.createRedoAction(self, "Rétablir")
        self._redo_action.setShortcut(QKeySequence(QKeySequence.StandardKey.Redo))

    def _build_menus(self) -> None:
        edit_menu = self.menuBar().addMenu("&Édition")
        edit_menu.addAction(self._undo_action)
        edit_menu.addAction(self._redo_action)

        view_menu = self.menuBar().addMenu("&Affichage")
        view_menu.addAction(self._project_explorer_dock.toggleViewAction())
        view_menu.addAction(self._properties_dock.toggleViewAction())
        view_menu.addAction(self._script_console_dock.toggleViewAction())

    def _build_toolbar(self) -> None:
        toolbar = QToolBar("Principale", self)
        toolbar.setObjectName("mainToolBar")
        toolbar.addAction(self._undo_action)
        toolbar.addAction(self._redo_action)
        self.addToolBar(toolbar)

    # -- état « document modifié » ---------------------------------------------

    def _update_title(self, modified: bool) -> None:
        # `test_main_window_opens_and_closes` (phase 0) exige le titre exact
        # "PlotYx" à l'état propre : l'astérisque est le seul ajout.
        self.setWindowTitle(f"{self._base_title}*" if modified else self._base_title)
