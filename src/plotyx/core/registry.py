"""`ObjectRegistry` : clé opaque stable ↔ objet du modèle.

C'est le point d'appui du drag & drop par référence (phase 7 : le
`QMimeData` transporte des clés, jamais des données) et de la console de
script (phase 9).

**Invariant central** : une clé est un UUID attribué à la construction de
l'objet et n'est fonction ni de son nom, ni de son chemin, ni de sa
position. Renommer ou déplacer un objet ne touche donc pas au registre : la
clé reste valide et continue de résoudre vers le même objet. Une clé n'est
retirée que lorsque l'objet quitte l'arbre du projet.

Les objets sont référencés **faiblement**. Conséquence voulue pour la
phase 2 : une colonne supprimée disparaît du registre, mais tant que la
`QUndoCommand` de suppression la retient, l'objet vit ; le `redo`/`undo` la
réinscrit sous *la même clé*, et toute référence enregistrée ailleurs
(série de graphe, variable de console) redevient valide.
"""

from __future__ import annotations

import weakref
from collections.abc import Iterator

from plotyx.core.errors import UnknownKeyError
from plotyx.core.node import ModelObject

__all__ = ["ObjectRegistry"]


class ObjectRegistry:
    """Table clé → objet, sans copie et sans rétention forte."""

    def __init__(self) -> None:
        self._objects: weakref.WeakValueDictionary[str, ModelObject] = (
            weakref.WeakValueDictionary()
        )

    def __len__(self) -> int:
        return len(self._objects)

    def __contains__(self, key: object) -> bool:
        return key in self._objects

    def __iter__(self) -> Iterator[str]:
        return iter(dict(self._objects))

    def keys(self) -> tuple[str, ...]:
        return tuple(self._objects.keys())

    def register(self, obj: ModelObject) -> str:
        """Inscrit `obj` sous sa clé propre. Idempotent. Retourne la clé."""
        existing = self._objects.get(obj.key)
        if existing is not None and existing is not obj:
            raise ValueError(f"collision de clé {obj.key!r} entre {existing!r} et {obj!r}")
        self._objects[obj.key] = obj
        return obj.key

    def unregister(self, obj: ModelObject) -> None:
        """Retire `obj`. Silencieux s'il n'était pas inscrit."""
        if self._objects.get(obj.key) is obj:
            del self._objects[obj.key]

    def resolve(self, key: str) -> ModelObject:
        """Objet inscrit sous `key`. Lève `UnknownKeyError` sinon."""
        try:
            return self._objects[key]
        except KeyError:
            raise UnknownKeyError(key) from None

    def get(self, key: str, default: ModelObject | None = None) -> ModelObject | None:
        return self._objects.get(key, default)

    def clear(self) -> None:
        self._objects.clear()
