"""`Project` : la racine du modèle — arbre, registre, notifications.

Le projet possède trois choses et rien d'autre :

* un **dossier racine**, sommet de l'arbre `Folder → Workbook → Worksheet
  → Column`, avec `Graph`, `Matrix` et `Note` comme feuilles ;
* un **`ObjectRegistry`**, tenu à jour automatiquement à chaque
  attachement/détachement, jamais à la main ;
* deux **signaux agrégés** (`data_changed`, `structure_changed`) où
  remontent toutes les notifications des descendants.

### Frontière des mutations (préparation de la phase 2)

L'ensemble des primitives de mutation du modèle est **fermé** et se limite
à ceci :

* `Column` : `set_values`, `set_length`, `insert_rows`, `delete_rows`,
  `set_metadata`, `set_column_type`
* `Worksheet` : `insert_column`, `remove_column`, `move_column`,
  `rename_column`, `insert_rows`, `delete_rows`
* `Workbook` : `insert_sheet`, `remove_sheet`, `move_sheet`, `rename_sheet`
* `Folder` : `insert_child`, `remove_child`, `move_child`, `rename_child`
* `ModelObject` : `_set_name`, appelée uniquement par le conteneur, seul à
  pouvoir valider l'unicité du nom

Chacune est réversible **par delta** — elle retourne exactement ce qu'il
faut pour l'annuler (ancien nom, ancien index, valeurs écrasées, objet
retiré) — et émet exactement une notification. Les méthodes de `Project`
ci-dessous ne sont que des *façades* qui aiguillent vers ces primitives :
la phase 2 enveloppera les primitives, pas les façades.
"""

from __future__ import annotations

from collections.abc import Iterator
from typing import Any

from plotyx.core.column import Column
from plotyx.core.errors import PathError, StructureError
from plotyx.core.node import ModelObject
from plotyx.core.nodes import Folder, Graph, Matrix, Note, WindowNode, Workbook
from plotyx.core.paths import ObjectPath, format_path, parse_path
from plotyx.core.registry import ObjectRegistry
from plotyx.core.signals import Signal
from plotyx.core.worksheet import Worksheet

__all__ = ["Project"]


class Project:
    """Racine du modèle de données."""

    def __init__(self, name: str = "Project") -> None:
        self._root = Folder(name)
        self._root._owner_project = self
        self._registry = ObjectRegistry()
        self._registry.register(self._root)

    def __repr__(self) -> str:  # pragma: no cover - confort de debug
        return f"<Project {self._root.name!r} windows={len(self.windows())}>"

    # -- accès ------------------------------------------------------------

    @property
    def root(self) -> Folder:
        return self._root

    @property
    def name(self) -> str:
        return self._root.name

    @property
    def registry(self) -> ObjectRegistry:
        return self._registry

    @property
    def data_changed(self) -> Signal:
        """Agrégat : toute modification de valeur d'un descendant."""
        return self._root.data_changed

    @property
    def structure_changed(self) -> Signal:
        """Agrégat : toute modification de structure ou de métadonnée."""
        return self._root.structure_changed

    def walk(self) -> Iterator[ModelObject]:
        """Parcours préfixe de tout l'arbre, racine comprise."""
        return self._root.walk()

    def windows(self) -> tuple[WindowNode, ...]:
        return tuple(n for n in self.walk() if isinstance(n, WindowNode))

    def find_window(self, name: str) -> WindowNode | None:
        """Fenêtre portant ce nom, ou None. Les noms sont uniques par projet."""
        for node in self.walk():
            if isinstance(node, WindowNode) and node.name == name:
                return node
        return None

    # -- registre ---------------------------------------------------------

    def _register_subtree(self, node: ModelObject) -> None:
        for descendant in node.walk():
            self._registry.register(descendant)

    def _unregister_subtree(self, node: ModelObject) -> None:
        for descendant in node.walk():
            self._registry.unregister(descendant)

    def resolve_key(self, key: str) -> ModelObject:
        """Objet inscrit sous cette clé. Insensible aux renommages/déplacements."""
        return self._registry.resolve(key)

    def key_of(self, obj: ModelObject) -> str:
        """Clé d'un objet, après vérification qu'il appartient bien au projet."""
        if self._registry.get(obj.key) is not obj:
            raise StructureError(f"{obj!r} n'appartient pas au projet {self.name!r}")
        return obj.key

    # -- chemins ----------------------------------------------------------

    def resolve_path(self, text: str | ObjectPath) -> ModelObject:
        """Résout un chemin `[Book1]Sheet1!ColA`. Lève `PathError` si introuvable."""
        path = text if isinstance(text, ObjectPath) else parse_path(text)

        if path.window is None:
            node: ModelObject = self._root
            for segment in path.folders:
                try:
                    node = node.folder(segment)  # type: ignore[union-attr]
                except (AttributeError, KeyError):
                    raise PathError(f"dossier introuvable : {segment!r}") from None
            return node

        window = self.find_window(path.window)
        if window is None:
            raise PathError(f"fenêtre introuvable : [{path.window}]")
        if path.folders and tuple(self._folder_names(window)) != path.folders:
            raise PathError(
                f"[{path.window}] n'est pas dans le dossier "
                f"{'/'.join(path.folders)!r} mais dans {'/'.join(self._folder_names(window))!r}"
            )

        if path.sheet is None:
            return window
        if not isinstance(window, Workbook):
            raise PathError(f"[{path.window}] n'est pas un classeur, il n'a pas de feuille")
        try:
            sheet = window.sheet(path.sheet)
        except KeyError:
            raise PathError(f"feuille introuvable : [{path.window}]{path.sheet}") from None

        if path.column is None:
            return sheet
        try:
            return sheet.column(path.column)
        except KeyError:
            raise PathError(f"colonne introuvable : {format_path(path)}") from None

    def path_of(self, obj: ModelObject) -> str:
        """Chemin canonique complet d'un objet du projet."""
        if self._registry.get(obj.key) is not obj:
            raise StructureError(f"{obj!r} n'appartient pas au projet {self.name!r}")
        if obj is self._root:
            return "/"
        if isinstance(obj, Folder):
            names = [*self._folder_names(obj), obj.name]
            return format_path(ObjectPath(folders=tuple(names), rooted=True))

        column = obj if isinstance(obj, Column) else None
        sheet = obj if isinstance(obj, Worksheet) else (column.parent if column else None)
        window = obj if isinstance(obj, WindowNode) else (sheet.parent if sheet else None)
        if window is None:
            raise StructureError(f"objet non adressable par chemin : {obj!r}")
        return format_path(
            ObjectPath(
                folders=tuple(self._folder_names(window)),
                window=window.name,
                sheet=sheet.name if sheet is not None else None,
                column=column.name if column is not None else None,
                rooted=True,
            )
        )

    def _folder_names(self, node: ModelObject) -> list[str]:
        """Noms des dossiers contenant `node`, racine exclue, de haut en bas."""
        names = [a.name for a in node.ancestors() if isinstance(a, Folder) and a is not self._root]
        names.reverse()
        return names

    # -- façades de mutation ----------------------------------------------
    #
    # Aiguillage vers la primitive du conteneur concerné. Aucune logique de
    # mutation propre : ces méthodes existent pour l'ergonomie de l'API et
    # de la console, pas pour contourner les primitives.

    def add(self, node: ModelObject, folder: Folder | None = None) -> ModelObject:
        """Ajoute un dossier ou une fenêtre dans `folder` (racine par défaut).

        Le test est bien `is None` : un `Folder` vide est *falsy* (il définit
        `__len__`), un `folder or self._root` rangerait donc silencieusement
        l'objet à la racine à chaque fois que le dossier cible est vide.
        """
        return (self._root if folder is None else folder).add_child(node)

    def add_folder(self, name: str = "Folder1", folder: Folder | None = None) -> Folder:
        return self.add(Folder(name), folder)  # type: ignore[return-value]

    def add_workbook(self, name: str | None = None, folder: Folder | None = None) -> Workbook:
        book = Workbook(name if name is not None else self._next_window_name("Book"))
        return self.add(book, folder)  # type: ignore[return-value]

    def add_graph(self, name: str | None = None, folder: Folder | None = None) -> Graph:
        node = Graph(name if name is not None else self._next_window_name("Graph"))
        return self.add(node, folder)  # type: ignore[return-value]

    def add_matrix(self, name: str | None = None, folder: Folder | None = None) -> Matrix:
        node = Matrix(name if name is not None else self._next_window_name("MBook"))
        return self.add(node, folder)  # type: ignore[return-value]

    def add_note(
        self, name: str | None = None, folder: Folder | None = None, text: str = ""
    ) -> Note:
        node = Note(name if name is not None else self._next_window_name("Note"), text)
        return self.add(node, folder)  # type: ignore[return-value]

    def remove(self, node: ModelObject) -> tuple[int, ModelObject]:
        """Retire `node` de son conteneur. Retourne `(index, node)`."""
        parent = node.parent
        if parent is None:
            raise StructureError(f"{node!r} n'a pas de conteneur")
        if isinstance(node, Column):
            return parent.remove_column(node)  # type: ignore[union-attr]
        if isinstance(node, Worksheet):
            return parent.remove_sheet(node)  # type: ignore[union-attr]
        return parent.remove_child(node)  # type: ignore[union-attr]

    def rename(self, node: ModelObject, name: str) -> str:
        """Renomme `node` via son conteneur (qui valide l'unicité)."""
        parent = node.parent
        if parent is None:
            return node._set_name(name)
        if isinstance(node, Column):
            return parent.rename_column(node, name)  # type: ignore[union-attr]
        if isinstance(node, Worksheet):
            return parent.rename_sheet(node, name)  # type: ignore[union-attr]
        return parent.rename_child(node, name)  # type: ignore[union-attr]

    def move(
        self, node: ModelObject, new_parent: ModelObject, index: int | None = None
    ) -> tuple[ModelObject, int]:
        """Déplace `node` sous `new_parent`. Retourne `(ancien parent, ancien index)`.

        La clé du registre est conservée : toute référence détenue ailleurs
        (série de graphe, variable de console, `QMimeData` en vol) reste
        valide après le déplacement.
        """
        old_parent = node.parent
        if old_parent is None:
            raise StructureError(f"{node!r} n'a pas de conteneur")
        self._check_move(node, new_parent)
        old_index, _ = self.remove(node)
        target = len(new_parent.children()) if index is None else index
        try:
            if isinstance(node, Column):
                new_parent.insert_column(target, node)  # type: ignore[union-attr]
            elif isinstance(node, Worksheet):
                new_parent.insert_sheet(target, node)  # type: ignore[union-attr]
            else:
                new_parent.insert_child(target, node)  # type: ignore[union-attr]
        except Exception:  # pragma: no cover - filet : on remet l'objet en place
            self._reinsert(node, old_parent, old_index)
            raise
        return old_parent, old_index

    def _check_move(self, node: ModelObject, new_parent: ModelObject) -> None:
        """Refuse un déplacement invalide **avant** de détacher quoi que ce soit."""
        expected = {
            Column: Worksheet,
            Worksheet: Workbook,
        }.get(type(node), Folder)
        if isinstance(node, WindowNode):
            expected = Folder
        if not isinstance(new_parent, expected):
            raise StructureError(
                f"{type(node).__name__} ne peut pas être déplacé dans "
                f"{type(new_parent).__name__}"
            )
        if node is new_parent or node in set(new_parent.ancestors()):
            raise StructureError("déplacement circulaire refusé")
        if isinstance(new_parent, Folder):
            new_parent._check_free_name(node)
        elif isinstance(new_parent, Workbook):
            new_parent._check_free_name(node.name)
        elif isinstance(new_parent, Worksheet):
            new_parent._check_free_name(node.name)

    def _reinsert(self, node: ModelObject, parent: ModelObject, index: int) -> None:
        if isinstance(node, Column):
            parent.insert_column(index, node)  # type: ignore[union-attr]
        elif isinstance(node, Worksheet):
            parent.insert_sheet(index, node)  # type: ignore[union-attr]
        else:
            parent.insert_child(index, node)  # type: ignore[union-attr]

    # -- utilitaires ------------------------------------------------------

    def _next_window_name(self, prefix: str) -> str:
        index = 1
        while self.find_window(f"{prefix}{index}") is not None:
            index += 1
        return f"{prefix}{index}"

    def observe(
        self,
        on_data: Any = None,
        on_structure: Any = None,
    ) -> list[Any]:
        """Confort de test et de pont UI : abonne des callbacks agrégés."""
        connections = []
        if on_data is not None:
            connections.append(self.data_changed.connect(on_data))
        if on_structure is not None:
            connections.append(self.structure_changed.connect(on_structure))
        return connections
