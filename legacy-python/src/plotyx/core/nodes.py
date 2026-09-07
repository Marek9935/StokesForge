"""Nœuds de l'arbre projet : dossiers, classeurs, et fenêtres.

L'arbre suit le modèle Origin :

```
Folder ─┬─ Folder ...
        ├─ Workbook ─── Worksheet ─── Column
        ├─ Graph        (nœud minimal en phase 1)
        ├─ Matrix       (idem)
        └─ Note         (idem)
```

Le niveau `Workbook` n'apparaît pas explicitement dans la roadmap, qui écrit
`Folder → Worksheet | Graph | Matrix | Note`, mais il est **imposé par la
grammaire de chemins** `[Book1]Sheet1!ColA` : `Book1` est le classeur,
`Sheet1` une de ses feuilles. Sans lui, la partie `[...]` du chemin n'a pas
de référent.

`Graph`, `Matrix` et `Note` sont volontairement réduits à leur identité et à
un dictionnaire de métadonnées : leur contenu appartient aux phases 6, 8 et
11. Ils existent ici pour que l'arbre, les chemins et le registre soient
complets et n'aient pas à être retouchés plus tard.
"""

from __future__ import annotations

from collections.abc import Iterator, Sequence
from typing import Any

from plotyx.core.errors import DuplicateNameError, StructureError
from plotyx.core.node import ModelObject, StructureChanged, attach, check_name, detach
from plotyx.core.worksheet import Worksheet

__all__ = ["Folder", "Graph", "Matrix", "Note", "Workbook", "WindowNode"]


class WindowNode(ModelObject):
    """Base des objets adressables par `[Nom]` : classeurs, graphes, ...

    Leur nom est unique **à l'échelle du projet**, pas seulement du dossier :
    c'est ce qui rend `[Book1]Sheet1!ColA` non ambigu où que le classeur
    soit rangé.
    """

    type_name = "window"

    def __init__(self, name: str, *, label: str = "", comment: str = "") -> None:
        super().__init__(name)
        self.label = label
        self.comment = comment
        self.metadata: dict[str, Any] = {}


class Graph(WindowNode):
    """Fenêtre graphique. Son contenu est un `PlotSpec` (phase 6).

    ## Pourquoi `core` ne connaît pas `plotyx.plot`

    Le spec est rangé ici sans être typé à l'exécution : `core` reste le
    socle dont tout dépend et qui ne dépend de rien, et la dépendance va
    dans un seul sens — `plotyx.plot.resolve` connaît `core`, jamais
    l'inverse. Le seul contrat que `Graph` exige de l'objet stocké est
    d'être immuable, ce que `PlotSpec` garantit (dataclasses `frozen`) :
    c'est ce qui permet à `SetPlotSpecCommand` de s'annuler en reposant
    l'ancien objet, sans copie ni delta.

    `None` signifie « graphe encore vide » — l'état d'un `Graph` créé
    avant qu'aucune série n'ait été tracée, et celui d'un graphe relu dans
    un fichier de schéma 1, écrit avant que ce champ n'existe.
    """

    type_name = "graph"

    def __init__(self, name: str, **kwargs: Any) -> None:
        super().__init__(name, **kwargs)
        self._plot_spec: Any = None

    @property
    def plot_spec(self) -> Any:
        """Le `PlotSpec` du graphe, ou `None` s'il n'a pas encore de contenu."""
        return self._plot_spec

    def set_plot_spec(self, spec: Any) -> Any:
        """Primitive de mutation. Retourne le spec précédent (delta d'annulation).

        Comme toutes les primitives du modèle : une seule notification, et
        de quoi s'inverser. Enveloppée par
        `plotyx.commands.plot.SetPlotSpecCommand`.
        """
        previous, self._plot_spec = self._plot_spec, spec
        self._notify_structure(StructureChanged(self, "plot_spec_changed", {"spec": spec}))
        return previous


class Matrix(WindowNode):
    """Fenêtre matrice. Nœud minimal : le contenu arrive plus tard."""

    type_name = "matrix"


class Note(WindowNode):
    """Fenêtre de notes. Nœud minimal : texte libre."""

    type_name = "note"

    def __init__(self, name: str, text: str = "", **kwargs: Any) -> None:
        super().__init__(name, **kwargs)
        self.text = text


class Workbook(WindowNode):
    """Classeur : une liste ordonnée de feuilles, adressé par `[Nom]`."""

    type_name = "workbook"

    def __init__(self, name: str = "Book1", sheets: Sequence[Worksheet] | None = None) -> None:
        super().__init__(name)
        self._sheets: list[Worksheet] = []
        for sheet in sheets if sheets is not None else [Worksheet("Sheet1")]:
            self.insert_sheet(len(self._sheets), sheet)

    # -- lecture ----------------------------------------------------------

    @property
    def sheets(self) -> tuple[Worksheet, ...]:
        return tuple(self._sheets)

    def children(self) -> tuple[ModelObject, ...]:
        return tuple(self._sheets)

    def __len__(self) -> int:
        return len(self._sheets)

    def __iter__(self) -> Iterator[Worksheet]:
        return iter(self._sheets)

    def sheet(self, ref: str | int | Worksheet) -> Worksheet:
        if isinstance(ref, Worksheet):
            if ref not in self._sheets:
                raise KeyError(f"feuille {ref.name!r} absente du classeur {self.name!r}")
            return ref
        if isinstance(ref, int):
            return self._sheets[ref]
        for sheet in self._sheets:
            if sheet.name == ref:
                return sheet
        raise KeyError(f"aucune feuille nommée {ref!r} dans le classeur {self.name!r}")

    def sheet_names(self) -> tuple[str, ...]:
        return tuple(sheet.name for sheet in self._sheets)

    # -- mutations primitives ---------------------------------------------

    def insert_sheet(self, index: int, sheet: Worksheet) -> Worksheet:
        if not isinstance(sheet, Worksheet):
            raise StructureError(f"un classeur ne contient que des feuilles, pas {sheet!r}")
        if sheet.parent is not None:
            raise StructureError(f"la feuille {sheet.name!r} appartient déjà à {sheet.parent!r}")
        self._check_free_name(sheet.name)
        index = max(0, min(index, len(self._sheets)))
        self._sheets.insert(index, sheet)
        attach(sheet, self)
        self._notify_structure(
            StructureChanged(self, "sheet_inserted", {"index": index, "sheet": sheet})
        )
        return sheet

    def add_sheet(self, name: str | None = None) -> Worksheet:
        """Confort : crée une feuille au nom libre suivant (`Sheet2`, ...)."""
        sheet = Worksheet(name if name is not None else self._next_free_name())
        return self.insert_sheet(len(self._sheets), sheet)

    def remove_sheet(self, ref: str | int | Worksheet) -> tuple[int, Worksheet]:
        sheet = self.sheet(ref)
        index = self._sheets.index(sheet)
        self._sheets.pop(index)
        detach(sheet)
        self._notify_structure(
            StructureChanged(self, "sheet_removed", {"index": index, "sheet": sheet})
        )
        return index, sheet

    def move_sheet(self, ref: str | int | Worksheet, to_index: int) -> int:
        sheet = self.sheet(ref)
        previous = self._sheets.index(sheet)
        to_index = max(0, min(to_index, len(self._sheets) - 1))
        if to_index != previous:
            self._sheets.pop(previous)
            self._sheets.insert(to_index, sheet)
            self._notify_structure(
                StructureChanged(
                    self, "sheet_moved", {"from_index": previous, "to_index": to_index}
                )
            )
        return previous

    def rename_sheet(self, ref: str | int | Worksheet, name: str) -> str:
        sheet = self.sheet(ref)
        check_name(name)
        if name != sheet.name:
            self._check_free_name(name)
        return sheet._set_name(name)

    # -- utilitaires ------------------------------------------------------

    def _check_free_name(self, name: str) -> None:
        if any(sheet.name == name for sheet in self._sheets):
            raise DuplicateNameError(
                f"une feuille nommée {name!r} existe déjà dans le classeur {self.name!r}"
            )

    def _next_free_name(self) -> str:
        taken = set(self.sheet_names())
        index = 1
        while f"Sheet{index}" in taken:
            index += 1
        return f"Sheet{index}"


class Folder(ModelObject):
    """Dossier de l'arbre projet."""

    type_name = "folder"

    def __init__(
        self, name: str = "Folder1", children: Sequence[ModelObject] | None = None
    ) -> None:
        super().__init__(name)
        self._children: list[ModelObject] = []
        for child in children or ():
            self.insert_child(len(self._children), child)

    # -- lecture ----------------------------------------------------------

    def children(self) -> tuple[ModelObject, ...]:
        return tuple(self._children)

    def __len__(self) -> int:
        return len(self._children)

    def __iter__(self) -> Iterator[ModelObject]:
        return iter(self._children)

    @property
    def folders(self) -> tuple[Folder, ...]:
        return tuple(c for c in self._children if isinstance(c, Folder))

    @property
    def windows(self) -> tuple[WindowNode, ...]:
        return tuple(c for c in self._children if isinstance(c, WindowNode))

    def child(self, name: str) -> ModelObject:
        for child in self._children:
            if child.name == name:
                return child
        raise KeyError(f"aucun enfant nommé {name!r} dans le dossier {self.name!r}")

    def folder(self, name: str) -> Folder:
        child = self.child(name)
        if not isinstance(child, Folder):
            raise KeyError(f"{name!r} n'est pas un dossier de {self.name!r}")
        return child

    # -- mutations primitives ---------------------------------------------

    def insert_child(self, index: int, child: ModelObject) -> ModelObject:
        if not isinstance(child, Folder | WindowNode):
            raise StructureError(
                f"un dossier contient des dossiers ou des fenêtres, pas {child!r}"
            )
        if child.parent is not None:
            raise StructureError(f"{child.name!r} appartient déjà à {child.parent!r}")
        if child is self or child in set(self.ancestors()):
            raise StructureError("un dossier ne peut pas se contenir lui-même")
        self._check_free_name(child)
        index = max(0, min(index, len(self._children)))
        self._children.insert(index, child)
        attach(child, self)
        self._notify_structure(
            StructureChanged(self, "child_inserted", {"index": index, "child": child})
        )
        return child

    def add_child(self, child: ModelObject) -> ModelObject:
        return self.insert_child(len(self._children), child)

    def remove_child(self, child: ModelObject | str) -> tuple[int, ModelObject]:
        node = self.child(child) if isinstance(child, str) else child
        index = self._children.index(node)
        self._children.pop(index)
        detach(node)
        self._notify_structure(
            StructureChanged(self, "child_removed", {"index": index, "child": node})
        )
        return index, node

    def move_child(self, child: ModelObject | str, to_index: int) -> int:
        node = self.child(child) if isinstance(child, str) else child
        previous = self._children.index(node)
        to_index = max(0, min(to_index, len(self._children) - 1))
        if to_index != previous:
            self._children.pop(previous)
            self._children.insert(to_index, node)
            self._notify_structure(
                StructureChanged(
                    self, "child_moved", {"from_index": previous, "to_index": to_index}
                )
            )
        return previous

    def rename_child(self, child: ModelObject | str, name: str) -> str:
        node = self.child(child) if isinstance(child, str) else child
        if node not in self._children:
            raise KeyError(f"{node!r} n'est pas un enfant de {self.name!r}")
        check_name(name)
        if name != node.name:
            self._check_free_name(node, name)
        return node._set_name(name)

    # -- utilitaires ------------------------------------------------------

    def _check_free_name(self, child: ModelObject, name: str | None = None) -> None:
        """Unicité : projet entier pour une fenêtre, fratrie pour un dossier."""
        name = child.name if name is None else name
        if isinstance(child, WindowNode):
            project = self.project
            if project is not None:
                existing = project.find_window(name)
                if existing is not None and existing is not child:
                    raise DuplicateNameError(
                        f"une fenêtre nommée {name!r} existe déjà dans le projet"
                    )
                return
        for sibling in self._children:
            if sibling is not child and sibling.name == name:
                raise DuplicateNameError(
                    f"{name!r} existe déjà dans le dossier {self.name!r}"
                )
