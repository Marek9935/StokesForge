"""Objet de base du modèle : identité stable, parent, notifications.

Tout ce qui est adressable par le registre ou par un chemin Origin dérive
de :class:`ModelObject` : dossiers, classeurs, feuilles, colonnes, et les
nœuds encore vides (`Graph`, `Matrix`, `Note`).

Deux invariants structurent la suite du projet :

* **L'identité ne dépend ni du nom, ni de la position.** `key` est un
  UUID attribué à la construction et jamais réécrit : renommer ou déplacer
  un objet ne casse aucune référence (drag & drop de phase 7, console de
  phase 9).
* **Les notifications remontent l'arbre.** Un objet émet sur lui-même puis
  sur chacun de ses ancêtres, jusqu'au projet. La vue peut donc s'abonner
  au niveau qui l'arrange.

Les deux méthodes de notification sont aussi le point d'observation du
garde-fou de la phase 2 (`plotyx.core.guard`) : chaque primitive émettant
exactement une notification, un hook branché là voit passer toutes les
mutations du modèle. Désactivé par défaut, coût nul en production.
"""

from __future__ import annotations

import uuid
from collections.abc import Iterator
from dataclasses import dataclass, field
from typing import Any

from plotyx.core import guard
from plotyx.core.errors import InvalidNameError
from plotyx.core.signals import Signal

__all__ = [
    "DataChanged",
    "ModelObject",
    "StructureChanged",
    "attach",
    "check_name",
    "detach",
    "RESERVED_NAME_CHARS",
]

#: Caractères réservés par la grammaire de chemins `[Book]Sheet!Col`.
RESERVED_NAME_CHARS = "[]!/"


def check_name(name: str) -> str:
    """Valide un nom d'objet et le retourne inchangé.

    Un nom est une chaîne non vide, sans espace de bordure, ne contenant
    aucun caractère réservé par la grammaire de chemins.
    """
    if not isinstance(name, str):
        raise InvalidNameError(f"nom non textuel : {name!r}")
    if name != name.strip() or not name:
        raise InvalidNameError(f"nom vide ou mal détouré : {name!r}")
    for char in RESERVED_NAME_CHARS:
        if char in name:
            raise InvalidNameError(f"caractère réservé {char!r} dans le nom {name!r}")
    return name


@dataclass(frozen=True)
class DataChanged:
    """Les valeurs d'une colonne ont changé, la structure non.

    `start`/`stop` délimitent les lignes touchées (`stop` exclu). Quand la
    longueur de la colonne a changé, `length_changed` est vrai — la vue doit
    alors relire le nombre de lignes en plus des cellules.
    """

    source: ModelObject
    start: int = 0
    stop: int = -1
    length_changed: bool = False


@dataclass(frozen=True)
class StructureChanged:
    """La structure ou les métadonnées ont changé.

    `kind` identifie l'opération primitive à l'origine du changement
    (`"column_inserted"`, `"node_renamed"`, ...) ; `details` porte les
    paramètres utiles à la vue. Le jeu de `kind` est volontairement fermé et
    aligné sur les mutations primitives, pour que la phase 2 puisse les
    envelopper une à une dans des `QUndoCommand`.
    """

    source: ModelObject
    kind: str
    details: dict[str, Any] = field(default_factory=dict)


class ModelObject:
    """Base commune : clé stable, nom, parent, signaux."""

    #: Nom lisible du type, utilisé dans les messages et les vues.
    type_name = "object"

    def __init__(self, name: str) -> None:
        self._key = uuid.uuid4().hex
        self._name = check_name(name)
        self._parent: ModelObject | None = None
        self.data_changed = Signal(f"{self.type_name}.data_changed")
        self.structure_changed = Signal(f"{self.type_name}.structure_changed")

    # -- identité ---------------------------------------------------------

    @property
    def key(self) -> str:
        """Clé opaque, stable pour toute la vie de l'objet."""
        return self._key

    @property
    def name(self) -> str:
        return self._name

    def _restore_key(self, key: str) -> None:
        """Reprend une clé déjà attribuée : réservé au chargement de projet.

        La phase 4 persiste la clé de naissance de chaque objet (voir
        `plotyx.persistence`) pour que les références qui la traversent —
        une série de graphe pointant sur une colonne, en phase 6 — restent
        valables après un cycle sauver/fermer/rouvrir. `__init__` a déjà
        attribué un UUID neuf ; ceci le remplace par celui du fichier,
        **avant** que l'objet ne soit attaché à un parent inscrit au
        registre (`attach()` est le seul point d'inscription, sous la clé
        courante à cet instant). Appelé hors bus, à l'identique de la
        construction d'un projet neuf : il n'y a rien à annuler vers.
        """
        self._key = key

    def __repr__(self) -> str:  # pragma: no cover - confort de debug
        return f"<{type(self).__name__} {self._name!r} key={self._key[:8]}>"

    # -- arbre ------------------------------------------------------------

    @property
    def parent(self) -> ModelObject | None:
        return self._parent

    @property
    def root(self) -> ModelObject:
        node: ModelObject = self
        while node._parent is not None:
            node = node._parent
        return node

    @property
    def project(self) -> Any:
        """Le projet propriétaire, ou None si l'objet est détaché."""
        return getattr(self.root, "_owner_project", None)

    def ancestors(self) -> Iterator[ModelObject]:
        node = self._parent
        while node is not None:
            yield node
            node = node._parent

    def children(self) -> tuple[ModelObject, ...]:
        """Enfants directs. Les feuilles renvoient un tuple vide."""
        return ()

    def walk(self) -> Iterator[ModelObject]:
        """Parcours préfixe de l'objet et de toute sa descendance."""
        yield self
        for child in self.children():
            yield from child.walk()

    # -- mutation primitive : renommage -----------------------------------

    def _set_name(self, name: str) -> str:
        """Primitive de renommage. Retourne l'ancien nom (delta d'annulation).

        Ne fait **aucun** contrôle d'unicité : c'est au conteneur (qui seul
        connaît ses frères) d'appeler cette primitive après validation.
        """
        previous = self._name
        self._name = check_name(name)
        self._notify_structure(
            StructureChanged(self, "node_renamed", {"previous": previous, "name": self._name})
        )
        return previous

    # -- notifications ----------------------------------------------------

    def _notify_data(self, event: DataChanged) -> None:
        """Émet `dataChanged` sur l'objet puis sur chacun de ses ancêtres."""
        if guard.hook is not None:  # garde-fou « aucune mutation hors commande »
            guard.hook(self, event)
        self.data_changed.emit(event)
        for ancestor in self.ancestors():
            ancestor.data_changed.emit(event)

    def _notify_structure(self, event: StructureChanged) -> None:
        """Émet `structureChanged` sur l'objet puis sur chacun de ses ancêtres."""
        if guard.hook is not None:  # garde-fou « aucune mutation hors commande »
            guard.hook(self, event)
        self.structure_changed.emit(event)
        for ancestor in self.ancestors():
            ancestor.structure_changed.emit(event)


def attach(child: ModelObject, parent: ModelObject) -> None:
    """Rattache `child` à `parent` et l'inscrit au registre du projet.

    Point de passage unique : c'est ici — et nulle part ailleurs — que
    `_parent` prend une valeur non nulle, et que le registre apprend
    l'existence d'un objet.
    """
    child._parent = parent
    project = parent.project
    if project is not None:
        project._register_subtree(child)


def detach(child: ModelObject) -> None:
    """Détache `child` de son parent et le retire du registre du projet."""
    project = child.project
    if project is not None:
        project._unregister_subtree(child)
    child._parent = None
