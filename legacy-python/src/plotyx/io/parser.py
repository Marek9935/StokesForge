"""`FileParser` : le contrat que tout parseur du registre doit remplir.

Deux méthodes, symétriques à la description de la roadmap :

* `can_parse(path)` — un score dans `[0, 1]` sans lire le fichier en entier
  (juste un aperçu). `0` = ne sait pas le lire, `1` = certitude. Le registre
  choisit le meilleur score, pas le premier qui accepte.
* `parse(path, options)` — fait le travail. `options` est un simple
  dictionnaire (pas un type Qt) : overrides du wizard, callables de
  progression/annulation pour les gros fichiers.
"""

from __future__ import annotations

from abc import ABC, abstractmethod
from pathlib import Path
from typing import Any

from plotyx.io.result import ImportResult

__all__ = ["FileParser", "ImportCancelled", "ImportOptions"]

#: Clés reconnues dans le dictionnaire d'options, à titre de documentation.
#: `progress`   : callable(fraction: float) -> None, appelé pendant le parsing.
#: `should_cancel` : callable() -> bool, vérifié entre les chunks.
#: `chunk_size` : nombre de lignes par chunk pour la lecture en flux.
#: `encoding`, `delimiter`, `decimal`, `skiprows`, `header` : overrides manuels.
ImportOptions = dict[str, Any]


class ImportCancelled(RuntimeError):
    """Levée par un parseur quand `options["should_cancel"]()` devient vrai.

    Ce n'est *pas* un warning structuré : une annulation n'est pas un
    fichier malformé, c'est une demande explicite de l'utilisateur. Le
    wizard l'attrape pour fermer proprement, sans toucher au modèle.
    """


class FileParser(ABC):
    """Un parseur pluggable du registre."""

    #: Nom court, stable, utilisé dans `ImportResult.parser_name` et les logs.
    name: str = "generic"

    @abstractmethod
    def can_parse(self, path: Path) -> float:
        """Score de confiance `[0, 1]` sans lever d'exception.

        Une erreur de lecture (fichier absent, permissions) doit rendre `0`,
        pas propager — c'est le registre qui décide si `0` partout est un
        problème pour l'appelant.
        """

    @abstractmethod
    def parse(self, path: Path, options: ImportOptions | None = None) -> ImportResult:
        """Lit `path`. Ne lève que pour une erreur *fatale* (I/O, annulation).

        Un fichier lisible mais mal formé rend un `ImportResult` avec des
        `ImportWarning` — jamais une exception.
        """
