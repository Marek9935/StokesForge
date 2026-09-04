"""Transformations pures d'un `PlotSpec` : entrée un spec, sortie un spec.

Le spec est immuable (voir `plotyx.plot.spec`). Modifier un graphe consiste
donc à en **calculer un nouveau**, jamais à muter l'ancien. Ces fonctions
sont l'endroit unique où cette arithmétique est écrite, ce qui donne trois
propriétés utiles :

* **testables sans Qt** — ce sont des fonctions de `PlotSpec` vers
  `PlotSpec`, sans effet de bord ;
* **undo gratuit** — `plotyx.commands.plot` s'en sert pour construire le
  nouveau spec, et garde l'ancien comme delta d'annulation. L'ancien spec
  ne pèse que ses chaînes : aucune donnée n'y est copiée, les tableaux
  restent dans les `Column` du modèle ;
* **réutilisables par la console (phase 9) et les templates (phase 11)**,
  qui manipuleront des specs sans passer par l'UI.

Aucune de ces fonctions ne touche au modèle : elles ignorent jusqu'à
l'existence de `Project`. C'est `plotyx.commands.plot` qui fait le lien.
"""

from __future__ import annotations

from dataclasses import replace
from typing import Any

from plotyx.plot.spec import (
    AxisSide,
    AxisSpec,
    LegendSpec,
    PanelSpec,
    PlotSpec,
    SeriesSpec,
    palette_color,
)

__all__ = [
    "add_panel",
    "add_series",
    "move_series",
    "next_series_color",
    "remove_panel",
    "remove_series",
    "series_for_column",
    "update_axis",
    "update_legend",
    "update_panel",
    "update_series",
    "update_title",
]


def _replace_panel(spec: PlotSpec, index: int, panel: PanelSpec) -> PlotSpec:
    if not 0 <= index < len(spec.panels):
        raise IndexError(f"panneau {index} inexistant (le graphe en a {len(spec.panels)})")
    panels = list(spec.panels)
    panels[index] = panel
    return spec.with_panels(tuple(panels))


def next_series_color(spec: PlotSpec) -> str:
    """Couleur suivante de la palette, en comptant les séries déjà présentes."""
    return palette_color(spec.series_count())


def series_for_column(
    spec: PlotSpec,
    y_key: str,
    x_key: str | None = None,
    **overrides: Any,
) -> SeriesSpec:
    """Construit une série au style par défaut, coloriée dans la palette.

    Confort d'appel : c'est la construction que fait l'UI quand on trace
    une colonne, et qu'un script fera de même en phase 9.
    """
    color = overrides.pop("color", None) or next_series_color(spec)
    line = overrides.pop("line", None)
    symbol = overrides.pop("symbol", None)
    series = SeriesSpec(y_key=y_key, x_key=x_key, **overrides)
    if line is None:
        line = replace(series.line, color=color)
    if symbol is None:
        symbol = replace(series.symbol, color=color)
    return replace(series, line=line, symbol=symbol)


def add_series(spec: PlotSpec, series: SeriesSpec, panel_index: int = 0) -> PlotSpec:
    """Ajoute une série à la fin du panneau `panel_index`."""
    panel = spec.panels[panel_index]
    return _replace_panel(spec, panel_index, replace(panel, series=(*panel.series, series)))


def remove_series(spec: PlotSpec, series_id: str) -> PlotSpec:
    """Retire une série. Lève `KeyError` si elle n'existe pas."""
    found = spec.find_series(series_id)
    if found is None:
        raise KeyError(f"série inconnue dans ce graphe : {series_id!r}")
    index, _ = found
    panel = spec.panels[index]
    kept = tuple(s for s in panel.series if s.id != series_id)
    return _replace_panel(spec, index, replace(panel, series=kept))


def update_series(spec: PlotSpec, series_id: str, **changes: Any) -> PlotSpec:
    """Modifie les champs d'une série (`plot_type`, `line`, `visible`, ...).

    Les sous-objets `line` et `symbol` acceptent aussi un dictionnaire de
    champs partiels (`line={"width": 3}`), ce qui évite à l'appelant de
    reconstruire un `LineSpec` complet pour changer une épaisseur.
    """
    found = spec.find_series(series_id)
    if found is None:
        raise KeyError(f"série inconnue dans ce graphe : {series_id!r}")
    panel_index, series = found
    for name in ("line", "symbol"):
        value = changes.get(name)
        if isinstance(value, dict):
            changes[name] = replace(getattr(series, name), **value)
    updated = replace(series, **changes)
    panel = spec.panels[panel_index]
    new_series = tuple(updated if s.id == series_id else s for s in panel.series)
    return _replace_panel(spec, panel_index, replace(panel, series=new_series))


def move_series(spec: PlotSpec, series_id: str, to_panel: int) -> PlotSpec:
    """Déplace une série vers un autre panneau, style compris."""
    found = spec.find_series(series_id)
    if found is None:
        raise KeyError(f"série inconnue dans ce graphe : {series_id!r}")
    from_panel, series = found
    if from_panel == to_panel:
        return spec
    spec = remove_series(spec, series_id)
    return add_series(spec, series, to_panel)


def update_axis(
    spec: PlotSpec, panel_index: int, side: AxisSide | str, **changes: Any
) -> PlotSpec:
    """Modifie un axe d'un panneau (`scale`, `label`, `range_min`, ...)."""
    panel = spec.panels[panel_index]
    side_name = str(AxisSide(side))
    axis: AxisSpec = getattr(panel, side_name)
    updated = replace(axis, **changes)
    return _replace_panel(spec, panel_index, replace(panel, **{side_name: updated}))


def update_legend(spec: PlotSpec, panel_index: int, **changes: Any) -> PlotSpec:
    panel = spec.panels[panel_index]
    legend: LegendSpec = replace(panel.legend, **changes)
    return _replace_panel(spec, panel_index, replace(panel, legend=legend))


def update_panel(spec: PlotSpec, panel_index: int, **changes: Any) -> PlotSpec:
    """Modifie un panneau lui-même (`title`, `row`, `column`)."""
    return _replace_panel(spec, panel_index, replace(spec.panels[panel_index], **changes))


def update_title(spec: PlotSpec, title: str) -> PlotSpec:
    return replace(spec, title=title)


def add_panel(spec: PlotSpec, panel: PanelSpec | None = None) -> PlotSpec:
    """Ajoute un panneau. Sans argument, l'empile sous les existants.

    Empiler verticalement est le défaut parce que c'est la disposition
    attendue quand on compare des signaux partageant l'abscisse ; la grille
    complète reste accessible en passant un `PanelSpec(row=..., column=...)`.
    """
    if panel is None:
        panel = PanelSpec(row=spec.rows, column=0)
    return spec.with_panels((*spec.panels, panel))


def remove_panel(spec: PlotSpec, panel_index: int) -> PlotSpec:
    """Retire un panneau et ses séries. Le dernier panneau n'est pas supprimable.

    Un graphe sans panneau ne serait pas rendable : `PlotSpec` en
    recréerait un vide silencieusement, ce qui masquerait l'erreur plutôt
    que de la signaler.
    """
    if len(spec.panels) <= 1:
        raise ValueError("un graphe garde au moins un panneau")
    panels = [p for index, p in enumerate(spec.panels) if index != panel_index]
    if len(panels) == len(spec.panels):
        raise IndexError(f"panneau {panel_index} inexistant")
    return spec.with_panels(tuple(panels))
