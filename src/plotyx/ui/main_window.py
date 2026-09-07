"""Fenêtre principale de PlotYx.

Phase 3 : la fenêtre porte un `Project` et le `CommandBus` qui va avec. Le
dock « Project Explorer » est rempli (arbre du projet, double-clic pour
ouvrir une feuille) ; le dock « Script Console » reste un stub (phase 9). Les menus
Undo/Redo utilisent `QKeySequence.StandardKey`, jamais un raccourci en
dur — voir la Politique macOS de la roadmap, qui s'applique dès
maintenant même si macOS est hors release.

Phase 4 : Save / Save As / Open, prompt de sauvegarde à la fermeture,
autosave périodique et recovery au démarrage. `_replace_project` est le
point de passage unique qui échange le projet et le bus vivants — ouvrir
un fichier ou récupérer un autosave **remplace** le projet, ça ne s'empile
pas dessus (§ « la règle d'or » de la phase 4) : la pile d'undo précédente
est détruite, une neuve la remplace.

Phase 6 : les graphes s'ouvrent en sous-fenêtres MDI (`GraphWindow`) au
même titre que les feuilles, et le dock « Properties » — jusqu'ici un
`QLabel` de substitution — accueille le panneau de propriétés du graphe
actif (`PlotPropertiesPanel`). Il se vide quand la sous-fenêtre active
n'est pas un graphe : un panneau qui garderait à l'écran les propriétés
d'une fenêtre qu'on ne regarde plus est pire qu'un panneau vide.

Phase 6, réouverte le 2026-09-07 : un graphe se peint désormais avec
Matplotlib par défaut, PyQtGraph n'étant plus qu'un « visualiseur léger »
(`plotyx.ui.renderers`). Cette fenêtre en offre les deux entrées, toutes
deux volontairement discrètes : « Ouvrir en visualiseur léger » dans le
menu contextuel de l'explorateur, et une case dans le menu Options qui
bascule la fenêtre de graphe **active**. Le mode n'est ni lu ni écrit dans
le `.plx`, et n'a donc pas de défaut d'application à mémoriser.
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
from plotyx.core.column import Column
from plotyx.core.nodes import Graph, Workbook
from plotyx.core.project import Project
from plotyx.core.worksheet import Worksheet
from plotyx.io.registry import default_registry
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
from plotyx.ui.graph_window import GraphWindow
from plotyx.ui.import_wizard import ImportWizardDialog
from plotyx.ui.plot_actions import create_graph_from_columns
from plotyx.ui.plot_properties import PlotPropertiesPanel
from plotyx.ui.project_tree_model import ProjectTreeModel
from plotyx.ui.renderers import DEFAULT_RENDER_MODE, RenderMode
from plotyx.ui.worksheet_model import WorksheetTableModel
from plotyx.ui.worksheet_view import WorksheetView

__all__ = ["MainWindow"]

#: Fréquence d'autosave par défaut. `0` désarme le minuteur (tests).
DEFAULT_AUTOSAVE_INTERVAL_MS = 120_000

_FILE_FILTER = f"Projets PlotYx (*{FILE_EXTENSION})"


def _make_stub_dock(title: str, object_name: str, placeholder_text: str) -> QDockWidget:
    """Crée un dock stub : juste un titre et un texte de substitution.

    Encore utilisé par « Script Console » (phase 9), et par « Properties »
    tant qu'aucun graphe n'est actif — le panneau de la phase 6 y prend sa
    place à la demande (`_update_properties_dock`).
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
        self.setAcceptDrops(True)

        self._current_path: Path | None = None
        self._session_id = uuid.uuid4().hex
        self._parser_registry = default_registry()

        self._project = project if project is not None else _default_project()
        self._bus = CommandBus(self._project, undo_limit=1000, parent=self)
        self._bus.modified_changed.connect(self._update_title)

        self._mdi_area = QMdiArea(self)
        self.setCentralWidget(self._mdi_area)
        self._open_sheets: dict[str, QMdiSubWindow] = {}
        self._open_graphs: dict[str, QMdiSubWindow] = {}
        self._properties_panel: PlotPropertiesPanel | None = None

        self._mdi_area.subWindowActivated.connect(self._on_sub_window_activated)
        self._build_project_explorer_dock()
        self._properties_dock = _make_stub_dock(
            "Properties", "propertiesDock", "(sélectionnez un graphe)"
        )
        self._script_console_dock = _make_stub_dock(
            "Script Console", "scriptConsoleDock", "(à venir — phase 9)"
        )

        self.addDockWidget(Qt.DockWidgetArea.LeftDockWidgetArea, self._project_explorer_dock)
        self.addDockWidget(Qt.DockWidgetArea.RightDockWidgetArea, self._properties_dock)
        self.addDockWidget(Qt.DockWidgetArea.BottomDockWidgetArea, self._script_console_dock)

        self._build_file_actions()
        self._build_plot_actions()
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

        # Un classeur déroulé jusqu'aux colonnes, multiplié par plusieurs
        # classeurs ouverts en même temps, rend l'arbre illisible. On garde
        # le déroulement (utile pour retrouver une colonne) et on donne un
        # moyen de tout réduire d'un coup plutôt que nœud par nœud.
        self._collapse_explorer_action = QAction("Réduire tout", self)
        self._collapse_explorer_action.setObjectName("collapseExplorerAction")
        self._collapse_explorer_action.setToolTip(
            "Replie tous les classeurs de l'explorateur de projet"
        )
        # Pas de QKeySequence.StandardKey pour ça : raccourci choisi, sans
        # collision avec Fichier/Édition (voir l'en-tête du module).
        self._collapse_explorer_action.setShortcut(QKeySequence("Ctrl+Shift+R"))
        self._collapse_explorer_action.setShortcutContext(
            Qt.ShortcutContext.WidgetWithChildrenShortcut
        )
        self._collapse_explorer_action.triggered.connect(tree.collapseAll)
        tree.addAction(self._collapse_explorer_action)

    def _on_explorer_double_clicked(self, index) -> None:
        node = self._project_tree_model.node_at(index)
        if isinstance(node, Worksheet):
            self.open_worksheet(node)
        elif isinstance(node, Graph):
            self.open_graph(node)
        elif isinstance(node, Workbook) and node.sheets:
            self.open_worksheet(node.sheets[0])

    def _on_explorer_context_menu(self, pos) -> None:
        from PySide6.QtWidgets import QMenu

        index = self._project_tree.indexAt(pos)
        node = self._project_tree_model.node_at(index)
        if node is None:
            return
        menu = QMenu()
        menu.addAction(self._collapse_explorer_action)
        menu.addSeparator()
        act_rename = menu.addAction("Renommer…")
        act_rename.triggered.connect(lambda: self._rename_node(node))
        act_delete = menu.addAction("Supprimer")
        act_delete.triggered.connect(lambda: self._delete_node(node))
        if isinstance(node, Workbook):
            act_add_sheet = menu.addAction("Nouvelle feuille")
            act_add_sheet.triggered.connect(lambda: self._add_sheet(node))
        if isinstance(node, Column | Worksheet):
            act_plot = menu.addAction("Tracer dans un nouveau graphe")
            act_plot.triggered.connect(lambda: self._plot_nodes([node]))
        if isinstance(node, Graph):
            # Le seul chemin qui *ouvre* directement en vue directe. Le
            # double-clic, lui, reste sur le rendu par défaut : c'est le
            # geste courant, il ne doit pas dépendre d'un mode mémorisé.
            act_light = menu.addAction("Ouvrir en visualiseur léger")
            act_light.setToolTip(
                "Rendu PyQtGraph, minimal mais direct — pour l'acquisition live"
            )
            act_light.triggered.connect(
                lambda: self.open_graph(node, mode=RenderMode.DIRECT)
            )
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

    def open_graph(
        self, graph: Graph, mode: RenderMode = DEFAULT_RENDER_MODE
    ) -> QMdiSubWindow:
        """Ouvre (ou ramène au premier plan) la sous-fenêtre d'un graphe.

        `mode` choisit le moteur de rendu **à l'ouverture** ; il n'est ni
        lu ni écrit dans le projet (voir `plotyx.ui.renderers`). Sur une
        fenêtre déjà ouverte, demander l'autre mode la fait basculer plutôt
        que d'en ouvrir une seconde : deux vues du même graphe côte à côte
        seraient deux sources de vérité pour un seul `PlotSpec`.
        """
        key = graph.key
        existing = self._open_graphs.get(key)
        if existing is not None:
            view = existing.widget()
            if isinstance(view, GraphWindow):
                view.set_render_mode(mode)
            self._mdi_area.setActiveSubWindow(existing)
            self._sync_light_view_action(existing)
            return existing

        widget = GraphWindow(graph, self._project, self._bus, mode=mode)
        sub_window = self._mdi_area.addSubWindow(widget)
        sub_window.setWindowTitle(f"[{graph.name}]")
        sub_window.setAttribute(Qt.WidgetAttribute.WA_DeleteOnClose)
        self._open_graphs[key] = sub_window

        def _on_destroyed(_obj=None, widget=widget, key=key) -> None:
            widget.disconnect_signals()
            self._open_graphs.pop(key, None)
            if self._properties_panel is not None and self._properties_panel.graph.key == key:
                self._clear_properties_dock()

        sub_window.destroyed.connect(_on_destroyed)
        sub_window.show()
        return sub_window

    # -- création de graphes (phase 6) --------------------------------------------

    def _active_worksheet_columns(self) -> list[Column]:
        """Colonnes à tracer d'après la sous-fenêtre active.

        Trois cas, alignés sur ce que fait déjà l'import
        (`_target_worksheet_for_import`) :

        * une **feuille** active → ses colonnes sélectionnées, ou toutes si
          la sélection est vide (« tracer la feuille ») ;
        * un **graphe** actif → rien : ajouter une série à un graphe
          existant est une autre action, celle du glisser-déposer de la
          phase 7 ;
        * **aucune** sous-fenêtre → la première feuille du projet, faute de
          quoi l'action serait inutilisable au démarrage.
        """
        active = self._mdi_area.activeSubWindow()
        if active is None:
            sheet = self._first_worksheet()
            return list(sheet.columns) if sheet is not None else []
        view = active.widget()
        if not isinstance(view, WorksheetView):
            return []
        table_model = view.model()
        if not isinstance(table_model, WorksheetTableModel):
            return []
        indexes = view.selectionModel().selectedIndexes() if view.selectionModel() else []
        selected = sorted({index.column() for index in indexes})
        columns = [table_model.column_at(index) for index in selected]
        return columns or list(table_model.sheet.columns)

    def _plot_nodes(self, nodes) -> None:
        """Trace une sélection de l'explorateur (colonnes, ou feuille entière)."""
        columns: list[Column] = []
        for node in nodes:
            if isinstance(node, Column):
                columns.append(node)
            elif isinstance(node, Worksheet):
                columns.extend(node.columns)
        self._create_graph(columns)

    def _on_plot_action(self) -> None:
        self._create_graph(self._active_worksheet_columns())

    def _create_graph(self, columns: list[Column]) -> Graph | None:
        """Crée le graphe et l'ouvre. Une seule entrée d'historique (macro)."""
        try:
            graph = create_graph_from_columns(self._project, self._bus, columns)
        except ValueError as error:
            QMessageBox.warning(self, "Tracé impossible", str(error))
            return None
        self.open_graph(graph)
        return graph

    # -- dock « Properties » ---------------------------------------------------------

    def _on_sub_window_activated(self, sub_window: QMdiSubWindow | None) -> None:
        widget = sub_window.widget() if sub_window is not None else None
        if isinstance(widget, GraphWindow):
            self._show_properties_for(widget.graph)
        else:
            self._clear_properties_dock()
        self._sync_light_view_action(sub_window)

    # -- moteur de rendu (phase 6, réouverte le 2026-09-07) ------------------------

    def _active_graph_window(self) -> GraphWindow | None:
        active = self._mdi_area.activeSubWindow()
        widget = active.widget() if active is not None else None
        return widget if isinstance(widget, GraphWindow) else None

    def _sync_light_view_action(self, sub_window: QMdiSubWindow | None) -> None:
        """Aligne la case du menu sur la fenêtre active, sans la déclencher.

        `setChecked` réémet `toggled` : sans la garde, activer une fenêtre en
        vue directe ferait basculer... la fenêtre qu'on vient d'activer, et
        un aller-retour entre deux graphes de modes différents les
        contaminerait l'un l'autre.
        """
        widget = sub_window.widget() if sub_window is not None else None
        graph_window = widget if isinstance(widget, GraphWindow) else None
        self._light_view_action.setEnabled(graph_window is not None)
        blocked = self._light_view_action.blockSignals(True)
        self._light_view_action.setChecked(
            graph_window is not None and graph_window.render_mode is RenderMode.DIRECT
        )
        self._light_view_action.blockSignals(blocked)

    def _on_light_view_toggled(self, enabled: bool) -> None:
        graph_window = self._active_graph_window()
        if graph_window is None:
            return
        graph_window.set_render_mode(RenderMode.DIRECT if enabled else RenderMode.FIGURE)

    def _show_properties_for(self, graph: Graph) -> None:
        if self._properties_panel is not None:
            if self._properties_panel.graph is graph:
                return
            self._properties_panel.disconnect_signals()
        panel = PlotPropertiesPanel(graph, self._project, self._bus)
        self._properties_dock.setWidget(panel)
        self._properties_panel = panel

    def _clear_properties_dock(self) -> None:
        """Remet le texte de substitution : pas de graphe actif, rien à éditer."""
        if self._properties_panel is not None:
            self._properties_panel.disconnect_signals()
            self._properties_panel = None
        placeholder = QLabel("(sélectionnez un graphe)")
        placeholder.setAlignment(Qt.AlignmentFlag.AlignCenter)
        placeholder.setEnabled(False)
        self._properties_dock.setWidget(placeholder)

    @property
    def properties_panel(self) -> PlotPropertiesPanel | None:
        """Le panneau de propriétés courant, ou `None` si aucun graphe actif."""
        return self._properties_panel

    # -- import de fichiers (phase 5) -------------------------------------------

    def _target_worksheet_for_import(self) -> Worksheet | None:
        """La feuille où atterrissent les colonnes importées.

        La sous-fenêtre active si c'en est une, sinon la première feuille du
        projet, sinon `None` — l'appelant doit alors renoncer : importer
        sans feuille où écrire n'a pas de sens (créer un classeur à la
        volée serait une mutation hors commande explicite non demandée
        par l'utilisateur).
        """
        active = self._mdi_area.activeSubWindow()
        if active is not None:
            for key, sub_window in self._open_sheets.items():
                if sub_window is active:
                    try:
                        return self._project.resolve_key(key)  # type: ignore[return-value]
                    except KeyError:
                        break
        return self._first_worksheet()

    def _open_import_wizard(self, paths: list[Path] | None = None) -> None:
        sheet = self._target_worksheet_for_import()
        if sheet is None:
            QMessageBox.warning(
                self, "Import impossible", "Aucune feuille n'existe pour recevoir l'import."
            )
            return
        dialog = ImportWizardDialog(sheet, self._bus, paths, self._parser_registry, self)
        dialog.exec()

    # -- drag & drop OS → application (phase 5) ---------------------------------

    def dragEnterEvent(self, event) -> None:  # noqa: N802 - API Qt
        if event.mimeData().hasUrls():
            event.acceptProposedAction()
        else:
            super().dragEnterEvent(event)

    def dropEvent(self, event) -> None:  # noqa: N802 - API Qt
        mime = event.mimeData()
        if not mime.hasUrls():
            super().dropEvent(event)
            return
        paths = [Path(url.toLocalFile()) for url in mime.urls() if url.isLocalFile()]
        paths = [p for p in paths if p.is_file()]
        if not paths:
            return
        event.acceptProposedAction()
        self._open_import_wizard(paths)

    # -- actions / menus / toolbar ---------------------------------------------

    def _build_file_actions(self) -> None:
        self._new_action = QAction("&Nouveau", self)
        self._new_action.triggered.connect(self._on_new)

        self._open_action = QAction("&Ouvrir…", self)
        self._open_action.setShortcut(QKeySequence(QKeySequence.StandardKey.Open))
        self._open_action.triggered.connect(self._on_open)

        self._import_action = QAction("&Importer…", self)
        self._import_action.triggered.connect(lambda: self._open_import_wizard())

        self._save_action = QAction("&Enregistrer", self)
        self._save_action.setShortcut(QKeySequence(QKeySequence.StandardKey.Save))
        self._save_action.triggered.connect(self._on_save)

        self._save_as_action = QAction("Enregistrer &sous…", self)
        self._save_as_action.setShortcut(QKeySequence(QKeySequence.StandardKey.SaveAs))
        self._save_as_action.triggered.connect(self._on_save_as)

    def _build_plot_actions(self) -> None:
        self._plot_action = QAction("Nouveau &graphe", self)
        self._plot_action.setToolTip(
            "Trace les colonnes sélectionnées de la feuille active"
        )
        self._plot_action.triggered.connect(self._on_plot_action)

        # Bascule du moteur de rendu. Volontairement discrète (décision du
        # 2026-09-07) : dans un menu, jamais dans la barre d'outils, pour un
        # usage estimé à 0,5 % des cas. L'état est celui de la fenêtre de
        # graphe **active**, jamais un défaut d'application — un graphe
        # d'acquisition et un graphe de dépouillement cohabitent.
        self._light_view_action = QAction("&Visualiseur léger (vue directe)", self)
        self._light_view_action.setObjectName("lightViewAction")
        self._light_view_action.setCheckable(True)
        self._light_view_action.setToolTip(
            "Rendu PyQtGraph, minimal mais direct — pour l'acquisition live. "
            "Jamais enregistré dans le projet."
        )
        self._light_view_action.toggled.connect(self._on_light_view_toggled)

    def _build_actions(self) -> None:
        self._undo_action: QAction = self._bus.stack.createUndoAction(self, "Annuler")
        self._undo_action.setShortcut(QKeySequence(QKeySequence.StandardKey.Undo))
        self._redo_action: QAction = self._bus.stack.createRedoAction(self, "Rétablir")
        self._redo_action.setShortcut(QKeySequence(QKeySequence.StandardKey.Redo))

    def _build_menus(self) -> None:
        file_menu = self.menuBar().addMenu("&Fichier")
        file_menu.addAction(self._new_action)
        file_menu.addAction(self._open_action)
        file_menu.addAction(self._import_action)
        file_menu.addAction(self._save_action)
        file_menu.addAction(self._save_as_action)
        self._file_menu = file_menu

        self._edit_menu = self.menuBar().addMenu("&Édition")
        self._edit_menu.addAction(self._undo_action)
        self._edit_menu.addAction(self._redo_action)

        graph_menu = self.menuBar().addMenu("&Graphe")
        graph_menu.addAction(self._plot_action)
        self._graph_menu = graph_menu

        view_menu = self.menuBar().addMenu("&Affichage")
        view_menu.addAction(self._collapse_explorer_action)
        view_menu.addSeparator()
        view_menu.addAction(self._project_explorer_dock.toggleViewAction())
        view_menu.addAction(self._properties_dock.toggleViewAction())
        view_menu.addAction(self._script_console_dock.toggleViewAction())

        options_menu = self.menuBar().addMenu("&Options")
        options_menu.addAction(self._light_view_action)
        self._options_menu = options_menu
        self._sync_light_view_action(None)

    def _build_toolbar(self) -> None:
        toolbar = QToolBar("Principale", self)
        toolbar.setObjectName("mainToolBar")
        toolbar.addAction(self._save_action)
        toolbar.addAction(self._open_action)
        toolbar.addSeparator()
        toolbar.addAction(self._undo_action)
        toolbar.addAction(self._redo_action)
        toolbar.addSeparator()
        toolbar.addAction(self._plot_action)
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
        for sub_window in [*self._open_sheets.values(), *self._open_graphs.values()]:
            sub_window.close()
        self._open_sheets.clear()
        self._open_graphs.clear()
        self._clear_properties_dock()

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
