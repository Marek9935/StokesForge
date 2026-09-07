"""`CsvParser` : détection et lecture, sur les pièges de format reproduits en fixtures."""

from __future__ import annotations

from pathlib import Path

import numpy as np
import pytest

from plotyx.io.csv_parser import (
    CsvParser,
    detect_data_start,
    detect_decimal,
    detect_header,
    guess_delimiter,
)
from plotyx.io.parser import ImportCancelled

FIXTURES = Path(__file__).parent / "fixtures" / "import"


def test_can_parse_scores_text_extensions_and_rejects_binary():
    parser = CsvParser()
    assert parser.can_parse(FIXTURES / "generic_named_header.csv") > 0.5
    assert parser.can_parse(FIXTURES / "rpe_nohead_tab.asc") > 0.5
    assert parser.can_parse(FIXTURES / "proprietary_binary.dat") == 0.0


def test_can_parse_missing_file_returns_zero(tmp_path):
    parser = CsvParser()
    assert parser.can_parse(tmp_path / "absent.csv") == 0.0


def test_parse_generic_named_header():
    result = CsvParser().parse(FIXTURES / "generic_named_header.csv")
    assert result.ok
    assert [c.name for c in result.columns] == ["Time", "Signal"]
    assert result.columns[0].role.value == "X"
    assert result.columns[1].role.value == "Y"
    np.testing.assert_allclose(result.columns[0].values[:3], [0.0, 0.1, 0.2])
    np.testing.assert_allclose(result.columns[1].values[:3], [0.0, 0.01, 0.04])


def test_parse_no_header_semicolon_comma_decimal():
    """Piège analytikjena : `;` délimiteur ET `,` décimal, aucun en-tête."""
    result = CsvParser().parse(FIXTURES / "analytikjena_nohead_semicolon.csv")
    assert result.ok
    assert len(result.columns) == 2
    codes = {w.code for w in result.warnings}
    assert "decimal_guess" in codes
    x, y = result.columns
    np.testing.assert_allclose(x.values[:3], [250.0, 251.0, 252.0])
    assert y.values[0] == pytest.approx(1.0639, abs=1e-4)


def test_parse_tab_no_header_dot_decimal():
    """RPE : TAB, aucun en-tête, point décimal — pas de faux positif sur la virgule."""
    result = CsvParser().parse(FIXTURES / "rpe_nohead_tab.asc")
    assert result.ok
    codes = {w.code for w in result.warnings}
    assert "decimal_guess" not in codes
    x, y = result.columns
    np.testing.assert_allclose(x.values[:2], [3331.5, 3331.573])


def test_jcamp_lookalike_is_handled_by_generic_parser_not_tripped_by_hashes():
    """Piège IR : `##CLE=valeur` façon JCAMP-DX, sans en être un — ne doit pas planter."""
    result = CsvParser().parse(FIXTURES / "ir_jcamp_lookalike.txt")
    assert result.ok
    assert len(result.columns) == 2
    np.testing.assert_allclose(result.columns[0].values[:2], [401.19, 405.05], atol=1e-2)


def test_wide_matrix_header_row_detected():
    """IrradiationSetup : matrice large, en-tête `wavelength\t0\t1\t2\t...`."""
    result = CsvParser().parse(FIXTURES / "irradiation_wide_matrix.txt")
    assert result.ok
    assert len(result.columns) == 31
    assert result.columns[0].name == "wavelength"
    assert result.columns[1].name == "0"
    # 20 lignes de données dans la fixture ; l'en-tête ne doit pas en faire une 21e.
    assert len(result.columns[0].values) == 20
    np.testing.assert_allclose(result.columns[0].values[:2], [400.0, 401.0])


def test_binary_dat_is_rejected_with_structured_error():
    result = CsvParser().parse(FIXTURES / "proprietary_binary.dat")
    assert not result.ok
    assert result.columns == []
    assert any(w.code == "binary_file" for w in result.warnings)


def test_empty_file_is_a_structured_error(tmp_path):
    empty = tmp_path / "empty.csv"
    empty.write_text("")
    result = CsvParser().parse(empty)
    assert not result.ok
    assert result.columns == []


def test_missing_file_is_a_structured_error(tmp_path):
    result = CsvParser().parse(tmp_path / "does_not_exist.csv")
    assert not result.ok
    assert any(w.code == "io_error" for w in result.warnings)


def test_manual_overrides_bypass_detection():
    """Un override explicite (délimiteur, décimal, skiprows) prime sur l'auto-détection."""
    options = {"delimiter": ";", "decimal": ",", "skiprows": 0}
    result = CsvParser().parse(FIXTURES / "analytikjena_nohead_semicolon.csv", options)
    assert result.ok
    assert len(result.columns) == 2


# -- fonctions de détection, isolément --------------------------------------------


def test_guess_delimiter_prefers_semicolon_over_comma_decimal_trap():
    lines = ["250,000000;1,063894", "251,000000;1,074138", "252,000000;1,090229"]
    assert guess_delimiter(lines) == ";"


def test_guess_delimiter_detects_tab():
    lines = ["1.0\t2.0", "3.0\t4.0", "5.0\t6.0"]
    assert guess_delimiter(lines) == "\t"


def test_detect_data_start_skips_metadata_lines():
    lines = ["TITLE;Foo", "XUNITS;nm", "1.0;2.0", "3.0;4.0", "5.0;6.0"]
    assert detect_data_start(lines, ";") == 2


def test_detect_header_recognises_numeric_looking_column_labels():
    lines = ["wavelength\t0\t1", "400.0\t0.1\t0.2", "401.0\t0.3\t0.4"]
    data_start = detect_data_start(lines, "\t")
    assert data_start == 1
    header_index, names = detect_header(lines, "\t", data_start)
    assert header_index == 0
    assert names == ["wavelength", "0", "1"]


def test_detect_decimal_defaults_to_dot_when_delimiter_is_comma():
    assert detect_decimal(["1.0,2.0"], 0, ",") == "."


# -- lecture en flux : gros fichiers, progression, annulation ----------------------


def _write_big_csv(path: Path, rows: int) -> None:
    lines = ["X,Y"] + [f"{i},{i * 2.0}" for i in range(rows)]
    path.write_text("\n".join(lines) + "\n")


def test_chunked_reading_matches_direct_reading(tmp_path):
    path = tmp_path / "big.csv"
    _write_big_csv(path, 5_000)

    direct = CsvParser().parse(path)
    chunked = CsvParser().parse(path, {"chunk_size": 777})

    assert len(direct.columns[0].values) == len(chunked.columns[0].values) == 5_000
    np.testing.assert_allclose(direct.columns[1].values, chunked.columns[1].values)


def test_chunked_reading_reports_progress(tmp_path):
    path = tmp_path / "big.csv"
    _write_big_csv(path, 3_000)
    seen: list[float] = []
    CsvParser().parse(path, {"chunk_size": 500, "progress": seen.append})
    assert seen, "la progression doit être appelée au moins une fois"
    assert seen[-1] == pytest.approx(1.0)
    assert all(0.0 <= f <= 1.0 for f in seen)
    assert seen == sorted(seen)


def test_chunked_reading_can_be_cancelled(tmp_path):
    path = tmp_path / "big.csv"
    _write_big_csv(path, 10_000)
    calls = {"n": 0}

    def should_cancel() -> bool:
        calls["n"] += 1
        return calls["n"] > 1  # laisse passer le premier chunk, annule au suivant

    with pytest.raises(ImportCancelled):
        CsvParser().parse(path, {"chunk_size": 500, "should_cancel": should_cancel})
