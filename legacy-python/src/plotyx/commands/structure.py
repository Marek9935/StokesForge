"""Commandes de mutation de la **structure** de l'arbre projet.

Ce sont elles qui portent le point délicat de la phase 2 : la durée de vie
des objets sortis de l'arbre. `ObjectRegistry` référence faiblement, donc
un objet détaché n'est plus retenu que par la commande qui l'a détaché.
Chaque commande de ce module garde donc une **référence forte** sur l'objet
concerné tant qu'elle est sur la pile, et la lâche dans `release()`, appelé
par le bus au moment exact où la commande en sort (éviction par
`setUndoLimit`, troncature de la branche de redo, ou `clear()`).

Corollaire important pour les phases 7 et 9 : tant qu'une suppression est
annulable, la clé de registre de l'objet supprimé **ne résout plus** (il
n'est plus dans l'arbre) mais l'objet est vivant, et l'annulation le
réinscrit *sous la même clé*. Une référence détenue ailleurs (série de
graphe, variable de console, `QMimeData` en vol) redevient donc valide.
"""

from __future__ import annotations

from plotyx.commands.base import ModelCommand
from plotyx.commands.dispatch import (
    child_index,
    insert_primitive,
    move_primitive,
    remove_primitive,
    rename_primitive,
)
from plotyx.core.errors import StructureError
from plotyx.core.node import ModelObject
from plotyx.core.worksheet import Column, Worksheet

__all__ = [
    "AddColumnCommand",
    "AddObjectCommand",
    "DeleteColumnCommand",
    "DeleteObjectCommand",
    "MoveCommand",
    "RenameCommand",
]


class _StructureCommand(ModelCommand):
    """Base des commandes qui retiennent un objet hors de l'arbre."""

    def __init__(self, text: str, parent: ModelObject, obj: ModelObject) -> None:
        super().__init__(text)
        self._parent: ModelObject | None = parent
        self._object: ModelObject | None = obj
        self._detached = False

    @property
    def object(self) -> ModelObject | None:
        return self._object

    def retained_objects(self) -> tuple[ModelObject, ...]:
        """L'objet, seulement s'il est actuellement hors de l'arbre."""
        if self._detached and self._object is not None:
            return (self._object,)
        return ()

    def release(self) -> None:
        super().release()
        self._object = None
        self._parent = None


class AddObjectCommand(_StructureCommand):
    """Insère un objet déjà construit dans un conteneur.

    Entre le `undo` (l'objet ressort de l'arbre) et un éventuel `redo`,
    c'est cette commande — et elle seule — qui maintient l'objet en vie.
    """

    def __init__(
        self,
        parent: ModelObject,
        obj: ModelObject,
        index: int | None = None,
        text: str = "",
    ) -> None:
        super().__init__(text or f"Ajout de {obj.name}", parent, obj)
        self._index = index
        self._detached = True  # pas encore inséré

    def _apply(self) -> None:
        index = len(self._parent.children()) if self._index is None else self._index
        insert_primitive(self._parent, index, self._object)
        self._index = child_index(self._parent, self._object)
        self._detached = False

    def _revert(self) -> None:
        self._index, _ = remove_primitive(self._parent, self._object)
        self._detached = True


class DeleteObjectCommand(_StructureCommand):
    """Retire un objet de son conteneur, en le maintenant en vie pour l'undo."""

    def __init__(self, obj: ModelObject, text: str = "") -> None:
        parent = obj.parent
        if parent is None:
            raise StructureError(f"{obj!r} n'a pas de conteneur")
        super().__init__(text or f"Suppression de {obj.name}", parent, obj)
        self._index = child_index(parent, obj)

    def _apply(self) -> None:
        self._index, _ = remove_primitive(self._parent, self._object)
        self._detached = True

    def _revert(self) -> None:
        insert_primitive(self._parent, self._index, self._object)
        self._detached = False


class AddColumnCommand(AddObjectCommand):
    """Ajoute une colonne à une feuille (spécialisation nommée par la roadmap)."""

    def __init__(
        self,
        sheet: Worksheet,
        column: Column,
        index: int | None = None,
        text: str = "",
    ) -> None:
        if not isinstance(sheet, Worksheet):
            raise StructureError(f"{sheet!r} n'est pas une feuille")
        super().__init__(sheet, column, index, text or f"Ajout de la colonne {column.name}")


class DeleteColumnCommand(DeleteObjectCommand):
    """Supprime une colonne d'une feuille, valeurs comprises.

    Aucun snapshot : c'est l'objet `Column` lui-même, avec son `ndarray`,
    qui est retenu. Retirer puis remettre une colonne d'un million de points
    ne copie donc pas un octet.
    """

    def __init__(self, sheet: Worksheet, ref: str | int | Column, text: str = "") -> None:
        column = sheet.column(ref)
        super().__init__(column, text or f"Suppression de la colonne {column.name}")


class RenameCommand(ModelCommand):
    """Renomme un objet via la primitive de son conteneur (qui valide l'unicité)."""

    def __init__(self, obj: ModelObject, name: str, text: str = "") -> None:
        super().__init__(text or f"Renommage de {obj.name}")
        self._object = obj
        self._name = name
        self._previous: str | None = None

    def _apply(self) -> None:
        self._previous = rename_primitive(self._object.parent, self._object, self._name)

    def _revert(self) -> None:
        rename_primitive(self._object.parent, self._object, self._previous)


class MoveCommand(ModelCommand):
    """Déplace un objet : réordonnancement dans son conteneur, ou changement de conteneur.

    Deux régimes, parce que les primitives ne sont pas les mêmes :

    * **même conteneur** — une seule primitive, `move_*`, qui retourne
      l'index précédent : annulation par paramètre, rien à retenir ;
    * **conteneur différent** — composite de deux primitives, `remove_*`
      puis `insert_*`. La validation (type de conteneur compatible, absence
      de cycle, nom libre à l'arrivée) est faite **avant** de détacher quoi
      que ce soit, sans quoi un échec laisserait l'objet orphelin.

    La clé de registre est préservée dans les deux cas : c'est l'invariant
    de la phase 1 dont dépendent le drag & drop (phase 7) et la console.
    """

    def __init__(
        self,
        obj: ModelObject,
        new_parent: ModelObject | None = None,
        index: int | None = None,
        text: str = "",
    ) -> None:
        super().__init__(text or f"Déplacement de {obj.name}")
        parent = obj.parent
        if parent is None:
            raise StructureError(f"{obj!r} n'a pas de conteneur")
        self._object = obj
        self._old_parent = parent
        self._new_parent = parent if new_parent is None else new_parent
        self._index = index
        self._old_index = child_index(parent, obj)
        self._reparent = self._new_parent is not parent
        if self._reparent:
            self._check_move()

    def _check_move(self) -> None:
        """Refuse un déplacement invalide avant toute mutation."""
        project = self._object.project
        if project is None:
            raise StructureError(f"{self._object!r} n'appartient à aucun projet")
        project._check_move(self._object, self._new_parent)

    def _apply(self) -> None:
        if not self._reparent:
            target = self._old_index if self._index is None else self._index
            self._old_index = move_primitive(self._old_parent, self._object, target)
            return
        self._check_move()
        self._old_index, _ = remove_primitive(self._old_parent, self._object)
        target = (
            len(self._new_parent.children()) if self._index is None else self._index
        )
        try:
            insert_primitive(self._new_parent, target, self._object)
        except Exception:  # pragma: no cover - filet : on remet l'objet en place
            insert_primitive(self._old_parent, self._old_index, self._object)
            raise
        self._index = child_index(self._new_parent, self._object)

    def _revert(self) -> None:
        if not self._reparent:
            move_primitive(self._old_parent, self._object, self._old_index)
            return
        remove_primitive(self._new_parent, self._object)
        insert_primitive(self._old_parent, self._old_index, self._object)

    def retained_objects(self) -> tuple[ModelObject, ...]:
        """Aucune : un déplacement laisse toujours l'objet dans l'arbre."""
        return ()
