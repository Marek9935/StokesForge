"""`JascoParser` : en-tête clé/valeur + marqueur `XYDATA`, virgule/point décimal."""

from __future__ import annotations

from pathlib import Path

import numpy as np
import pytest

from plotyx.io.csv_parser import CsvParser
from plotyx.io.jasco_parser import JascoParser

FIXTURES = Path(__file__).parent / "fixtures" / "import"


def test_can_parse_scores_higher_than_generic_csv_on_jasco_files():
    jasco, csv = JascoParser(), CsvParser()
    for name in ("jasco_uvvis_semicolon.csv", "jasco_fluo_tab.txt"):
        path = FIXTURES / name
        assert jasco.can_parse(path) > csv.can_parse(path)


def test_can_parse_rejects_non_jasco_files():
    jasco = JascoParser()
    assert jasco.can_parse(FIXTURES / "generic_named_header.csv") == 0.0
    assert jasco.can_parse(FIXTURES / "ir_jcamp_lookalike.txt") == 0.0


def test_parse_semicolon_comma_decimal_uvvis():
    result = JascoParser().parse(FIXTURES / "jasco_uvvis_semicolon.csv")
    assert result.ok
    assert len(result.columns) == 2
    x, y = result.columns
    assert x.unit == "nm"
    np.testing.assert_allclose(x.values, [800.0, 798.0, 796.0, 794.0, 792.0, 790.0])
    assert y.values[0] == pytest.approx(0.099146)
    assert result.metadata["ORIGIN"] == "JASCO"
    assert result.metadata["XUNITS"] == "NANOMETERS"


def test_parse_tab_comma_decimal_fluorescence():
    result = JascoParser().parse(FIXTURES / "jasco_fluo_tab.txt")
    assert result.ok
    x, y = result.columns
    np.testing.assert_allclose(x.values, [300.0, 301.0, 302.0, 303.0, 304.0, 305.0])
    np.testing.assert_allclose(y.values, [12.5, 12.7, 12.9, 13.05, 13.3, 13.6])
    assert result.metadata["DATA TYPE"] == "FLUORESCENCE SPECTRUM"


def test_npoints_mismatch_is_a_structured_warning_not_an_exception():
    result = JascoParser().parse(FIXTURES / "jasco_npoints_mismatch.txt")
    assert result.ok  # un NPOINTS erroné n'est pas fatal
    assert len(result.columns[0].values) == 4
    codes = [w.code for w in result.warnings]
    assert "npoints_mismatch" in codes
    mismatch = next(w for w in result.warnings if w.code == "npoints_mismatch")
    assert "99" in mismatch.message and "4" in mismatch.message


def test_decimal_separator_is_not_hardcoded_to_locale():
    """LOCALE=1033 (anglo-saxon) + données à point : ne doit PAS forcer la virgule."""
    result = JascoParser().parse(FIXTURES / "jasco_english_locale_dot.csv")
    assert result.ok
    assert result.metadata["LOCALE"] == "1033"
    codes = {w.code for w in result.warnings}
    assert "decimal_guess" not in codes
    np.testing.assert_allclose(result.columns[0].values[:2], [600.0, 599.0])
    np.testing.assert_allclose(result.columns[1].values[:2], [0.2, 0.201])


def test_missing_xydata_marker_is_a_structured_error(tmp_path):
    text = "TITLE;NoData\r\nORIGIN;JASCO\r\nXUNITS;NANOMETERS\r\n"
    path = tmp_path / "no_marker.csv"
    path.write_bytes(text.encode("latin-1"))
    result = JascoParser().parse(path)
    assert not result.ok
    assert result.columns == []
    assert any(w.code == "no_xydata" for w in result.warnings)


def test_empty_file_is_a_structured_error(tmp_path):
    path = tmp_path / "empty.csv"
    path.write_bytes(b"")
    result = JascoParser().parse(path)
    assert not result.ok


def test_metadata_carries_instrument_fields():
    result = JascoParser().parse(FIXTURES / "jasco_uvvis_semicolon.csv")
    for key in ("TITLE", "XUNITS", "YUNITS", "FIRSTX", "LASTX", "NPOINTS"):
        assert key in result.metadata
