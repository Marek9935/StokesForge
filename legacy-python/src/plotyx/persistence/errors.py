"""Exceptions de la couche de persistance."""

from __future__ import annotations

from plotyx.core.errors import PlotyxError

__all__ = [
    "CorruptedProjectFileError",
    "PersistenceError",
    "SchemaVersionError",
]


class PersistenceError(PlotyxError):
    """Base des erreurs de lecture/écriture d'un fichier projet."""


class CorruptedProjectFileError(PersistenceError):
    """Le fichier n'est pas (ou plus) un fichier projet PlotYx valide."""


class SchemaVersionError(PersistenceError):
    """Le fichier porte un numéro de schéma que cette version ne sait pas lire.

    Toujours levée proprement — jamais un plantage — pour le cas décrit par
    la roadmap : un utilisateur ouvre, avec une version ancienne de PlotYx,
    un projet enregistré par une version plus récente.
    """
