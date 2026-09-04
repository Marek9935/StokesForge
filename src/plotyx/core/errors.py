"""Exceptions du noyau métier."""

from __future__ import annotations

__all__ = [
    "PlotyxError",
    "DuplicateNameError",
    "InvalidNameError",
    "PathError",
    "StructureError",
    "UnknownKeyError",
]


class PlotyxError(Exception):
    """Base de toutes les erreurs métier de PlotYx."""


class InvalidNameError(PlotyxError, ValueError):
    """Nom d'objet syntaxiquement invalide (vide, ou caractère réservé)."""


class DuplicateNameError(PlotyxError, ValueError):
    """Nom déjà pris dans le conteneur cible."""


class PathError(PlotyxError, ValueError):
    """Chemin `[Book]Sheet!Col` mal formé ou non résolu."""


class StructureError(PlotyxError, ValueError):
    """Opération de structure impossible (mauvais type de parent, cycle, ...)."""


class UnknownKeyError(PlotyxError, KeyError):
    """Clé absente du registre d'objets."""
