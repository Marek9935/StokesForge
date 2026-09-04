"""`CommandBus` : une `QUndoStack` par projet, plus ce que Qt ne fait pas.

Qt fournit la pile, le regroupement en macros et le signal `cleanChanged`.
Il ne fournit pas :

* **une application sûre** — `QUndoStack.push()` appelle `redo()` de l'autre
  côté de la frontière C++ ; une exception Python y est au mieux
  imprévisible. Le bus applique donc la commande avant de l'empiler ;
* **la libération des objets retenus** — quand `setUndoLimit(N)` évince la
  commande du bas de la pile, Qt détruit l'objet C++, mais rien ne garantit
  ni la date ni l'ordre de libération des références Python qu'elle
  détenait. Le bus tient un miroir de la pile et appelle `release()` sur
  toute commande qui en est sortie : éviction, troncature de la branche de
  redo, ou `clear()` ;
* **l'abandon d'une macro** — `QUndoStack` ne sait pas annuler un
  `beginMacro()` en cours. Voir :meth:`abort_macro`.

Le bus ne construit **aucune UI** : il expose `clean_changed` /
`modified_changed` et l'état `is_modified`, à charge pour la phase 3 d'en
faire un astérisque de titre et un prompt de sauvegarde.
"""

from __future__ import annotations

from collections.abc import Iterator, Sequence
from contextlib import contextmanager

from PySide6.QtCore import QObject, Signal
from PySide6.QtGui import QUndoCommand, QUndoStack

from plotyx.commands.base import ModelCommand
from plotyx.core.node import ModelObject
from plotyx.core.project import Project

__all__ = ["CommandBus"]


class _AbortMarker(QUndoCommand):
    """Commande jetable : tronque la branche de redo, puis se retire.

    `QUndoStack.push()` purge la branche de redo avant d'empiler, et
    supprime immédiatement une commande qui se déclare obsolète pendant son
    `redo()`. La pousser laisse donc la pile exactement telle qu'elle était,
    moins la branche de redo — ce qu'on cherche pour effacer une macro
    abandonnée.
    """

    def __init__(self) -> None:
        super().__init__("(abandon)")

    def redo(self) -> None:  # noqa: N802 - API Qt
        self.setObsolete(True)

    def undo(self) -> None:  # noqa: N802 - API Qt
        """Jamais appelée : la commande ne reste pas sur la pile."""


class _Entry:
    """Une entrée *de premier niveau* de la pile et toutes ses commandes.

    Pour une macro, `top` est la commande parente créée par Qt et `members`
    la liste des commandes filles — celles qui retiennent réellement quelque
    chose. Pour une commande simple, les deux se confondent.
    """

    __slots__ = ("top", "members")

    def __init__(self, top: QUndoCommand, members: Sequence[ModelCommand]) -> None:
        self.top = top
        self.members = list(members)

    def release(self) -> None:
        for command in self.members:
            command.release()


class CommandBus(QObject):
    """Bus de commandes d'un projet."""

    #: Relais direct de `QUndoStack.cleanChanged` (vrai = document sauvegardé).
    clean_changed = Signal(bool)
    #: Le même, inversé — c'est celui que veut un titre de fenêtre.
    modified_changed = Signal(bool)

    def __init__(
        self,
        project: Project,
        *,
        undo_limit: int = 0,
        parent: QObject | None = None,
    ) -> None:
        super().__init__(parent)
        self._project = project
        self._stack = QUndoStack(self)
        self._entries: list[_Entry] = []
        self._macro_members: list[list[ModelCommand]] = []
        if undo_limit:
            self._stack.setUndoLimit(undo_limit)
        self._stack.cleanChanged.connect(self._on_clean_changed)

    def __repr__(self) -> str:  # pragma: no cover - confort de debug
        return (
            f"<CommandBus project={self._project.name!r} "
            f"count={self._stack.count()} index={self._stack.index()}>"
        )

    # -- accès -------------------------------------------------------------

    @property
    def project(self) -> Project:
        return self._project

    @property
    def stack(self) -> QUndoStack:
        """La pile Qt.

        La phase 3 s'en sert directement pour `createUndoAction()` /
        `createRedoAction()` et pour s'abonner à `indexChanged` ; le bus ne
        duplique pas ces signaux.
        """
        return self._stack

    @property
    def count(self) -> int:
        return self._stack.count()

    @property
    def index(self) -> int:
        return self._stack.index()

    @property
    def can_undo(self) -> bool:
        return self._stack.canUndo()

    @property
    def can_redo(self) -> bool:
        return self._stack.canRedo()

    @property
    def undo_text(self) -> str:
        return self._stack.undoText()

    @property
    def redo_text(self) -> str:
        return self._stack.redoText()

    # -- limite d'historique ------------------------------------------------

    @property
    def undo_limit(self) -> int:
        """Nombre maximal d'entrées d'historique (`0` = illimité)."""
        return self._stack.undoLimit()

    @undo_limit.setter
    def undo_limit(self, limit: int) -> None:
        if limit < 0:
            raise ValueError(f"limite d'historique négative : {limit}")
        if self._stack.count():
            # Qt l'exige : changer la limite sur une pile non vide pourrait
            # détruire la commande à l'index courant.
            raise RuntimeError("la limite d'historique ne se change que sur une pile vide")
        self._stack.setUndoLimit(limit)

    # -- état « document modifié » ------------------------------------------

    @property
    def is_modified(self) -> bool:
        """Vrai si l'état courant diffère du dernier point de sauvegarde."""
        return not self._stack.isClean()

    def set_clean(self) -> None:
        """Marque l'état courant comme sauvegardé (à appeler après un save)."""
        self._stack.setClean()

    def _on_clean_changed(self, clean: bool) -> None:
        self.clean_changed.emit(clean)
        self.modified_changed.emit(not clean)

    # -- exécution -----------------------------------------------------------

    def push(self, command: ModelCommand) -> ModelCommand:
        """Applique `command` puis l'empile. Retourne la commande.

        L'application a lieu **avant** l'empilement : si la mutation échoue
        (nom en double, index hors bornes), l'exception remonte normalement
        et la pile n'a pas bougé.
        """
        if not isinstance(command, ModelCommand):
            raise TypeError(f"le bus n'accepte que des ModelCommand, pas {command!r}")
        command.apply_before_push()
        if self._macro_members:
            self._macro_members[-1].append(command)
            self._stack.push(command)
        else:
            self._stack.push(command)
            self._entries.append(_Entry(command, [command]))
        self._reconcile()
        return command

    def undo(self) -> None:
        self._stack.undo()

    def redo(self) -> None:
        self._stack.redo()

    def clear(self) -> None:
        """Vide l'historique. Tout ce qui n'était retenu que par lui est libéré."""
        self._stack.clear()
        self._reconcile()

    # -- macros ---------------------------------------------------------------

    @contextmanager
    def macro(self, text: str) -> Iterator[CommandBus]:
        """Regroupe tout ce qui est poussé dans le bloc en **une** entrée.

        C'est ce qui permet à un import (phase 5) de tenir en une seule ligne
        d'historique. En cas d'exception, la macro partielle est annulée et
        effacée de la pile — voir :meth:`abort_macro`.
        """
        self.begin_macro(text)
        try:
            yield self
        except BaseException:
            self.abort_macro()
            raise
        else:
            self.end_macro()

    def begin_macro(self, text: str) -> None:
        self._macro_members.append([])
        self._stack.beginMacro(text)

    def end_macro(self) -> None:
        if not self._macro_members:
            raise RuntimeError("end_macro() sans begin_macro()")
        members = self._macro_members.pop()
        self._stack.endMacro()
        if self._macro_members:
            # Macro imbriquée : ses commandes appartiennent à la macro
            # englobante, qui seule constituera une entrée.
            self._macro_members[-1].extend(members)
            return
        top = self._stack.command(self._stack.count() - 1)
        self._entries.append(_Entry(top, members))
        self._reconcile()

    def abort_macro(self) -> None:
        """Annule et efface une macro en cours.

        `QUndoStack` n'a pas de `cancelMacro()`. La séquence est donc :
        fermer la macro, l'annuler (ce qui rejoue les `undo()` des commandes
        déjà appliquées), puis pousser une commande obsolète qui purge la
        branche de redo et disparaît. La pile retrouve son état d'avant le
        `begin_macro`, et les objets retenus par les commandes effacées sont
        libérés.
        """
        if not self._macro_members:
            raise RuntimeError("abort_macro() sans begin_macro()")
        depth = len(self._macro_members)
        for _ in range(depth):
            self._macro_members.pop()
            self._stack.endMacro()
        self._stack.undo()
        self._stack.push(_AbortMarker())
        self._reconcile()

    @property
    def in_macro(self) -> bool:
        return bool(self._macro_members)

    # -- rétention -------------------------------------------------------------

    def retained_objects(self) -> tuple[ModelObject, ...]:
        """Objets hors de l'arbre que l'historique maintient en vie.

        Introspection : c'est le poste mémoire de l'undo sur les
        suppressions. Utile en test et pour un futur indicateur de debug.
        """
        held: list[ModelObject] = []
        for entry in self._entries:
            for command in entry.members:
                held.extend(command.retained_objects())
        for pending in self._macro_members:
            for command in pending:
                held.extend(command.retained_objects())
        return tuple(held)

    def _reconcile(self) -> None:
        """Libère les commandes qui ne sont plus sur la pile.

        Trois causes possibles : l'éviction par `setUndoLimit`, la troncature
        de la branche de redo lors d'un nouveau `push`, et `clear()`. Qt ne
        signale aucune des trois — d'où ce miroir, comparé par identité au
        contenu réel de la pile.
        """
        live = [self._stack.command(i) for i in range(self._stack.count())]
        live_ids = {id(command) for command in live}
        kept, dropped = [], []
        for entry in self._entries:
            (kept if id(entry.top) in live_ids else dropped).append(entry)
        self._entries = kept
        for entry in dropped:
            entry.release()
