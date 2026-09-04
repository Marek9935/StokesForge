"""`plotyx.io._text` : détection d'encodage et heuristique binaire, isolément."""

from __future__ import annotations

from plotyx.io._text import decode_best_effort, detect_encoding, looks_binary


def test_looks_binary_empty_is_false():
    assert looks_binary(b"") is False


def test_looks_binary_null_byte_is_true():
    assert looks_binary(b"hello\x00world") is True


def test_looks_binary_plain_text_is_false():
    assert looks_binary("bonjour, séparateur; décimal\r\n1,0;2,0\r\n".encode()) is False


def test_looks_binary_random_bytes_is_true():
    assert looks_binary(bytes(range(200)) * 20) is True


def test_detect_encoding_empty_returns_default():
    assert detect_encoding(b"", default="utf-8") == "utf-8"


def test_detect_encoding_low_confidence_falls_back_to_default():
    # Une poignée d'octets ambigus : chardet ne peut pas trancher avec confiance.
    assert detect_encoding(b"\x00\x01", default="ascii") == "ascii"


def test_detect_encoding_invalid_codec_name_falls_back(monkeypatch):
    import plotyx.io._text as text_module

    fake_guess = {"encoding": "not-a-real-codec", "confidence": 0.9}
    monkeypatch.setattr(text_module.chardet, "detect", lambda raw: fake_guess)
    assert detect_encoding(b"whatever", default="utf-8") == "utf-8"


def test_decode_best_effort_falls_back_to_latin1_on_bad_hint():
    text, used = decode_best_effort("café".encode(), encoding_hint="ascii")
    assert used == "latin-1"
    assert text  # décodé sans lever, même si le résultat n'est pas "café"


def test_decode_best_effort_uses_valid_hint_directly():
    text, used = decode_best_effort("café".encode(), encoding_hint="utf-8")
    assert used == "utf-8"
    assert text == "café"
