"""Commandes de mutation d'un graphe — la règle d'or appliquée au tracé.

**Aucune mutation du modèle hors commande du bus** : créer une série,
changer une couleur, passer un axe en logarithmique, ajouter un panneau
sont autant de commandes annulables. Le garde-fou
(`plotyx.commands.guard`) reste donc satisfait pendant toute la phase 6.

## Une seule commande, et des fabriques

Le contenu d'un graphe est un `PlotSpec` **immuable** (voir
`plotyx.plot.spec`). Toute modification revient donc à poser un nouvel
objet à la place de l'ancien, et l'annulation à reposer l'ancien :
:class:`SetPlotSpecCommand` suffit à tout, et c'est la seule commande de ce
module. Les fabriques ci-dessous ne font que calculer le nouveau spec avec
les fonctions pures de `plotyx.plot.edit`, et donner à l'entrée
d'historique un texte lisible (« Couleur de la série », « Échelle
logarithmique »…).

## Coût mémoire

Le delta d'annulation est l'ancien `PlotSpec`, c'est-à-dire quelques
chaînes, énumérations et flottants — **jamais des données**. Les tableaux
restent dans les `Column` du modèle, la série n'en tient que la clé de
registre. Un graphe de 10 séries d'un million de points coûte à
l'historique le même prix qu'un graphe vide : c'est la conséquence directe
du choix « une série est une référence, pas une copie » de la roadmap.
"""

from __future__ import annotations

from typing import Any

from plotyx.commands.base import ModelCommand
from plotyx.core.errors import StructureError
from plotyx.core.nodes import Graph
from plotyx.plot import edit
from plotyx.plot.spec import AxisSide, PanelSpec, PlotSpec, SeriesSpec

__all__ = [
    "SetPlotSpecCommand",
    "add_panel_command",
    "add_series_command",
    "current_spec",
    "move_series_command",
    "remove_panel_command",
    "remove_series_command",
    "set_graph_title_command",
    "update_axis_command",
    "update_legend_command",
    "update_panel_command",
    "update_series_command",
]


def current_spec(graph: Graph) -> PlotSpec:
    """Le spec du graphe, ou un spec vide s'il n'en a pas encore.

    Un `Graph` fraîchement créé (ou relu d'un fichier de schéma 1) porte
    `None` : les fabriques n'ont pas à traiter ce cas une par une.
    """
    if not isinstance(graph, Graph):
        raise StructureError(f"{graph!r} n'est pas un graphe")
    spec = graph.plot_spec
    return spec if isinstance(spec, PlotSpec) else PlotSpec.empty(graph.name)


class SetPlotSpecCommand(ModelCommand):
    """Remplace le `PlotSpec` d'un graphe. Annulation : l'ancien spec.

    Le spec précédent est capturé dans `_apply`, pas à la construction :
    une commande rejouée après un `undo` doit reprendre l'état effectif du
    graphe à cet instant, pas celui qu'il avait quand l'utilisateur a
    cliqué.
    """

    def __init__(self, graph: Graph, spec: PlotSpec, text: str = "") -> None:
        super().__init__(text or f"Modification du graphe {graph.name}")
        if not isinstance(graph, Graph):
            raise StructureError(f"{graph!r} n'est pas un graphe")
        if not isinstance(spec, PlotSpec):
            raise TypeError(f"un graphe se décrit par un PlotSpec, pas {spec!r}")
        self._graph = graph
        self._spec = spec
        self._previous: PlotSpec | None = None

    @property
    def graph(self) -> Graph:
        return self._graph

    @property
    def spec(self) -> PlotSpec:
        return self._spec

    def _apply(self) -> None:
        self._previous = self._graph.set_plot_spec(self._spec)

    def _revert(self) -> None:
        self._graph.set_plot_spec(self._previous)


# -- fabriques ----------------------------------------------------------------
#
# Chacune calcule le nouveau spec au moment de l'appel, à partir de l'état
# courant du graphe, et rend une commande prête à pousser sur le bus.


def add_series_command(
    graph: Graph, series: SeriesSpec, panel_index: int = 0, text: str = ""
) -> SetPlotSpecCommand:
    spec = current_spec(graph)
    return SetPlotSpecCommand(
        graph,
        edit.add_series(spec, series, panel_index),
        text or f"Ajout d'une série à {graph.name}",
    )


def remove_series_command(graph: Graph, series_id: str, text: str = "") -> SetPlotSpecCommand:
    spec = current_spec(graph)
    return SetPlotSpecCommand(
        graph, edit.remove_series(spec, series_id), text or "Suppression d'une série"
    )


def update_series_command(
    graph: Graph, series_id: str, text: str = "", **changes: Any
) -> SetPlotSpecCommand:
    spec = current_spec(graph)
    return SetPlotSpecCommand(
        graph, edit.update_series(spec, series_id, **changes), text or "Style de série"
    )


def move_series_command(
    graph: Graph, series_id: str, to_panel: int, text: str = ""
) -> SetPlotSpecCommand:
    spec = current_spec(graph)
    return SetPlotSpecCommand(
        graph, edit.move_series(spec, series_id, to_panel), text or "Déplacement d'une série"
    )


def update_axis_command(
    graph: Graph, panel_index: int, side: AxisSide | str, text: str = "", **changes: Any
) -> SetPlotSpecCommand:
    spec = current_spec(graph)
    return SetPlotSpecCommand(
        graph, edit.update_axis(spec, panel_index, side, **changes), text or "Propriétés d'axe"
    )


def update_legend_command(
    graph: Graph, panel_index: int, text: str = "", **changes: Any
) -> SetPlotSpecCommand:
    spec = current_spec(graph)
    return SetPlotSpecCommand(
        graph, edit.update_legend(spec, panel_index, **changes), text or "Légende"
    )


def update_panel_command(
    graph: Graph, panel_index: int, text: str = "", **changes: Any
) -> SetPlotSpecCommand:
    spec = current_spec(graph)
    return SetPlotSpecCommand(
        graph, edit.update_panel(spec, panel_index, **changes), text or "Propriétés du panneau"
    )


def add_panel_command(
    graph: Graph, panel: PanelSpec | None = None, text: str = ""
) -> SetPlotSpecCommand:
    spec = current_spec(graph)
    return SetPlotSpecCommand(graph, edit.add_panel(spec, panel), text or "Ajout d'un panneau")


def remove_panel_command(graph: Graph, panel_index: int, text: str = "") -> SetPlotSpecCommand:
    spec = current_spec(graph)
    return SetPlotSpecCommand(
        graph, edit.remove_panel(spec, panel_index), text or "Suppression d'un panneau"
    )


def set_graph_title_command(graph: Graph, title: str, text: str = "") -> SetPlotSpecCommand:
    spec = current_spec(graph)
    return SetPlotSpecCommand(graph, edit.update_title(spec, title), text or "Titre du graphe")
