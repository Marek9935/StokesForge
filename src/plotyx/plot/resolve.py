"""Résolution des clés d'un `PlotSpec` vers les colonnes réelles du projet.

C'est le seul module du paquet `plotyx.plot` qui connaît `plotyx.core` — et
il ne connaît toujours ni Qt, ni PyQtGraph, ni Matplotlib. La frontière est
donc :

```
spec.py / edit.py   ── aucune dépendance (stdlib seule)
resolve.py          ── + plotyx.core, numpy
plotyx.ui.plot_*    ── + Qt / PyQtGraph          (le renderer)
phase 10            ── + Matplotlib              (le second renderer)
```

## Références par clé, jamais par copie

Une série porte la clé de registre (UUID de naissance, phase 1) de ses
colonnes. La résolution passe par `Project.registry`, donc :

* renommer une colonne, la déplacer, sauver et rouvrir le projet ne casse
  rien — c'est exactement pour cela que les clés sont persistées (phase 4) ;
* **rien n'est copié** : `ResolvedSeries.y` est la vue en lecture seule du
  `ndarray` de la colonne. Le graphe se met donc à jour dès que la donnée
  change, sans qu'aucune synchronisation ne soit à écrire.

## Références rompues

Le registre est en **références faibles** (décision de la phase 1) : une
colonne supprimée en sort et n'y revient qu'à l'`undo`. Une série qui la
désigne devient donc temporairement non résoluble. Le parti pris ici est
d'en faire un **état de première classe** plutôt qu'une exception :
`ResolvedSeries` existe toujours, avec `missing` non vide et `ok` faux.

Conséquences voulues :

* rien ne plante, ni au rendu, ni dans le panneau de propriétés ;
* la série **n'est pas supprimée du spec** : l'`undo` de la suppression de
  colonne la réinscrit sous la même clé, et le graphe se répare tout seul ;
* elle reste **visible** dans le panneau de propriétés, marquée « référence
  rompue », et le graphe affiche un bandeau — elle ne disparaît donc pas
  silencieusement.
"""

from __future__ import annotations

from dataclasses import dataclass, replace
from typing import Any

import numpy as np

from plotyx.core.column import Column, ColumnType
from plotyx.core.project import Project
from plotyx.plot.spec import PanelSpec, PlotSpec, PlotType, SeriesSpec

__all__ = [
    "ResolvedPanel",
    "ResolvedPlot",
    "ResolvedSeries",
    "ComputeCancelled",
    "bar_width",
    "histogram",
    "prepare",
    "resolve_column",
    "resolve_series",
    "resolve_spec",
]


def resolve_column(project: Project, key: str | None) -> Column | None:
    """Colonne inscrite sous `key`, ou `None` si la référence est rompue.

    `None` couvre trois cas volontairement indistincts pour l'appelant :
    clé absente du spec, objet sorti de l'arbre (supprimé, en attente d'un
    undo), ou objet qui n'est pas une colonne. Dans les trois, il n'y a
    rien à tracer.
    """
    if not key:
        return None
    obj = project.registry.get(key)
    return obj if isinstance(obj, Column) else None


def _numeric(column: Column | None) -> np.ndarray | None:
    """Vue `float64` d'une colonne, ou `None` si elle n'est pas numérique.

    Une colonne texte n'est pas traçable : ce n'est pas une erreur de
    l'utilisateur mais un choix de colonne inadapté, traité comme une
    référence inexploitable plutôt que par une exception.
    """
    if column is None or column.column_type is ColumnType.TEXT:
        return None
    return column.values


@dataclass(frozen=True)
class ResolvedSeries:
    """Une série et ses tableaux, ou la description de ce qui manque."""

    spec: SeriesSpec
    label: str
    x: np.ndarray | None = None
    y: np.ndarray | None = None
    x_error: np.ndarray | None = None
    y_error: np.ndarray | None = None
    #: Rôles dont la colonne est introuvable : `"x"`, `"y"`, `"xErr"`, `"yErr"`.
    missing: tuple[str, ...] = ()
    #: Vrai si x et y n'avaient pas la même longueur (colonnes de longueur
    #: libre, décision de la phase 1) : le tracé porte sur le préfixe commun.
    truncated: bool = False
    #: `(bornes, effectifs)` pré-calculés pour un histogramme, ou `None`.
    #: Rempli par :func:`prepare`, éventuellement hors du thread UI — c'est
    #: le seul calcul en O(n) de la chaîne de tracé (voir
    #: `plotyx.ui.plot_worker`). Le renderer le calcule lui-même s'il est
    #: absent, ce qui garde le chemin synchrone utilisable tel quel.
    histogram: tuple[np.ndarray, np.ndarray] | None = None

    @property
    def ok(self) -> bool:
        """Vrai si la série est traçable en l'état."""
        return not self.missing and self.y is not None and self.y.size > 0

    @property
    def broken(self) -> bool:
        """Vrai si au moins une colonne référencée est introuvable."""
        return bool(self.missing)

    @property
    def point_count(self) -> int:
        return 0 if self.y is None else int(self.y.size)


@dataclass(frozen=True)
class ResolvedPanel:
    spec: PanelSpec
    series: tuple[ResolvedSeries, ...] = ()

    def find(self, series_id: str) -> ResolvedSeries | None:
        for series in self.series:
            if series.spec.id == series_id:
                return series
        return None


@dataclass(frozen=True)
class ResolvedPlot:
    spec: PlotSpec
    panels: tuple[ResolvedPanel, ...] = ()

    @property
    def broken_series(self) -> tuple[ResolvedSeries, ...]:
        return tuple(s for panel in self.panels for s in panel.series if s.broken)

    @property
    def point_count(self) -> int:
        return sum(s.point_count for panel in self.panels for s in panel.series)


def _label_for(series: SeriesSpec, y_column: Column | None) -> str:
    """Nom affiché : celui du spec s'il est fixé, sinon celui de la colonne.

    Laisser le nom vide dans le spec est le cas normal : la légende suit
    alors les renommages de colonne sans qu'aucune commande n'ait à
    réécrire le graphe.
    """
    if series.name:
        return series.name
    if y_column is None:
        return "(référence rompue)"
    if y_column.long_name:
        return y_column.long_name
    return y_column.name


def resolve_series(project: Project, series: SeriesSpec) -> ResolvedSeries:
    """Résout une série. Ne lève jamais : une référence rompue est un état."""
    y_column = resolve_column(project, series.y_key)
    label = _label_for(series, y_column)
    missing: list[str] = []

    y = _numeric(y_column)
    if y is None:
        missing.append("y")

    x: np.ndarray | None = None
    if series.plot_type.needs_x and series.x_key:
        x_column = resolve_column(project, series.x_key)
        x = _numeric(x_column)
        if x is None:
            missing.append("x")

    x_error = y_error = None
    if series.x_error_key:
        x_error = _numeric(resolve_column(project, series.x_error_key))
        if x_error is None:
            missing.append("xErr")
    if series.y_error_key:
        y_error = _numeric(resolve_column(project, series.y_error_key))
        if y_error is None:
            missing.append("yErr")

    if missing:
        return ResolvedSeries(spec=series, label=label, missing=tuple(missing))

    truncated = False
    lengths = [len(a) for a in (x, y, x_error, y_error) if a is not None]
    n = min(lengths) if lengths else 0
    if any(length != n for length in lengths):
        # Colonnes de longueur libre : on trace le préfixe commun plutôt que
        # de refuser le tracé — c'est le cas courant d'une colonne d'erreur
        # plus courte que le signal.
        truncated = True
    if x is None and series.plot_type.needs_x and y is not None:
        # Abscisse implicite : l'indice de ligne, façon Origin quand aucune
        # colonne X n'est désignée. Construit à la volée, jamais stocké.
        x = np.arange(n, dtype=np.float64)

    def _cut(array: np.ndarray | None) -> np.ndarray | None:
        return None if array is None else array[:n]

    return ResolvedSeries(
        spec=series,
        label=label,
        x=_cut(x),
        y=_cut(y),
        x_error=_cut(x_error),
        y_error=_cut(y_error),
        truncated=truncated,
    )


def resolve_spec(project: Project, spec: PlotSpec) -> ResolvedPlot:
    """Résout toutes les séries d'un graphe. Ne lève jamais.

    Coût : indépendant du nombre de points — une résolution de clé et des
    tranches de vues numpy par série, jamais une copie.
    """
    panels = tuple(
        ResolvedPanel(
            spec=panel,
            series=tuple(resolve_series(project, series) for series in panel.series),
        )
        for panel in spec.panels
    )
    return ResolvedPlot(spec=spec, panels=panels)


def prepare(
    project: Project,
    spec: PlotSpec,
    progress: Any = None,
    should_cancel: Any = None,
) -> ResolvedPlot:
    """Résout un graphe **et** pré-calcule ce qui coûte en O(n).

    Aujourd'hui : les histogrammes, seul calcul de la chaîne de tracé qui
    parcourt les données (tout le reste est une vue numpy passée telle
    quelle à PyQtGraph, qui décime au moment de peindre). Les rappels
    `progress`/`should_cancel` ont la même signature que ceux de
    `plotyx.io` : c'est ce qui permet à `plotyx.ui.plot_worker` de
    réutiliser tel quel le motif `QThread` de la phase 5, sans que ce
    module ne connaisse Qt.
    """
    resolved = resolve_spec(project, spec)
    total = max(1, sum(len(panel.series) for panel in resolved.panels))
    done = 0
    panels: list[ResolvedPanel] = []
    for panel in resolved.panels:
        series_list: list[ResolvedSeries] = []
        for series in panel.series:
            if should_cancel is not None and should_cancel():
                raise ComputeCancelled("préparation du graphe annulée")
            if series.ok and series.spec.plot_type is PlotType.HISTOGRAM:
                series = replace(
                    series, histogram=histogram(series.y, series.spec.histogram_bins)
                )
            series_list.append(series)
            done += 1
            if progress is not None:
                progress(done / total)
        panels.append(replace(panel, series=tuple(series_list)))
    return replace(resolved, panels=tuple(panels))


class ComputeCancelled(RuntimeError):
    """L'utilisateur a annulé la préparation d'un graphe."""


def bar_width(x: np.ndarray, requested: float) -> float:
    """Largeur des barres : celle demandée, sinon 80 % du pas médian.

    Ici et pas dans un renderer : la déduction ne doit pas donner deux
    largeurs différentes selon le moteur qui peint. C'est la même raison
    qui a fait naître `PlotSpec` — tout ce qui n'est pas du vocabulaire de
    moteur vit du côté neutre.
    """
    if requested > 0:
        return float(requested)
    if x is None or x.size < 2:
        return 1.0
    steps = np.diff(np.sort(x))
    steps = steps[np.isfinite(steps) & (steps > 0)]
    return float(np.median(steps) * 0.8) if steps.size else 1.0


def histogram(values: np.ndarray, bins: int) -> tuple[np.ndarray, np.ndarray]:
    """Bornes et effectifs d'un histogramme, NaN exclus.

    Dans `plotyx.plot` — donc sans Qt — pour que le calcul soit testable
    seul et partagé par les deux renderers.
    """
    finite = values[np.isfinite(values)]
    if finite.size == 0:
        return np.zeros(1, dtype=np.float64), np.zeros(0, dtype=np.float64)
    counts, edges = np.histogram(finite, bins=max(1, int(bins)))
    return edges.astype(np.float64), counts.astype(np.float64)
