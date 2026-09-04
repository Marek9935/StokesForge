"""`PlotSpec` : description d'un graphe **indépendante de tout moteur de rendu**.

C'est la décision d'architecture la plus coûteuse à revenir dessus du
projet, et la roadmap l'exige ici plutôt qu'en phase 10 : PyQtGraph (écran,
phase 6) et Matplotlib (export publication, phase 10) doivent être deux
*renderers* d'une même description, sinon les deux rendus divergent et
chaque option de style est à implémenter deux fois, dans deux vocabulaires.

## Ce qui garantit la neutralité

1. **Aucun import de Qt, de PyQtGraph, de Matplotlib — ni même de numpy**
   dans ce module. Il ne manipule que des `str`, `float`, `bool`, `tuple`
   et des `StrEnum`. `tests/test_plot_spec_is_engine_neutral.py` vérifie
   l'import dans un interpréteur où ces trois paquets sont rendus
   introuvables.
2. **Aucun vocabulaire d'un moteur.** Pas de `'o'` ni de `symbol=`,
   pas de `Qt.PenStyle`, pas de `linestyle='--'` : des énumérations
   nommées (`SymbolShape.CIRCLE`, `LineStyle.DASH`) que chaque renderer
   traduit chez lui. La table de traduction vit dans le renderer, jamais
   ici.
3. **Aucune donnée.** Une série porte la **clé de registre** de ses
   colonnes (UUID de naissance, phase 1, persisté en phase 4), jamais un
   tableau. La résolution clé → colonne est faite par
   `plotyx.plot.resolve`, qui est le seul module du paquet à connaître
   `plotyx.core`.
4. **Immuable.** Toutes les dataclasses sont `frozen` : une modification
   de style produit un *nouveau* `PlotSpec` (voir `plotyx.plot.edit`).
   C'est ce qui rend l'undo trivial et sans copie de données — le delta
   d'annulation est l'ancien spec, qui ne pèse que ses chaînes.

## Ce que la couche décrit

```
PlotSpec
├── title, background
└── panels : PanelSpec  (multi-panneaux : grille row/column)
    ├── title
    ├── axes : bottom / left / right / top  → AxisSpec (échelle, limites, grille)
    ├── legend : LegendSpec
    └── series : SeriesSpec
        ├── x_key / y_key / x_error_key / y_error_key  (clés de registre)
        ├── plot_type : scatter | line | line+scatter | bar | histogram | error_bar
        ├── line : LineSpec        (style, épaisseur, couleur)
        ├── symbol : SymbolSpec    (forme, taille, couleurs)
        └── axis : left | right    (axe secondaire)
```

## Sérialisation

`to_dict()` / `from_dict()` ne produisent que des types JSON. C'est ce que
la persistance HDF5 écrit dans l'attribut `plot_spec_json` du groupe d'un
`Graph` (schéma 2). Un champ inconnu à la relecture est **ignoré** plutôt
que fatal : un projet écrit par une version plus récente reste ouvrable si
le numéro de schéma le permet.
"""

from __future__ import annotations

import uuid
from collections.abc import Iterator
from dataclasses import dataclass, field, fields, replace
from enum import StrEnum
from typing import Any

__all__ = [
    "AxisScale",
    "AxisSide",
    "AxisSpec",
    "DEFAULT_PALETTE",
    "LegendPosition",
    "LegendSpec",
    "LineSpec",
    "LineStyle",
    "PanelSpec",
    "PlotSpec",
    "PlotType",
    "SeriesSpec",
    "SymbolShape",
    "SymbolSpec",
    "new_id",
    "palette_color",
]


def new_id() -> str:
    """Identifiant opaque d'une série ou d'un panneau, stable et sérialisé.

    Distinct de la clé de registre d'un objet du modèle : il identifie une
    *entrée du spec*, pas un objet du projet. Deux séries peuvent tracer la
    même colonne avec des styles différents.
    """
    return uuid.uuid4().hex


class PlotType(StrEnum):
    """Type de tracé d'une série. Jeu fermé (roadmap phase 6)."""

    SCATTER = "scatter"
    LINE = "line"
    LINE_SCATTER = "line+scatter"
    BAR = "bar"
    HISTOGRAM = "histogram"
    ERROR_BAR = "error_bar"

    @property
    def draws_line(self) -> bool:
        return self in (PlotType.LINE, PlotType.LINE_SCATTER, PlotType.ERROR_BAR)

    @property
    def draws_symbols(self) -> bool:
        return self in (PlotType.SCATTER, PlotType.LINE_SCATTER, PlotType.ERROR_BAR)

    @property
    def needs_x(self) -> bool:
        """`False` pour un histogramme : il ne consomme qu'une variable."""
        return self is not PlotType.HISTOGRAM


class AxisScale(StrEnum):
    LINEAR = "linear"
    LOG = "log"


class AxisSide(StrEnum):
    BOTTOM = "bottom"
    LEFT = "left"
    RIGHT = "right"
    TOP = "top"


class LineStyle(StrEnum):
    NONE = "none"
    SOLID = "solid"
    DASH = "dash"
    DOT = "dot"
    DASH_DOT = "dash_dot"


class SymbolShape(StrEnum):
    NONE = "none"
    CIRCLE = "circle"
    SQUARE = "square"
    TRIANGLE = "triangle"
    DIAMOND = "diamond"
    PLUS = "plus"
    CROSS = "cross"


class LegendPosition(StrEnum):
    TOP_LEFT = "top_left"
    TOP_RIGHT = "top_right"
    BOTTOM_LEFT = "bottom_left"
    BOTTOM_RIGHT = "bottom_right"


#: Palette par défaut, en hexadécimal `#RRGGBB` — la forme la plus neutre
#: qui soit : PyQtGraph comme Matplotlib l'acceptent telle quelle, et elle
#: se sérialise en JSON sans conversion.
DEFAULT_PALETTE: tuple[str, ...] = (
    "#4c78a8",
    "#f58518",
    "#54a24b",
    "#e45756",
    "#72b7b2",
    "#eeca3b",
    "#b279a2",
    "#ff9da6",
    "#9d755d",
    "#bab0ac",
)


def palette_color(index: int) -> str:
    """Couleur de la palette par défaut, cyclique."""
    return DEFAULT_PALETTE[index % len(DEFAULT_PALETTE)]


def _coerce(value: Any, enum_type: type[StrEnum]) -> StrEnum:
    return value if isinstance(value, enum_type) else enum_type(str(value))


def _as_dict(obj: Any) -> Any:
    """Conversion récursive vers des types JSON, sans `dataclasses.asdict`.

    `asdict()` recopierait aussi les tuples en listes de façon opaque et
    perdrait le contrôle sur les `StrEnum` ; ici la sortie est explicite et
    stable, ce qui est ce qu'on attend d'un format persisté.
    """
    if isinstance(obj, StrEnum):
        return str(obj)
    if isinstance(obj, tuple | list):
        return [_as_dict(item) for item in obj]
    if hasattr(obj, "__dataclass_fields__"):
        return {f.name: _as_dict(getattr(obj, f.name)) for f in fields(obj)}
    return obj


def _known(cls: type, data: dict[str, Any]) -> dict[str, Any]:
    """Filtre les champs inconnus : un spec plus récent reste lisible."""
    names = {f.name for f in fields(cls)}
    return {k: v for k, v in data.items() if k in names}


@dataclass(frozen=True)
class LineSpec:
    """Trait d'une courbe. `LineStyle.NONE` = pas de trait du tout."""

    color: str = DEFAULT_PALETTE[0]
    style: LineStyle = LineStyle.SOLID
    width: float = 1.5

    def __post_init__(self) -> None:
        object.__setattr__(self, "style", _coerce(self.style, LineStyle))
        object.__setattr__(self, "width", float(self.width))

    def to_dict(self) -> dict[str, Any]:
        return _as_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> LineSpec:
        return cls(**_known(cls, data))


@dataclass(frozen=True)
class SymbolSpec:
    """Marqueur d'un point. `SymbolShape.NONE` = aucun marqueur."""

    shape: SymbolShape = SymbolShape.CIRCLE
    size: float = 7.0
    color: str = DEFAULT_PALETTE[0]
    #: Contour du marqueur. `""` signifie « la même couleur que le
    #: remplissage » — un renderer n'a jamais à deviner une valeur nulle.
    edge_color: str = ""

    def __post_init__(self) -> None:
        object.__setattr__(self, "shape", _coerce(self.shape, SymbolShape))
        object.__setattr__(self, "size", float(self.size))

    def to_dict(self) -> dict[str, Any]:
        return _as_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> SymbolSpec:
        return cls(**_known(cls, data))


@dataclass(frozen=True)
class SeriesSpec:
    """Une série : des **clés de registre**, un type de tracé, un style.

    `y_key` est obligatoire ; `x_key` à `None` signifie « abscisse
    implicite » (l'indice de la ligne), ce qui est le comportement d'Origin
    quand aucune colonne X n'est désignée.
    """

    y_key: str
    x_key: str | None = None
    x_error_key: str | None = None
    y_error_key: str | None = None
    plot_type: PlotType = PlotType.LINE
    #: Nom affiché en légende. `""` = dérivé de la colonne à l'affichage,
    #: donc automatiquement à jour après un renommage de colonne.
    name: str = ""
    line: LineSpec = field(default_factory=LineSpec)
    symbol: SymbolSpec = field(default_factory=SymbolSpec)
    #: Axe des ordonnées auquel la série est rattachée (axe secondaire).
    axis: AxisSide = AxisSide.LEFT
    visible: bool = True
    show_in_legend: bool = True
    #: Largeur des barres ; `0.0` = déduite du pas des abscisses.
    bar_width: float = 0.0
    histogram_bins: int = 40
    id: str = field(default_factory=new_id)

    def __post_init__(self) -> None:
        object.__setattr__(self, "plot_type", _coerce(self.plot_type, PlotType))
        axis = _coerce(self.axis, AxisSide)
        if axis not in (AxisSide.LEFT, AxisSide.RIGHT):
            raise ValueError(f"une série se rattache à l'axe gauche ou droit, pas à {axis!r}")
        object.__setattr__(self, "axis", axis)
        if not self.y_key:
            raise ValueError("une série doit désigner une colonne Y par sa clé de registre")
        object.__setattr__(self, "bar_width", float(self.bar_width))
        object.__setattr__(self, "histogram_bins", max(1, int(self.histogram_bins)))

    # -- lecture ------------------------------------------------------------

    def keys(self) -> tuple[str, ...]:
        """Toutes les clés de registre référencées, dans l'ordre x, y, erreurs."""
        candidates = (self.x_key, self.y_key, self.x_error_key, self.y_error_key)
        return tuple(key for key in candidates if key)

    def references(self, key: str) -> bool:
        return key in self.keys()

    # -- sérialisation ------------------------------------------------------

    def to_dict(self) -> dict[str, Any]:
        return _as_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> SeriesSpec:
        data = dict(_known(cls, data))
        if isinstance(data.get("line"), dict):
            data["line"] = LineSpec.from_dict(data["line"])
        if isinstance(data.get("symbol"), dict):
            data["symbol"] = SymbolSpec.from_dict(data["symbol"])
        return cls(**data)


@dataclass(frozen=True)
class AxisSpec:
    """Un axe : titre, unité, échelle, limites.

    `auto_range` prime sur `range_min`/`range_max` : les bornes restent
    mémorisées quand on repasse en automatique, pour qu'un aller-retour
    dans le panneau de propriétés ne les perde pas.
    """

    label: str = ""
    unit: str = ""
    scale: AxisScale = AxisScale.LINEAR
    auto_range: bool = True
    range_min: float | None = None
    range_max: float | None = None
    grid: bool = False
    visible: bool = True
    inverted: bool = False

    def __post_init__(self) -> None:
        object.__setattr__(self, "scale", _coerce(self.scale, AxisScale))
        for name in ("range_min", "range_max"):
            value = getattr(self, name)
            object.__setattr__(self, name, None if value is None else float(value))

    @property
    def display_label(self) -> str:
        """Titre affiché : `label (unité)`, sans parenthèses vides."""
        if self.label and self.unit:
            return f"{self.label} ({self.unit})"
        return self.label or (f"({self.unit})" if self.unit else "")

    @property
    def has_explicit_range(self) -> bool:
        return (
            not self.auto_range and self.range_min is not None and self.range_max is not None
        )

    def to_dict(self) -> dict[str, Any]:
        return _as_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> AxisSpec:
        return cls(**_known(cls, data))


@dataclass(frozen=True)
class LegendSpec:
    visible: bool = True
    position: LegendPosition = LegendPosition.TOP_RIGHT

    def __post_init__(self) -> None:
        object.__setattr__(self, "position", _coerce(self.position, LegendPosition))

    def to_dict(self) -> dict[str, Any]:
        return _as_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> LegendSpec:
        return cls(**_known(cls, data))


@dataclass(frozen=True)
class PanelSpec:
    """Un panneau : une zone de tracé et ses quatre axes.

    `row`/`column` placent le panneau dans la grille du `PlotSpec` — c'est
    la description du multi-panneaux, sans rien emprunter au système de
    disposition d'un moteur donné.
    """

    title: str = ""
    series: tuple[SeriesSpec, ...] = ()
    bottom: AxisSpec = field(default_factory=AxisSpec)
    left: AxisSpec = field(default_factory=AxisSpec)
    right: AxisSpec = field(default_factory=AxisSpec)
    top: AxisSpec = field(default_factory=lambda: AxisSpec(visible=False))
    legend: LegendSpec = field(default_factory=LegendSpec)
    row: int = 0
    column: int = 0
    id: str = field(default_factory=new_id)

    def __post_init__(self) -> None:
        object.__setattr__(self, "series", tuple(self.series))
        object.__setattr__(self, "row", max(0, int(self.row)))
        object.__setattr__(self, "column", max(0, int(self.column)))

    # -- lecture ------------------------------------------------------------

    def axis(self, side: AxisSide | str) -> AxisSpec:
        return getattr(self, str(_coerce(side, AxisSide)))

    def find_series(self, series_id: str) -> SeriesSpec | None:
        for series in self.series:
            if series.id == series_id:
                return series
        return None

    @property
    def uses_right_axis(self) -> bool:
        return any(s.axis is AxisSide.RIGHT for s in self.series)

    def keys(self) -> tuple[str, ...]:
        seen: list[str] = []
        for series in self.series:
            for key in series.keys():
                if key not in seen:
                    seen.append(key)
        return tuple(seen)

    # -- sérialisation ------------------------------------------------------

    def to_dict(self) -> dict[str, Any]:
        return _as_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> PanelSpec:
        data = dict(_known(cls, data))
        data["series"] = tuple(SeriesSpec.from_dict(s) for s in data.get("series", ()))
        for side in ("bottom", "left", "right", "top"):
            if isinstance(data.get(side), dict):
                data[side] = AxisSpec.from_dict(data[side])
        if isinstance(data.get("legend"), dict):
            data["legend"] = LegendSpec.from_dict(data["legend"])
        return cls(**data)


#: Version du *format* du spec, indépendante de la version de schéma HDF5 :
#: un `PlotSpec` peut aussi voyager par presse-papiers ou par template
#: (phase 11) sans passer par un fichier projet.
SPEC_FORMAT_VERSION = 1


@dataclass(frozen=True)
class PlotSpec:
    """La description complète d'un graphe. Le seul objet que la phase 10 lira."""

    title: str = ""
    panels: tuple[PanelSpec, ...] = ()
    #: `""` = laisser le renderer suivre le thème de l'application.
    background: str = ""
    format_version: int = SPEC_FORMAT_VERSION

    def __post_init__(self) -> None:
        panels = tuple(self.panels) or (PanelSpec(),)
        object.__setattr__(self, "panels", panels)
        object.__setattr__(self, "format_version", int(self.format_version))

    # -- construction --------------------------------------------------------

    @classmethod
    def empty(cls, title: str = "") -> PlotSpec:
        """Un graphe vide mais valide : un panneau, aucune série."""
        return cls(title=title, panels=(PanelSpec(),))

    # -- lecture -------------------------------------------------------------

    @property
    def rows(self) -> int:
        return max((panel.row for panel in self.panels), default=0) + 1

    @property
    def columns(self) -> int:
        return max((panel.column for panel in self.panels), default=0) + 1

    def iter_series(self) -> Iterator[tuple[int, SeriesSpec]]:
        """`(index de panneau, série)` pour toutes les séries du graphe."""
        for index, panel in enumerate(self.panels):
            for series in panel.series:
                yield index, series

    def find_series(self, series_id: str) -> tuple[int, SeriesSpec] | None:
        for index, series in self.iter_series():
            if series.id == series_id:
                return index, series
        return None

    def panel_index(self, panel_id: str) -> int | None:
        for index, panel in enumerate(self.panels):
            if panel.id == panel_id:
                return index
        return None

    def keys(self) -> tuple[str, ...]:
        """Clés de registre référencées par l'ensemble des séries."""
        seen: list[str] = []
        for _, series in self.iter_series():
            for key in series.keys():
                if key not in seen:
                    seen.append(key)
        return tuple(seen)

    def series_count(self) -> int:
        return sum(len(panel.series) for panel in self.panels)

    def with_panels(self, panels: tuple[PanelSpec, ...]) -> PlotSpec:
        return replace(self, panels=tuple(panels))

    # -- sérialisation --------------------------------------------------------

    def to_dict(self) -> dict[str, Any]:
        return _as_dict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> PlotSpec:
        data = dict(_known(cls, data))
        data["panels"] = tuple(PanelSpec.from_dict(p) for p in data.get("panels", ()))
        return cls(**data)
