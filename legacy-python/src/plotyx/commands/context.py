"""Contexte d'exécution des commandes : « suis-je en train d'appliquer une commande ? »

Un simple compteur de profondeur, au niveau du module. Il est renseigné par
:class:`plotyx.commands.base.ModelCommand` autour de chaque `redo`/`undo`,
et lu par le garde-fou (`plotyx.commands.guard`).

Le compteur vit ici plutôt que sur le bus pour une raison précise : Qt
appelle `redo()` lui-même depuis `QUndoStack.push()`, `undo()` et `redo()`,
sans passer par le bus. Un drapeau porté par le bus laisserait donc des
angles morts — typiquement un `stack.undo()` appelé directement par un
`QAction` de la phase 3.

Mono-thread par construction : les commandes s'exécutent sur le thread UI
(la règle « aucun calcul lourd sur le thread UI » de la phase 8 s'applique
au *calcul*, pas à l'application du delta).
"""

from __future__ import annotations

from collections.abc import Iterator
from contextlib import contextmanager

__all__ = ["executing", "execution_depth", "is_executing"]

_depth = 0


def is_executing() -> bool:
    """Vrai si une commande est en cours d'application ou d'annulation."""
    return _depth > 0


def execution_depth() -> int:
    return _depth


@contextmanager
def executing() -> Iterator[None]:
    """Marque le bloc comme exécuté « sous commande »."""
    global _depth
    _depth += 1
    try:
        yield
    finally:
        _depth -= 1
