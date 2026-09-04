"""Garde-fou « aucune mutation du modèle en dehors d'une `QUndoCommand` ».

C'est la règle d'or du §3 de l'architecture, et le premier risque du tableau
de la roadmap : si la console de la phase 9 mute le modèle directement, la
couche métier est à réécrire. Le garde-fou la rend **vérifiable**, pas
seulement documentée.

Mécanique : `plotyx.core.guard` expose un hook appelé par chaque
notification du modèle — donc par chaque primitive de mutation, qui en émet
exactement une. On l'arme avec un callback qui vérifie que le contexte
d'exécution des commandes est actif. Toute mutation faite hors commande lève
alors :class:`MutationOutsideCommandError`, à la ligne exacte du coupable.

Deux usages :

* **les tests** — armé en permanence, il transforme tout contournement en
  échec de test ;
* **une session de debug** — armable à la main, coût nul le reste du temps
  (le hook est `None`, une comparaison par opération primitive).

### Ce que le garde-fou ne voit pas

Il observe les *notifications*, pas la mémoire. Une écriture directe dans
`Column._values` n'émet rien et lui échappe. Ce n'est pas un trou de
conception mais une limite assumée : `Column.values` rend une vue en lecture
seule, donc un tel contournement suppose de viser délibérément un attribut
privé. Le test anti-régression complète le hook par une comparaison d'état
exhaustive du projet, qui, elle, attrape ce cas.
"""

from __future__ import annotations

from collections.abc import Iterator
from contextlib import contextmanager
from typing import Any

from plotyx.commands.context import is_executing
from plotyx.core import guard as core_guard

__all__ = ["MutationOutsideCommandError", "command_guard", "mutation_guard"]


class MutationOutsideCommandError(RuntimeError):
    """Le modèle a été muté sans passer par une commande annulable."""


def _describe(source: Any, event: Any) -> str:
    kind = getattr(event, "kind", None) or type(event).__name__
    return f"mutation hors commande sur {source!r} ({kind})"


@contextmanager
def mutation_guard(*, strict: bool = True) -> Iterator[list[str]]:
    """Arme le garde-fou le temps du bloc `with`.

    Rend la liste des infractions constatées. En mode `strict` (défaut), la
    première lève immédiatement ; sinon elles sont seulement collectées, ce
    qui sert à auditer du code existant sans l'interrompre.
    """
    breaches: list[str] = []

    def hook(source: Any, event: Any) -> None:
        if is_executing():
            return
        message = _describe(source, event)
        breaches.append(message)
        if strict:
            raise MutationOutsideCommandError(message)

    with core_guard.mutation_hook(hook):
        yield breaches


#: Alias lisible côté appelant : `with command_guard(): ...`
command_guard = mutation_guard
