"""Autosave périodique et recovery au démarrage.

Un fichier indépendant du fichier projet de l'utilisateur, écrit dans un
répertoire temporaire partagé (`tempfile.gettempdir()/plotyx-autosave`,
recouvrable via `PLOTYX_AUTOSAVE_DIR` — la variable existe surtout pour
isoler les tests les uns des autres et d'une éventuelle session interactive
sur la même machine). Une session est identifiée par un `session_id` opaque
(un UUID généré une fois par fenêtre) : c'est le nom du fichier, ce qui
distingue plusieurs instances de PlotYx lancées en parallèle sans qu'elles
se marchent dessus.

**Recovery.** Sur une fermeture propre (sauvegarde ou abandon confirmé par
l'utilisateur), la fenêtre efface son propre fichier d'autosave
(`discard_autosave`). S'il subsiste au prochain démarrage, c'est qu'une
session précédente ne s'est pas terminée proprement (plantage, `kill -9`,
coupure) : `list_recoverable` le retrouve et l'UI propose de le rouvrir.
"""

from __future__ import annotations

import os
import tempfile
from pathlib import Path

from plotyx.core.project import Project
from plotyx.persistence.reader import load_project
from plotyx.persistence.schema import ATTR_AUTOSAVE_SOURCE, FILE_EXTENSION
from plotyx.persistence.writer import save_project

__all__ = [
    "autosave_dir",
    "autosave_path",
    "autosave_source",
    "discard_autosave",
    "list_recoverable",
    "recover",
    "write_autosave",
]

_SUFFIX = f".autosave{FILE_EXTENSION}"


def autosave_dir() -> Path:
    """Répertoire des fichiers d'autosave, créé s'il n'existe pas encore."""
    override = os.environ.get("PLOTYX_AUTOSAVE_DIR")
    root = Path(override) if override else Path(tempfile.gettempdir()) / "plotyx-autosave"
    root.mkdir(parents=True, exist_ok=True)
    return root


def autosave_path(session_id: str) -> Path:
    return autosave_dir() / f"{session_id}{_SUFFIX}"


def write_autosave(project: Project, session_id: str, source_path: Path | None) -> Path:
    """Écrit l'état courant du projet dans le fichier d'autosave de la session."""
    path = autosave_path(session_id)
    save_project(
        project,
        path,
        extra_attrs={ATTR_AUTOSAVE_SOURCE: str(source_path) if source_path else ""},
    )
    return path


def discard_autosave(session_id: str) -> None:
    """Efface le fichier d'autosave de la session — après un save ou un abandon confirmé."""
    autosave_path(session_id).unlink(missing_ok=True)


def list_recoverable(*, exclude_session: str | None = None) -> list[Path]:
    """Fichiers d'autosave laissés par des sessions qui ne se sont pas terminées proprement."""
    exclude = autosave_path(exclude_session) if exclude_session else None
    files = sorted(autosave_dir().glob(f"*{_SUFFIX}"))
    return [f for f in files if f != exclude]


def recover(path: Path) -> Project:
    """Charge un fichier d'autosave comme n'importe quel fichier projet."""
    return load_project(path)


def autosave_source(path: Path) -> str:
    """Chemin d'origine mémorisé dans un fichier d'autosave, ou `""` si inédit."""
    import h5py

    with h5py.File(path, "r") as file:
        value = file.attrs.get(ATTR_AUTOSAVE_SOURCE, "")
        return value.decode("utf-8") if isinstance(value, bytes) else str(value)
