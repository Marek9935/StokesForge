"""Persistance HDF5 — phase 4.

Format `.plx` (HDF5, `h5py`), schéma `/project` versionné dès le premier
octet écrit. Voir `plotyx.persistence.schema` pour l'arborescence complète,
`plotyx.persistence.writer` / `.reader` pour la lecture/écriture, et
`plotyx.persistence.autosave` pour la sauvegarde périodique et la
récupération après plantage.

**Hors bus, comme `_default_project()`.** Construire un `Project` depuis un
fichier est, comme la construction d'un projet neuf, une opération qui n'a
rien vers quoi annuler. C'est à l'appelant (`ui.main_window`) de remplacer
le projet courant et de repartir d'une pile d'undo neuve.
"""

from plotyx.persistence.autosave import (
    autosave_dir,
    autosave_path,
    autosave_source,
    discard_autosave,
    list_recoverable,
    recover,
    write_autosave,
)
from plotyx.persistence.errors import (
    CorruptedProjectFileError,
    PersistenceError,
    SchemaVersionError,
)
from plotyx.persistence.reader import load_project
from plotyx.persistence.schema import CURRENT_SCHEMA_VERSION, FILE_EXTENSION
from plotyx.persistence.writer import save_project

__all__ = [
    "CURRENT_SCHEMA_VERSION",
    "CorruptedProjectFileError",
    "FILE_EXTENSION",
    "PersistenceError",
    "SchemaVersionError",
    "autosave_dir",
    "autosave_path",
    "autosave_source",
    "discard_autosave",
    "list_recoverable",
    "load_project",
    "recover",
    "save_project",
    "write_autosave",
]
