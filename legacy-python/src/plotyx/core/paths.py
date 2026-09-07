"""Chemins d'objets, grammaire Origin `[Book1]Sheet1!ColA`.

Formes reconnues, de la plus large à la plus fine :

| Chemin | Désigne |
|---|---|
| `/` | le dossier racine |
| `/Manips/Jour1` | un dossier |
| `[Book1]` | un classeur (ou un graphe, une matrice, une note) |
| `[Book1]Sheet1` | une feuille |
| `[Book1]Sheet1!ColA` | une colonne |
| `/Manips/[Book1]Sheet1!ColA` | idem, avec le dossier explicite |

Le préfixe dossier est **optionnel et purement indicatif** : les noms de
fenêtres étant uniques dans tout le projet, `[Book1]Sheet1!ColA` suffit
toujours. Quand il est présent, la résolution vérifie qu'il correspond,
sinon le chemin est rejeté — c'est ce qui permet de coller un chemin
complet depuis l'explorateur de projet sans risque d'homonymie silencieuse.
"""

from __future__ import annotations

from dataclasses import dataclass

from plotyx.core.errors import PathError
from plotyx.core.node import RESERVED_NAME_CHARS

__all__ = ["ObjectPath", "format_path", "parse_path"]


def _check_segment(segment: str, kind: str) -> str:
    if not segment or segment != segment.strip():
        raise PathError(f"{kind} vide ou mal détouré dans le chemin")
    for char in RESERVED_NAME_CHARS:
        if char in segment:
            raise PathError(f"caractère réservé {char!r} dans le {kind} {segment!r}")
    return segment


@dataclass(frozen=True)
class ObjectPath:
    """Chemin analysé. Les champs valent None quand ils sont absents."""

    folders: tuple[str, ...] = ()
    window: str | None = None
    sheet: str | None = None
    column: str | None = None
    rooted: bool = False

    def __str__(self) -> str:
        return format_path(self)

    @property
    def depth(self) -> str:
        """Le niveau le plus fin désigné : folder, window, sheet ou column."""
        if self.column is not None:
            return "column"
        if self.sheet is not None:
            return "sheet"
        if self.window is not None:
            return "window"
        return "folder"


def format_path(path: ObjectPath) -> str:
    """Rend la forme textuelle canonique d'un `ObjectPath`."""
    prefix = "/".join(path.folders)
    if path.rooted:
        prefix = "/" + prefix
    if path.window is None:
        return prefix or "/"
    if prefix and not prefix.endswith("/"):
        prefix += "/"
    text = f"{prefix}[{path.window}]"
    if path.sheet is not None:
        text += path.sheet
    if path.column is not None:
        text += f"!{path.column}"
    return text


def parse_path(text: str) -> ObjectPath:
    """Analyse un chemin. Lève `PathError` si la syntaxe est invalide."""
    if not isinstance(text, str):
        raise PathError(f"chemin non textuel : {text!r}")
    raw = text.strip()
    if not raw:
        raise PathError("chemin vide")

    if "[" not in raw:
        if "]" in raw or "!" in raw:
            raise PathError(f"chemin mal formé : {text!r}")
        rooted = raw.startswith("/")
        folders = tuple(_check_segment(s, "dossier") for s in raw.split("/") if s)
        if not folders and not rooted:
            raise PathError(f"chemin mal formé : {text!r}")
        return ObjectPath(folders=folders, rooted=rooted)

    prefix, _, rest = raw.partition("[")
    if prefix and not prefix.endswith("/"):
        raise PathError(f"préfixe de dossier non terminé par '/' : {text!r}")
    rooted = prefix.startswith("/")
    folders = tuple(_check_segment(s, "dossier") for s in prefix.split("/") if s)

    window, sep, tail = rest.partition("]")
    if not sep:
        raise PathError(f"crochet fermant manquant : {text!r}")
    _check_segment(window, "nom de fenêtre")
    if "[" in tail or "]" in tail:
        raise PathError(f"crochet en trop : {text!r}")

    sheet_part, bang, column_part = tail.partition("!")
    sheet = _check_segment(sheet_part, "nom de feuille") if sheet_part else None
    if bang:
        column = _check_segment(column_part, "nom de colonne")
        if sheet is None:
            raise PathError(f"colonne sans feuille : {text!r}")
    else:
        column = None
    return ObjectPath(folders=folders, window=window, sheet=sheet, column=column, rooted=rooted)
