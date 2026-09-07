"""`ProjectTreeModel` : `QAbstractItemModel` sur l'arbre `Project`.

Alimente le dock « Project Explorer ». Comme le modèle de worksheet, il ne
copie rien : chaque `QModelIndex` porte directement un `ModelObject` du
noyau comme `internalPointer`, et la structure est relue à travers
`children()`/`parent` à chaque appel. Un seul abonnement, à la racine du
projet, suffit à tout voir : `structure_changed` remonte de n'importe quel
descendant jusqu'à la racine (`core/node.py`).

La racine du projet (`Project.root`, un `Folder` sans nom visible d'intérêt)
n'est pas montrée : ses enfants directs sont les lignes de premier niveau,
comme un explorateur de fichiers ne montre pas `/` littéralement.
"""

from __future__ import annotations

from typing import Any

from PySide6.QtCore import QAbstractItemModel, QModelIndex, Qt

from plotyx.core.column import Column
from plotyx.core.node import ModelObject, StructureChanged
from plotyx.core.nodes import Folder, Graph, Matrix, Note, Workbook
from plotyx.core.project import Project
from plotyx.core.worksheet import Worksheet

__all__ = ["ProjectTreeModel"]

#: Kinds de `StructureChanged` qui insèrent une ligne, avec le conteneur
#: comme `event.source` et l'index dans `event.details["index"]`.
_INSERT_KINDS = {"child_inserted", "sheet_inserted", "column_inserted"}
_REMOVE_KINDS = {"child_removed", "sheet_removed", "column_removed"}
_MOVE_KINDS = {"child_moved", "sheet_moved", "column_moved"}


def _icon_text(node: ModelObject) -> str:
    if isinstance(node, Folder):
        return node.name
    if isinstance(node, Workbook):
        return f"{node.name}"
    if isinstance(node, Worksheet):
        return node.name
    if isinstance(node, Column):
        role = f" [{node.role}]" if node.role.value != "None" else ""
        return f"{node.name}{role}"
    if isinstance(node, Graph | Matrix | Note):
        return node.name
    return node.name  # pragma: no cover - filet


class ProjectTreeModel(QAbstractItemModel):
    """Arbre `Folder → Workbook → Worksheet → Column`, plus `Graph`/`Matrix`/`Note`."""

    def __init__(self, project: Project, parent: Any = None) -> None:
        super().__init__(parent)
        self._project = project
        self._structure_conn = project.structure_changed.connect(self._on_structure_changed)

    def disconnect_signals(self) -> None:
        self._structure_conn.disconnect()

    @property
    def project(self) -> Project:
        return self._project

    # -- structure de l'arbre -------------------------------------------------

    def _children_of(self, node: ModelObject) -> tuple[ModelObject, ...]:
        if isinstance(node, Column):
            return ()
        return node.children()

    def index(  # noqa: N802 - API Qt
        self, row: int, column: int, parent: QModelIndex = QModelIndex()  # noqa: B008
    ) -> QModelIndex:
        parent_node = parent.internalPointer() if parent.isValid() else self._project.root
        children = self._children_of(parent_node)
        if 0 <= row < len(children):
            return self.createIndex(row, column, children[row])
        return QModelIndex()

    def parent(self, index: QModelIndex) -> QModelIndex:  # noqa: N802 - API Qt
        if not index.isValid():
            return QModelIndex()
        node = index.internalPointer()
        parent_node = node.parent
        if parent_node is None or parent_node is self._project.root:
            return QModelIndex()
        return self._index_for(parent_node)

    def _index_for(self, node: ModelObject) -> QModelIndex:
        if node is None or node is self._project.root:
            return QModelIndex()
        parent_node = node.parent
        siblings = self._children_of(parent_node) if parent_node is not None else (node,)
        try:
            row = siblings.index(node)
        except ValueError:  # pragma: no cover - incohérence transitoire
            return QModelIndex()
        return self.createIndex(row, 0, node)

    def rowCount(self, parent: QModelIndex = QModelIndex()) -> int:  # noqa: N802,B008 - API Qt
        if parent.column() > 0:
            return 0
        parent_node = parent.internalPointer() if parent.isValid() else self._project.root
        return len(self._children_of(parent_node))

    def columnCount(self, parent: QModelIndex = QModelIndex()) -> int:  # noqa: N802,B008 - API Qt
        return 1

    def data(self, index: QModelIndex, role: int = Qt.ItemDataRole.DisplayRole) -> Any:
        if not index.isValid():
            return None
        node = index.internalPointer()
        if role == Qt.ItemDataRole.DisplayRole:
            return _icon_text(node)
        if role == Qt.ItemDataRole.ToolTipRole:
            return f"{type(node).__name__} : {self._project.path_of(node)}"
        if role == Qt.ItemDataRole.UserRole:
            return node
        return None

    def flags(self, index: QModelIndex) -> Qt.ItemFlag:
        if not index.isValid():
            return Qt.ItemFlag.NoItemFlags
        return Qt.ItemFlag.ItemIsEnabled | Qt.ItemFlag.ItemIsSelectable

    def headerData(
        self,
        section: int,
        orientation: Qt.Orientation,
        role: int = Qt.ItemDataRole.DisplayRole,
    ) -> Any:
        if orientation == Qt.Orientation.Horizontal and role == Qt.ItemDataRole.DisplayRole:
            return "Projet"
        return None

    def node_at(self, index: QModelIndex) -> ModelObject | None:
        return index.internalPointer() if index.isValid() else None

    # -- adaptateur de notifications core → Qt -------------------------------

    def _on_structure_changed(self, event: StructureChanged) -> None:
        """Insertion/suppression/déplacement → reset complet, délibérément.

        `core` applique la mutation *avant* de notifier (voir
        `core/node.py`) : au moment où cet abonné est appelé, l'enfant est
        déjà retiré (ou déjà inséré) de `children()`. Un `beginRemoveRows`
        interrogerait alors un `rowCount()` déjà à jour — exactement le
        piège d'appariement mis en garde pour cette phase. Le modèle de
        worksheet le résout avec des compteurs tenus à la main parce que
        c'est le chemin chaud (1M lignes) ; l'explorateur de projet, lui,
        change rarement et n'a aucune exigence de performance : un reset
        complet est correct par construction et plus simple à garder juste.
        """
        kind = event.kind
        if kind in _INSERT_KINDS or kind in _REMOVE_KINDS or kind in _MOVE_KINDS:
            self.beginResetModel()
            self.endResetModel()
        elif kind == "node_renamed":
            self._emit_changed(event.source)
        elif kind in ("column_metadata_changed", "column_type_changed"):
            self._emit_changed(event.source)

    def _emit_changed(self, node: ModelObject) -> None:
        index = self._index_for(node)
        if index.isValid():
            self.dataChanged.emit(index, index)
