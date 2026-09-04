"""`ModelCommand` : la base de toutes les commandes annulables de PlotYx.

Trois responsabilités, et rien d'autre :

1. **Marquer l'exécution** — `redo`/`undo` s'exécutent dans le contexte
   `plotyx.commands.context`, ce qui rend le garde-fou capable de
   distinguer une mutation légitime d'un contournement.
2. **Appliquer avant d'empiler** — `QUndoStack.push()` appelle `redo()` de
   l'autre côté de la frontière C++, où une exception Python est au mieux
   imprévisible, au pire avalée. Le bus applique donc la commande *lui-même*
   (`apply_before_push`) : si elle échoue, elle échoue proprement et la pile
   n'a jamais été touchée. Le premier `redo()` déclenché par Qt est alors
   sauté, une fois.
3. **Retenir les objets détachés** — voir `retained_objects` / `release`.

Durée de vie des objets supprimés (le point délicat laissé ouvert par la
phase 1). `ObjectRegistry` référence faiblement : un objet retiré de l'arbre
n'est plus retenu par personne. C'est la commande qui doit le maintenir en
vie entre son `redo` (suppression) et un éventuel `undo`. Deux échecs à
éviter :

* **la fuite** — une pile non bornée retiendrait indéfiniment des colonnes
  de plusieurs millions de points. D'où `setUndoLimit(N)` et, quand une
  commande est évincée, l'appel à `release()` par le bus ;
* **la clé morte** — une clé de registre qui ne résout plus alors que le
  drag & drop (phase 7) ou la console (phase 9) la détient encore. C'est
  l'inverse : ne *pas* retenir assez. Le compromis retenu est de garder une
  référence forte tant que la commande est sur la pile, et de la lâcher au
  moment exact où elle en sort — plus tôt casserait l'annulation, plus tard
  serait une fuite.
"""

from __future__ import annotations

from PySide6.QtGui import QUndoCommand

from plotyx.commands.context import executing
from plotyx.core.node import ModelObject

__all__ = ["CommandReleasedError", "ModelCommand"]


class CommandReleasedError(RuntimeError):
    """Une commande évincée de la pile a été rejouée. Bug d'appelant."""


class ModelCommand(QUndoCommand):
    """Commande de mutation du modèle, réversible par delta."""

    def __init__(self, text: str) -> None:
        super().__init__(text)
        # Doublon volontaire de `QUndoCommand.text()` : une commande évincée
        # a son objet C++ détruit, et tout appel de méthode Qt sur le
        # wrapper lèverait alors. Les attributs Python, eux, survivent.
        self._text = text
        self._skip_next_redo = False
        self._released = False

    # -- contrat des sous-classes -----------------------------------------

    def _apply(self) -> None:
        """Applique la mutation en appelant les primitives de `core`."""
        raise NotImplementedError

    def _revert(self) -> None:
        """Annule la mutation, à partir du delta enregistré par `_apply`."""
        raise NotImplementedError

    # -- exécution ---------------------------------------------------------

    def apply_before_push(self) -> None:
        """Première application, hors de la pile. Voir le docstring du module."""
        self._check_live()
        with executing():
            self._apply()
        self._skip_next_redo = True

    def redo(self) -> None:  # noqa: N802 - API Qt
        if self._skip_next_redo:
            self._skip_next_redo = False
            return
        self._check_live()
        with executing():
            self._apply()

    def undo(self) -> None:  # noqa: N802 - API Qt
        self._check_live()
        with executing():
            self._revert()

    def _check_live(self) -> None:
        if self._released:
            raise CommandReleasedError(f"commande évincée de la pile : {self._text!r}")

    # -- rétention ---------------------------------------------------------

    def retained_objects(self) -> tuple[ModelObject, ...]:
        """Objets détachés de l'arbre que cette commande maintient en vie."""
        return ()

    def release(self) -> None:
        """Lâche les références retenues. Appelé par le bus à l'éviction.

        Après cet appel la commande n'est plus rejouable : elle n'est de
        toute façon plus atteignable depuis la pile.
        """
        self._released = True

    @property
    def released(self) -> bool:
        return self._released
