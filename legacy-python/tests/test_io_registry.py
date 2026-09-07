"""`ParserRegistry` : sélection par score, jamais d'`if extension == ...`."""

from __future__ import annotations

from pathlib import Path

from plotyx.io.parser import FileParser
from plotyx.io.registry import ParserRegistry, default_registry
from plotyx.io.result import ImportResult

FIXTURES = Path(__file__).parent / "fixtures" / "import"


class _FixedScoreParser(FileParser):
    """Parseur de test : score constant, résultat constant."""

    def __init__(self, name: str, score: float) -> None:
        self.name = name
        self._score = score

    def can_parse(self, path: Path) -> float:
        return self._score

    def parse(self, path: Path, options=None) -> ImportResult:
        return ImportResult(parser_name=self.name)


class _BrokenScoreParser(FileParser):
    """Un `can_parse` qui lève ne doit pas casser la sélection des autres."""

    name = "broken"

    def can_parse(self, path: Path) -> float:
        raise RuntimeError("boom")

    def parse(self, path: Path, options=None) -> ImportResult:  # pragma: no cover - jamais appelé
        raise AssertionError("ne doit jamais être sélectionné")


def test_default_registry_prefers_jasco_over_generic_csv():
    registry = default_registry()
    result = registry.parse(FIXTURES / "jasco_uvvis_semicolon.csv")
    assert result.parser_name == "jasco"


def test_default_registry_falls_back_to_csv_for_jcamp_lookalike():
    """Le piège volontaire : ##CLE=valeur ne doit tromper aucun parseur vers Jasco."""
    registry = default_registry()
    result = registry.parse(FIXTURES / "ir_jcamp_lookalike.txt")
    assert result.parser_name == "csv"
    assert len(result.columns) == 2


def test_registry_scores_are_sorted_descending():
    registry = ParserRegistry()
    registry.register(_FixedScoreParser("low", 0.2))
    registry.register(_FixedScoreParser("high", 0.9))
    scored = registry.scores(FIXTURES / "generic_named_header.csv")
    assert [name for _, name in [(s, p.name) for s, p in scored]] == ["high", "low"]


def test_registry_tie_break_keeps_registration_order():
    registry = ParserRegistry()
    registry.register(_FixedScoreParser("first", 0.5))
    registry.register(_FixedScoreParser("second", 0.5))
    best = registry.best_parser(FIXTURES / "generic_named_header.csv")
    assert best.name == "first"


def test_registry_ignores_a_parser_whose_scoring_raises():
    registry = ParserRegistry()
    registry.register(_BrokenScoreParser())
    registry.register(_FixedScoreParser("ok", 0.4))
    best = registry.best_parser(FIXTURES / "generic_named_header.csv")
    assert best.name == "ok"


def test_no_parser_accepts_returns_structured_error(tmp_path):
    registry = ParserRegistry()
    registry.register(_FixedScoreParser("never", 0.0))
    result = registry.parse(tmp_path / "whatever.xyz")
    assert not result.ok
    assert any(w.code == "no_parser" for w in result.warnings)


def test_binary_file_has_no_matching_parser():
    registry = default_registry()
    result = registry.parse(FIXTURES / "proprietary_binary.dat")
    assert not result.ok
    assert result.columns == []
