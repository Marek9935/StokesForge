"""Les deux moteurs de rendu, et le contrat qu'ils partagent.

Depuis la décision du 2026-09-07, un graphe se peint de deux façons :

| Mode | Moteur | Rôle |
|---|---|---|
| `RenderMode.FIGURE` | Matplotlib | **le défaut** — tout ce que le panneau de propriétés expose |
| `RenderMode.DIRECT` | PyQtGraph | le **visualiseur léger**, acquisition live, minimal |

Ce module existe pour que `GraphWindow` n'ait à connaître **ni** l'un
**ni** l'autre : elle demande un renderer pour un mode et parle au
`PlotRenderer` ci-dessous. Sans ce point de passage, chaque ajout au
protocole se paierait en `isinstance` disséminés dans l'UI.

## Le mode n'est jamais persisté

Tranché avec l'implémentation, comme le rapport de décision le prévoyait :
le mode est un **état d'exécution de la fenêtre**, jamais un attribut du
graphe. Il n'entre donc pas dans le `.plx` et le schéma HDF5 reste en
version 2. La raison est un principe, pas une économie : un fichier ne doit
pas imposer un moteur de rendu à qui l'ouvre — un projet enregistré sur une
machine d'acquisition s'ouvrirait sinon en vue dégradée sur le poste de
dépouillement, sans que personne ne l'ait demandé.

On y entre donc explicitement, à chaque fois : « Ouvrir en visualiseur
léger » dans le menu contextuel de l'explorateur de projet, ou la bascule du
menu Options — délibérément peu visible, pour un usage estimé à 0,5 % des
cas.

## Le protocole, et ce qu'il n'inclut pas

`PlotRenderer` ne décrit que ce dont `GraphWindow` a besoin. Tout ce qui
est propre à un moteur — `plot_items` et `data_coords` côté PyQtGraph,
`figure` et `axes` côté Matplotlib — reste sur la classe concrète, et
n'est utilisé que par les tests de ce moteur. C'est la frontière qui
empêche le vocabulaire d'un moteur de remonter dans la fenêtre.
"""

from __future__ import annotations

from collections.abc import Callable
from enum import StrEnum
from typing import Any, Protocol, runtime_checkable

from PySide6.QtWidgets import QWidget

from plotyx.plot.resolve import ResolvedPlot
from plotyx.ui.mpl_renderer import MatplotlibRenderer, new_canvas
from plotyx.ui.plot_renderer import PyQtGraphRenderer, new_plot_widget

__all__ = ["PlotRenderer", "RenderMode", "create_renderer"]


class RenderMode(StrEnum):
    """Le moteur avec lequel une fenêtre de graphe peint."""

    #: Matplotlib — rendu par défaut, complet.
    FIGURE = "figure"
    #: PyQtGraph — visualiseur léger, acquisition live.
    DIRECT = "direct"

    @property
    def label(self) -> str:
        """Nom affiché à l'utilisateur."""
        return "Visualiseur léger" if self is RenderMode.DIRECT else "Figure (Matplotlib)"


#: Le mode d'un graphe qu'on ouvre sans rien préciser.
DEFAULT_RENDER_MODE = RenderMode.FIGURE


@runtime_checkable
class PlotRenderer(Protocol):
    """Ce que `GraphWindow` attend d'un moteur de rendu.

    Les deux implémentations sont `MatplotlibRenderer` et
    `PyQtGraphRenderer`. Aucune n'hérite de ce protocole : il documente et
    fait vérifier une convergence, il n'impose pas de hiérarchie.
    """

    #: Faux pour un moteur qui n'honore pas tout ce que le panneau de
    #: propriétés expose — `GraphWindow` le signale alors à l'utilisateur.
    supports_full_styling: bool

    @property
    def widget(self) -> QWidget:
        """Le widget à insérer dans la fenêtre."""

    @property
    def curves(self) -> dict[str, Any]:
        """Artistes « ligne » indexés par identifiant de série."""

    @property
    def crosshair_enabled(self) -> bool: ...

    def render(self, resolved: ResolvedPlot) -> None:
        """Reconstruit tout le graphe. Réponse à un changement de *spec*."""

    def update_data(self, resolved: ResolvedPlot) -> bool:
        """Réécrit les seuls tableaux. `False` s'il faut repasser par `render`."""

    def series_xy(self, series_id: str) -> tuple[Any, Any] | None:
        """Les tableaux réellement tracés pour une série, ou `None`."""

    def set_crosshair(self, enabled: bool) -> None: ...

    def auto_range(self) -> None:
        """Recadre tous les panneaux sur leurs données."""

    def set_coords_callback(self, callback: Callable[..., None] | None) -> None:
        """Branche le rapport des coordonnées sous le curseur.

        Le renderer appelle `callback((x, y))` au survol d'un panneau et
        `callback(None)` en dehors. C'est le renderer qui suit la souris,
        pas la fenêtre : le mécanisme de suivi n'a rien de commun entre une
        scène `QGraphicsView` et un canevas Matplotlib.
        """

    def detach(self) -> None:
        """Coupe les connexions et vide le rendu. Après quoi l'objet est mort."""


def create_renderer(mode: RenderMode) -> PlotRenderer:
    """Le renderer d'un mode, avec le widget qui va avec (`renderer.widget`)."""
    if RenderMode(mode) is RenderMode.DIRECT:
        return PyQtGraphRenderer(new_plot_widget())
    return MatplotlibRenderer(new_canvas())
