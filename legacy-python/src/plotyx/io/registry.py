"""`ParserRegistry` : sélection par score, jamais d'`if extension == ...`.

Le registre ne connaît aucun format par son nom : il interroge chaque
parseur enregistré via `can_parse()` et retient le meilleur score. Deux
parseurs concurrents (`CsvParser` générique et `JascoParser` spécialisé)
peuvent tous les deux accepter un même fichier ; c'est le score qui
tranche, pas l'ordre d'enregistrement — sauf égalité stricte, où le premier
enregistré gagne (déterminisme du test).
"""

from __future__ import annotations

from pathlib import Path

from plotyx.io.csv_parser import CsvParser
from plotyx.io.jasco_parser import JascoParser
from plotyx.io.parser import FileParser, ImportOptions
from plotyx.io.result import ImportResult, ImportWarning

__all__ = ["ParserRegistry", "default_registry"]


class ParserRegistry:
    """Registre pluggable de `FileParser`."""

    def __init__(self) -> None:
        self._parsers: list[FileParser] = []

    def register(self, parser: FileParser) -> None:
        self._parsers.append(parser)

    @property
    def parsers(self) -> tuple[FileParser, ...]:
        return tuple(self._parsers)

    def scores(self, path: Path) -> list[tuple[float, FileParser]]:
        """Score de chaque parseur enregistré, triés du meilleur au pire.

        Un parseur dont `can_parse` lève une exception est traité comme
        s'il rendait `0` : une bogue de scoring ne doit jamais faire
        échouer la sélection d'un autre parseur.
        """
        scored: list[tuple[float, FileParser]] = []
        for parser in self._parsers:
            try:
                score = float(parser.can_parse(path))
            except Exception:  # noqa: BLE001 - un parseur cassé ne doit pas planter la sélection
                score = 0.0
            scored.append((score, parser))
        # Tri stable : à score égal, l'ordre d'enregistrement est préservé.
        scored.sort(key=lambda pair: pair[0], reverse=True)
        return scored

    def best_parser(self, path: Path) -> FileParser | None:
        scored = self.scores(path)
        if not scored or scored[0][0] <= 0.0:
            return None
        return scored[0][1]

    def parse(self, path: Path, options: ImportOptions | None = None) -> ImportResult:
        """Sélectionne le meilleur parseur et l'applique.

        Aucun parseur ne se déclare capable : rend un `ImportResult` vide
        avec une erreur structurée plutôt que de lever — cohérent avec le
        contrat « jamais d'exception qui plante l'import » des parseurs
        eux-mêmes.
        """
        parser = self.best_parser(Path(path))
        if parser is None:
            return ImportResult(
                warnings=[
                    ImportWarning(
                        "no_parser",
                        f"aucun parseur ne reconnaît {Path(path).name!r}",
                        severity="error",
                    )
                ]
            )
        return parser.parse(Path(path), options)


def default_registry() -> ParserRegistry:
    """Le registre standard de l'application : Jasco avant le CSV générique.

    L'ordre d'enregistrement ne joue que sur les égalités strictes de score
    (voir `ParserRegistry.scores`) ; en usage réel, `JascoParser` score
    toujours strictement plus haut que `CsvParser` sur un fichier Jasco,
    l'ordre est documentaire.
    """
    registry = ParserRegistry()
    registry.register(JascoParser())
    registry.register(CsvParser())
    return registry
