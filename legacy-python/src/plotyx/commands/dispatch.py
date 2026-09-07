"""Aiguillage vers les **primitives** de conteneur, pas vers les façades.

`Project.add` / `remove` / `rename` / `move` sont documentées en tête de
`core/project.py` comme des façades d'aiguillage sans logique propre, et la
phase 2 doit envelopper les primitives sous-jacentes. Ce module est cet
aiguillage, côté commandes : il traduit `(conteneur, opération)` en appel de
la primitive du conteneur — `insert_column`, `remove_sheet`,
`rename_child`, … — chacune retournant son delta d'inversion et émettant
exactement une notification.

Pourquoi ne pas simplement réutiliser les façades de `Project` ? Parce que
`Project.move` est un composite (retrait + insertion) qui masque les deux
deltas dont l'annulation a besoin, et que `Project.remove` ne dit pas dans
quel conteneur remettre l'objet. Les commandes ont besoin des primitives
nues.

Le jeu est **fermé** : trois conteneurs, quatre primitives chacun. Toute
extension future de l'arbre (une matrice qui contiendrait des feuilles, par
exemple) se déclare ici et nulle part ailleurs.
"""

from __future__ import annotations

from plotyx.core.errors import StructureError
from plotyx.core.node import ModelObject
from plotyx.core.nodes import Folder, Workbook
from plotyx.core.worksheet import Worksheet

__all__ = [
    "child_index",
    "insert_primitive",
    "move_primitive",
    "remove_primitive",
    "rename_primitive",
]

#: conteneur → (insert, remove, move, rename)
_PRIMITIVES: dict[type, tuple[str, str, str, str]] = {
    Worksheet: ("insert_column", "remove_column", "move_column", "rename_column"),
    Workbook: ("insert_sheet", "remove_sheet", "move_sheet", "rename_sheet"),
    Folder: ("insert_child", "remove_child", "move_child", "rename_child"),
}


def _names(parent: ModelObject) -> tuple[str, str, str, str]:
    for container, primitives in _PRIMITIVES.items():
        if isinstance(parent, container):
            return primitives
    raise StructureError(f"{type(parent).__name__} n'est pas un conteneur du modèle")


def insert_primitive(parent: ModelObject, index: int, obj: ModelObject) -> ModelObject:
    """`insert_column` / `insert_sheet` / `insert_child` selon le conteneur."""
    return getattr(parent, _names(parent)[0])(index, obj)


def remove_primitive(parent: ModelObject, obj: ModelObject) -> tuple[int, ModelObject]:
    """`remove_*`. Retourne `(index, objet)` — le delta d'inversion."""
    return getattr(parent, _names(parent)[1])(obj)


def move_primitive(parent: ModelObject, obj: ModelObject, to_index: int) -> int:
    """`move_*` dans le même conteneur. Retourne l'index précédent."""
    return getattr(parent, _names(parent)[2])(obj, to_index)


def rename_primitive(parent: ModelObject | None, obj: ModelObject, name: str) -> str:
    """`rename_*` — le conteneur valide l'unicité. Retourne l'ancien nom.

    Sans conteneur (racine du projet, objet détaché), on tombe sur la
    primitive `ModelObject._set_name`, celle-là même que les `rename_*`
    appellent après validation.
    """
    if parent is None:
        return obj._set_name(name)
    return getattr(parent, _names(parent)[3])(obj, name)


def child_index(parent: ModelObject, obj: ModelObject) -> int:
    """Position de `obj` dans `parent`."""
    children = parent.children()
    for index, child in enumerate(children):
        if child is obj:
            return index
    raise StructureError(f"{obj!r} n'est pas un enfant de {parent!r}")
