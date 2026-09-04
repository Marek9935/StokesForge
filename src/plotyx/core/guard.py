"""Hook de mutation optionnel — garde-fou de la règle d'or.

Le README pose comme principe non négociable qu'**aucune mutation du modèle
ne se fait en dehors d'une `QUndoCommand`**. Ce module fournit le point
d'observation qui permet de le *vérifier*, sans que `plotyx.core` ait à
connaître Qt ni la couche de commandes.

Fonctionnement : chaque primitive de mutation émet exactement une
notification (`ModelObject._notify_data` / `_notify_structure`). Ces deux
méthodes appellent le hook installé ici, s'il y en a un.

**Coût en production : nul ou presque.** Le hook vaut `None` par défaut ;
le seul surcoût est une lecture d'attribut de module et une comparaison à
`None` par notification — c'est-à-dire par *opération primitive*, jamais par
cellule. Aucune instrumentation, aucun décorateur, aucun état accumulé.

La couche `plotyx.commands.guard` s'en sert pour lever une exception dès
qu'une mutation survient hors de l'exécution d'une commande. C'est un outil
de debug et de test, pas un mécanisme de sécurité : une écriture directe
dans `Column._values` n'émet aucune notification et passe donc sous le
radar. Le test anti-régression complète le hook par une comparaison d'état
exhaustive pour couvrir ce cas.

Limite connue : le hook est un état de module, donc **global au processus**
et non protégé par verrou. Il est destiné à un usage mono-thread (tests,
session de debug), pas à une exécution concurrente.
"""

from __future__ import annotations

from collections.abc import Callable, Iterator
from contextlib import contextmanager
from typing import Any

__all__ = ["MutationHook", "get_mutation_hook", "mutation_hook", "set_mutation_hook"]

#: Signature d'un hook : `(source, event)` où `event` est un `DataChanged`
#: ou un `StructureChanged`.
MutationHook = Callable[[Any, Any], None]

#: Hook courant. Lu directement par `plotyx.core.node` — délibérément public
#: pour éviter un appel de fonction supplémentaire sur le chemin chaud.
hook: MutationHook | None = None


def set_mutation_hook(new_hook: MutationHook | None) -> MutationHook | None:
    """Installe (ou retire, avec `None`) le hook. Retourne le précédent."""
    global hook
    previous, hook = hook, new_hook
    return previous


def get_mutation_hook() -> MutationHook | None:
    return hook


@contextmanager
def mutation_hook(new_hook: MutationHook | None) -> Iterator[MutationHook | None]:
    """Installe `new_hook` le temps du bloc `with`, puis restaure l'ancien."""
    previous = set_mutation_hook(new_hook)
    try:
        yield new_hook
    finally:
        set_mutation_hook(previous)
