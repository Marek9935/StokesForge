"""Détection d'encodage et heuristique binaire, partagées par les parseurs."""

from __future__ import annotations

import chardet

__all__ = ["decode_best_effort", "detect_encoding", "looks_binary"]

#: Octets considérés « texte » pour l'heuristique binaire : imprimables + CR/LF/TAB/BS/FF.
_TEXT_BYTES = frozenset(range(32, 127)) | {ord("\n"), ord("\r"), ord("\t"), 8, 12}


def looks_binary(raw: bytes) -> bool:
    """Heuristique rapide : `raw` ressemble-t-il à du binaire, pas du texte ?

    Un octet nul est un indice fort (jamais légitime dans du texte). Sinon,
    une proportion trop élevée d'octets hors de la plage imprimable/contrôle
    usuelle (>15 % sur les 4 premiers kio) l'indique aussi. Sert à écarter
    les `.dat` propriétaires qui ne sont du texte qu'en tête (en-tête façon
    INI suivi d'un payload binaire) — un piège réel du corpus.
    """
    if not raw:
        return False
    if b"\x00" in raw:
        return True
    sample = raw[:4096]
    nontext = sum(byte not in _TEXT_BYTES for byte in sample)
    return nontext / len(sample) > 0.15


def detect_encoding(raw: bytes, default: str = "utf-8") -> str:
    """Encodage détecté par `chardet`, avec repli si la confiance est faible."""
    if not raw:
        return default
    guess = chardet.detect(raw)
    encoding = guess.get("encoding")
    confidence = guess.get("confidence") or 0.0
    if not encoding or confidence < 0.5:
        return default
    try:
        "".encode(encoding)
    except LookupError:
        return default
    return encoding


def decode_best_effort(raw: bytes, encoding_hint: str | None = None) -> tuple[str, str]:
    """Décode `raw`. Rend `(texte, encodage réellement utilisé)`.

    Jamais d'exception : un encodage détecté mais invalide sur ces octets
    précis retombe sur `latin-1`, qui ne peut pas échouer (toute suite
    d'octets y a un sens, un octet = un caractère).
    """
    encoding = encoding_hint or detect_encoding(raw)
    try:
        return raw.decode(encoding), encoding
    except (LookupError, UnicodeDecodeError):
        return raw.decode("latin-1", errors="replace"), "latin-1"
