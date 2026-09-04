"""Fenêtre principale de PlotYx.

Phase 3 : la fenêtre porte un `Project` et le `CommandBus` qui va avec. Le
dock « Project Explorer » est rempli (arbre du projet, double-clic pour
ouvrir une feuille) ; les docks « Properties » et « Script Console »
restent des stubs, hors périmètre de cette phase (6/9). Les menus
Undo/Redo utilisent `QKeySequence.StandardKey`, jamais un raccourci en
dur — voir la Politique macOS de la roadmap, qui s'applique dès
maintenant même si macOS est hors release.

Phase 4 : Save / Save As / Open, prompt de sauvegarde à la fermeture,
autosave périodique et recovery au démarrage. `_replace_project` est le
point de passage unique qui échange le projet et le bus vivants — ouvrir
un fichier ou récupérer un autosave **remplace** le projet, ça ne s'empile
pas dessus (§ « la règle d'or » de la phase 4) : la pile d'undo précédente
est détruite, une neuve la remplace.
"""

from __future__ import annotations

import uuid
from pathlib import Path

from PySide6.QtCore import Qt, QTimer
from PySide6.QtGui import QAction, QKeySequence
from PySide6.QtWidgets import (
    QDockWidget,
    QFileDialog,
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
from plotyx.persistence import (
    FILE_EXTENSION,
    PersistenceError,
    SchemaVersionError,
    autosave_path,
    autosave_source,
    discard_autosave,
    list_recoverable,
    load_project,
    recover,
    save_project,
    write_autosave,
)
from plotyx.ui.project_tree_model import ProjectTreeModel
from plotyx.ui.worksheet_model import WorksheetTableModel
from plotyx.ui.worksheet_view import WorksheetView

__all__ = ["MainWindow"]

#: Fréquence d'autosave par défaut. `0` désarme le minuteur (tests).
DEFAULT_AUTOSAVE_INTERVAL_MS = 120_000

_FILE_FILTER = f"Projets PlotYx (*{FILE_EXTENSION})"


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

    def __init__(
        self,
        project: Project | None = None,
        *,
        check_recovery: bool = False,
        autosave_interval_ms: int = DEFAULT_AUTOSAVE_INTERVAL_MS,
    ) -> None:
        """`check_recovery` par défaut à `False`.

        Une session interrompue laisse un fichier d'autosave, et le
        proposer déclenche un `QMessageBox` modal — inoffensif à l'usage
        réel (`app.main()` passe `check_recovery=True`), mais une suite de
        350+ tests qui construit des `MainWindow()` à la chaîne ne doit
        jamais dépendre de ce qui traîne dans le répertoire d'autosave
        partagé de la machine. Les tests de recovery l'activent
        explicitement.
        """
        super().__init__()
        self.setObjectName("PlotYxMainWindow")
        self._base_title = "PlotYx"
        self.setWindowTitle(self._base_title)
        self.resize(1200, 800)

        self._current_path: Path | None = None
        self._session_id = uuid.uuid4().hex

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

        self._build_file_actions()
        self._build_actions()
        self._build_menus()
        self._build_toolbar()
        self._update_title(self._bus.is_modified)

        self._autosave_timer = QTimer(self)
        self._autosave_timer.setInterval(max(autosave_interval_ms, 0))
        self._autosave_timer.timeout.connect(self._on_autosave_tick)
        if autosave_interval_ms > 0:
            self._autosave_timer.start()

        # Ouvre la première feuille trouvée, pour que l'appli affiche
        # immédiatement une table plutôt qu'une zone MDI vide.
        first_sheet = self._first_worksheet()
        if first_sheet is not None:
            self.open_worksheet(first_sheet)

        if check_recovery:
            self._offer_recovery()

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

    @property
    def current_path(self) -> Path | None:
        return self._current_path

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

    def _build_file_actions(self) -> None:
        self._new_action = QAction("&Nouveau", self)
        self._new_action.triggered.connect(self._on_new)

        self._open_action = QAction("&Ouvrir…", self)
        self._open_action.setShortcut(QKeySequence(QKeySequence.StandardKey.Open))
        self._open_action.triggered.connect(self._on_open)

        self._save_action = QAction("&Enregistrer", self)
        self._save_action.setShortcut(QKeySequence(QKeySequence.StandardKey.Save))
        self._save_action.triggered.connect(self._on_save)

        self._save_as_action = QAction("Enregistrer &sous…", self)
        self._save_as_action.setShortcut(QKeySequence(QKeySequence.StandardKey.SaveAs))
        self._save_as_action.triggered.connect(self._on_save_as)

    def _build_actions(self) -> None:
        self._undo_action: QAction = self._bus.stack.createUndoAction(self, "Annuler")
        self._undo_action.setShortcut(QKeySequence(QKeySequence.StandardKey.Undo))
        self._redo_action: QAction = self._bus.stack.createRedoAction(self, "Rétablir")
        self._redo_action.setShortcut(QKeySequence(QKeySequence.StandardKey.Redo))

    def _build_menus(self) -> None:
        file_menu = self.menuBar().addMenu("&Fichier")
        file_menu.addAction(self._new_action)
        file_menu.addAction(self._open_action)
        file_menu.addAction(self._save_action)
        file_menu.addAction(self._save_as_action)
        self._file_menu = file_menu

        self._edit_menu = self.menuBar().addMenu("&Édition")
        self._edit_menu.addAction(self._undo_action)
        self._edit_menu.addAction(self._redo_action)

        view_menu = self.menuBar().addMenu("&Affichage")
        view_menu.addAction(self._project_explorer_dock.toggleViewAction())
        view_menu.addAction(self._properties_dock.toggleViewAction())
        view_menu.addAction(self._script_console_dock.toggleViewAction())

    def _build_toolbar(self) -> None:
        toolbar = QToolBar("Principale", self)
        toolbar.setObjectName("mainToolBar")
        toolbar.addAction(self._save_action)
        toolbar.addAction(self._open_action)
        toolbar.addSeparator()
        toolbar.addAction(self._undo_action)
        toolbar.addAction(self._redo_action)
        self.addToolBar(toolbar)
        self._toolbar = toolbar

    # -- état « document modifié » ---------------------------------------------

    def _update_title(self, modified: bool) -> None:
        # `test_main_window_opens_and_closes` (phase 0) exige le titre exact
        # "PlotYx" à l'état propre : l'astérisque est le seul ajout.
        self.setWindowTitle(f"{self._base_title}*" if modified else self._base_title)

    # -- persistance : remplacement du projet vivant ---------------------------

    def _replace_project(self, project: Project, path: Path | None) -> None:
        """Échange projet, bus et vue explorateur. Réinitialise l'undo.

        Point de passage unique pour « ouvrir un fichier » et « récupérer un
        autosave » : les deux **remplacent** le projet courant, ils ne
        s'empilent pas dessus — voir la règle d'or de la phase 4. Toute
        sous-fenêtre de feuille ouverte sur l'ancien projet est fermée : ses
        widgets pointent sur des `Worksheet` qui n'existent plus.
        """
        for sub_window in list(self._open_sheets.values()):
            sub_window.close()
        self._open_sheets.clear()

        old_bus = self._bus
        self._project = project
        self._bus = CommandBus(project, undo_limit=1000, parent=self)
        self._bus.modified_changed.connect(self._update_title)
        old_bus.modified_changed.disconnect(self._update_title)
        old_bus.setParent(None)
        old_bus.deleteLater()

        self._project_tree_model = ProjectTreeModel(self._project, self)
        self._project_tree.setModel(self._project_tree_model)

        self._rebind_undo_redo_actions()

        self._current_path = path
        self._update_title(self._bus.is_modified)

        first_sheet = self._first_worksheet()
        if first_sheet is not None:
            self.open_worksheet(first_sheet)

    def _rebind_undo_redo_actions(self) -> None:
        """`QAction.createUndoAction` lie l'action à *une* pile pour toute sa
        vie : après un remplacement de projet, il faut de nouvelles actions
        posées aux mêmes emplacements dans le menu et la barre d'outils.
        """
        old_undo, old_redo = self._undo_action, self._redo_action
        self._undo_action = self._bus.stack.createUndoAction(self, "Annuler")
        self._undo_action.setShortcut(QKeySequence(QKeySequence.StandardKey.Undo))
        self._redo_action = self._bus.stack.createRedoAction(self, "Rétablir")
        self._redo_action.setShortcut(QKeySequence(QKeySequence.StandardKey.Redo))

        for container in (self._edit_menu, self._toolbar):
            container.insertAction(old_undo, self._undo_action)
            container.insertAction(old_redo, self._redo_action)
            container.removeAction(old_undo)
            container.removeAction(old_redo)

    # -- Fichier : nouveau / ouvrir / enregistrer -------------------------------

    def _on_new(self) -> None:
        if not self._confirm_discard_changes():
            return
        self._discard_own_autosave()
        self._replace_project(_default_project(), None)

    def _on_open(self) -> None:
        if not self._confirm_discard_changes():
            return
        path_str, _ = QFileDialog.getOpenFileName(self, "Ouvrir un projet", "", _FILE_FILTER)
        if not path_str:
            return
        self.open_path(Path(path_str))

    def open_path(self, path: Path) -> bool:
        """Charge `path` et remplace le projet courant. `True` si réussi."""
        try:
            project = load_project(path)
        except SchemaVersionError as error:
            QMessageBox.critical(self, "Version de fichier non prise en charge", str(error))
            return False
        except PersistenceError as error:
            QMessageBox.critical(self, "Fichier illisible", str(error))
            return False
        self._discard_own_autosave()
        self._replace_project(project, path)
        self._bus.set_clean()
        self._update_title(False)
        return True

    def _on_save(self) -> bool:
        if self._current_path is None:
            return self._on_save_as()
        return self._save_to(self._current_path)

    def _on_save_as(self) -> bool:
        default_name = f"{self._project.name}{FILE_EXTENSION}"
        default_path = str(self._current_path) if self._current_path else default_name
        path_str, _ = QFileDialog.getSaveFileName(
            self, "Enregistrer sous", default_path, _FILE_FILTER
        )
        if not path_str:
            return False
        path = Path(path_str)
        if path.suffix.lower() != FILE_EXTENSION:
            path = path.with_name(path.name + FILE_EXTENSION)
        return self._save_to(path)

    def _save_to(self, path: Path) -> bool:
        try:
            save_project(self._project, path)
        except OSError as error:
            QMessageBox.critical(self, "Échec de l'enregistrement", str(error))
            return False
        self._current_path = path
        self._bus.set_clean()
        self._discard_own_autosave()
        return True

    def _confirm_discard_changes(self) -> bool:
        """`True` s'il est permis de continuer (document propre, ou sauvé, ou
        abandon explicitement confirmé). `False` : l'appelant doit annuler
        son opération (fermeture de fenêtre, nouveau projet, ouverture...).
        """
        if not self._bus.is_modified:
            return True
        choice = QMessageBox.warning(
            self,
            "Modifications non enregistrées",
            "Le projet a été modifié. Enregistrer avant de continuer ?",
            QMessageBox.StandardButton.Save
            | QMessageBox.StandardButton.Discard
            | QMessageBox.StandardButton.Cancel,
            QMessageBox.StandardButton.Save,
        )
        if choice == QMessageBox.StandardButton.Cancel:
            return False
        if choice == QMessageBox.StandardButton.Save:
            return self._on_save()
        return True  # Discard

    def closeEvent(self, event) -> None:  # noqa: N802 - API Qt
        if not self._confirm_discard_changes():
            event.ignore()
            return
        self._autosave_timer.stop()
        self._discard_own_autosave()
        super().closeEvent(event)

    # -- autosave et recovery ---------------------------------------------------

    def _on_autosave_tick(self) -> None:
        if not self._bus.is_modified:
            return
        try:
            write_autosave(self._project, self._session_id, self._current_path)
        except OSError:
            # Best-effort : un autosave raté ne doit jamais interrompre
            # l'utilisateur ni faire planter l'application.
            pass

    def _discard_own_autosave(self) -> None:
        discard_autosave(self._session_id)

    def _offer_recovery(self) -> None:
        """Propose de restaurer l'autosave le plus récent d'une session
        interrompue.

        Si l'utilisateur accepte, le fichier d'autosave retrouvé est
        **renommé** au nom de la session courante plutôt que rechargé puis
        effacé : la protection contre un nouveau plantage reste continue,
        sans dépendre d'un signal « document modifié » qu'aucun `undo`
        n'a produit (rien n'a encore été tapé dans cette session). Les
        autres traces trouvées (sessions plus anciennes) sont effacées
        qu'il accepte ou refuse — les proposer indéfiniment à chaque
        démarrage serait plus gênant qu'utile.
        """
        candidates = list_recoverable(exclude_session=self._session_id)
        if not candidates:
            return
        newest = max(candidates, key=lambda p: p.stat().st_mtime)
        choice = QMessageBox.question(
            self,
            "Récupération",
            f"Une sauvegarde automatique a été trouvée ({newest.name}), probablement "
            "issue d'une session interrompue par un plantage. La restaurer ?",
            QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No,
        )
        if choice == QMessageBox.StandardButton.Yes:
            try:
                project = recover(newest)
            except PersistenceError as error:
                QMessageBox.critical(self, "Récupération impossible", str(error))
            else:
                source = autosave_source(newest)
                newest.replace(autosave_path(self._session_id))
                self._replace_project(project, Path(source) if source else None)
        for candidate in candidates:
            candidate.unlink(missing_ok=True)
