"""Renderer Matplotlib — le rendu **par défaut** d'un `PlotSpec`.

Décision du 2026-09-07 (`docs/decision-rendu-matplotlib-2026-09-07.md`) :
Matplotlib remplace PyQtGraph comme moteur par défaut, PyQtGraph devenant
le « visualiseur léger » de `plot_renderer.py`. Ce module est donc le
chantier réel de la réouverture de la phase 6 — le repli, lui, était déjà
construit.

La règle qui garde les deux moteurs alignés est inchangée et vaut ici :
**toute la traduction du vocabulaire neutre vers celui du moteur est
écrite dans ce fichier**, dans les trois tables `_LINE_STYLES`,
`_MARKERS` et `_LEGEND_LOCS`, et nulle part ailleurs. Une option de style
qui ne s'exprimerait pas sans nommer Matplotlib est un manque du `PlotSpec`,
pas une raison de contourner la couche.

Ce module ne mute jamais le modèle : il *lit* un `PlotSpec` résolu et
peint. Les interactions qui modifient le graphe passent par le panneau de
propriétés, donc par le bus.

## Pas de `pyplot`

La figure est construite à la main (`Figure` + `FigureCanvasQTAgg`), jamais
par `pyplot`. C'est le chemin d'intégration recommandé, et surtout le seul
qui n'installe pas le gestionnaire global de figures de `pyplot` — lequel
retiendrait chaque figure jusqu'à un `close()` explicite et ferait fuir une
figure par graphe ouvert.

## Interactions

La barre d'outils est la **`NavigationToolbar` standard de Matplotlib**
(`create_toolbar`) : Home / Back / Forward, Pan, Zoom rectangle, réglage
des marges, éditeur d'axes et de courbes, enregistrement de la figure, et
sa propre lecture des coordonnées. C'est le vocabulaire que connaît
quiconque a déjà utilisé Matplotlib, et il apporte au passage un export
d'image avant l'heure (phase 10).

Deux écarts assumés, tous deux dans `PlotNavigationToolbar` :

* **Home recadre sur les données** au lieu de restaurer la vue initiale.
  La vue initiale d'origine devient fausse dès que les données changent —
  or ici elles changent tout le temps, c'est le principe même du « modèle
  de série = référence ». Home reprend donc le rôle de l'ancien bouton
  « Ajuster ».
* **Un bouton « Curseurs » est ajouté** : le réticule n'a pas d'équivalent
  standard, et le perdre aurait été une régression.

S'y ajoutent deux gestes que la barre ne couvre pas, actifs **seulement
quand aucun de ses outils n'est armé** — sans quoi ils se battraient avec
le Pan et le rectangle de zoom :

* **molette** → zoom centré sur le curseur, axe par axe, correct en
  échelle logarithmique (le facteur s'applique dans l'espace transformé) ;
* **glissé bouton gauche** → déplacement, délégué à `Axes.start_pan` /
  `drag_pan` / `end_pan`, l'API publique que la barre utilise elle-même —
  elle traite les axes log et inversés sans cas particulier.

Un geste de zoom ou de déplacement, d'où qu'il vienne, **désarme le
recadrage automatique** de l'axe concerné, comme le fait une `ViewBox`
PyQtGraph ; `auto_range` (donc Home) le réarme.

## Le réticule est *blitté*

C'est le seul point où le mode de rendu de Matplotlib (tout redessiner à
chaque interaction) coûterait vraiment : un réticule redessiné en
`draw_idle` repeindrait les 700 courbes du pire cas mesuré à chaque
mouvement de souris. Les deux lignes sont donc `animated=True` — donc
exclues du dessin normal — et le fond est capturé une fois par dessin
(`copy_from_bbox`), puis restauré à chaque mouvement. Le coût d'un
déplacement de réticule devient indépendant du contenu du graphe.

Le repli `draw_idle` reste en place si le canevas ne sait pas blitter :
mieux vaut un réticule lent qu'un réticule absent.
"""

from __future__ import annotations

import math
from collections.abc import Callable
from typing import Any

import numpy as np
from matplotlib.axes import Axes
from matplotlib.backends.backend_qtagg import FigureCanvasQTAgg, NavigationToolbar2QT
from matplotlib.figure import Figure
from PySide6.QtGui import QAction
from PySide6.QtWidgets import QWidget

from plotyx.plot.resolve import ResolvedPlot, ResolvedSeries, bar_width, histogram
from plotyx.plot.spec import (
    AxisScale,
    AxisSide,
    AxisSpec,
    LegendPosition,
    LineStyle,
    PanelSpec,
    PlotSpec,
    PlotType,
    SymbolShape,
)

__all__ = [
    "MatplotlibRenderer",
    "PlotNavigationToolbar",
    "line_style",
    "marker_of",
    "new_canvas",
]

#: Vocabulaire neutre → styles de trait Matplotlib. Table unique, cf. docstring.
_LINE_STYLES: dict[LineStyle, str] = {
    LineStyle.SOLID: "-",
    LineStyle.DASH: "--",
    LineStyle.DOT: ":",
    LineStyle.DASH_DOT: "-.",
    LineStyle.NONE: "none",
}

#: Vocabulaire neutre → codes de marqueur Matplotlib.
_MARKERS: dict[SymbolShape, str | None] = {
    SymbolShape.NONE: None,
    SymbolShape.CIRCLE: "o",
    SymbolShape.SQUARE: "s",
    SymbolShape.TRIANGLE: "^",
    SymbolShape.DIAMOND: "D",
    SymbolShape.PLUS: "+",
    SymbolShape.CROSS: "x",
}

#: Position de légende → `loc` Matplotlib.
_LEGEND_LOCS: dict[LegendPosition, str] = {
    LegendPosition.TOP_LEFT: "upper left",
    LegendPosition.TOP_RIGHT: "upper right",
    LegendPosition.BOTTOM_LEFT: "lower left",
    LegendPosition.BOTTOM_RIGHT: "lower right",
}

#: Fond par défaut. Le thème de l'application est clair (décision
#: 2026-09-07) ; `PlotSpec.background` reste prioritaire quand un graphe
#: fixe sa propre couleur, exactement comme côté PyQtGraph.
DEFAULT_BACKGROUND = "white"

#: Facteur de zoom d'un cran de molette. 1,25 : assez pour que le geste se
#: voie, assez peu pour qu'un cran de trop se rattrape d'un cran en arrière.
_ZOOM_STEP = 1.25

#: Taille de marqueur : le spec la donne en pixels de diamètre (convention
#: PyQtGraph, `symbolSize`), Matplotlib l'attend en points de côté.
_POINTS_PER_PIXEL = 72.0 / 96.0


def line_style(style: LineStyle) -> str:
    return _LINE_STYLES[LineStyle(style)]


def marker_of(shape: SymbolShape) -> str | None:
    return _MARKERS[SymbolShape(shape)]


def new_canvas() -> FigureCanvasQTAgg:
    """Un canevas vide, prêt à recevoir un `MatplotlibRenderer`.

    `constrained` plutôt que `tight` : la mise en page est recalculée à
    chaque dessin, ce qui évite qu'un titre d'axe édité en cours de route
    déborde du canevas jusqu'au prochain rendu complet.
    """
    figure = Figure(layout="constrained")
    canvas = FigureCanvasQTAgg(figure)
    canvas.setObjectName("graphCanvas")
    return canvas


def _scaled_range(low: float, high: float, focus: float, factor: float, log: bool) -> tuple:
    """Bornes zoomées d'un facteur `factor` autour de `focus`.

    En échelle logarithmique, le calcul se fait dans l'espace des
    logarithmes : c'est là qu'un zoom est uniforme. Les axes inversés
    (`low > high`) traversent l'arithmétique sans cas particulier.
    """
    if log:
        if min(low, high, focus) <= 0:
            return low, high
        low, high, focus = math.log10(low), math.log10(high), math.log10(focus)
    new_low = focus - (focus - low) * factor
    new_high = focus + (high - focus) * factor
    if log:
        return 10.0**new_low, 10.0**new_high
    return new_low, new_high


#: Info-bulles de la barre standard, en français. Clés = libellés
#: Matplotlib. Un libellé absent (version différente, outil ajouté) est
#: simplement laissé tel quel : traduire ne doit jamais casser la barre.
_TOOLBAR_TOOLTIPS: dict[str, str] = {
    "Home": "Recadrer tous les panneaux sur les données",
    "Back": "Vue précédente",
    "Forward": "Vue suivante",
    "Pan": (
        "Bouton gauche : déplacer · bouton droit : zoomer\n"
        "x/y limite à un axe, Ctrl conserve le rapport"
    ),
    "Zoom": "Zoom sur un rectangle\nx/y limite à un axe",
    "Subplots": "Régler les marges et l'espacement des panneaux",
    "Customize": "Éditer les axes, les courbes et les images",
    "Save": "Enregistrer la figure en image",
}


class PlotNavigationToolbar(NavigationToolbar2QT):
    """La barre Matplotlib standard, avec deux ajustements.

    Le parti pris est de **ne pas réinventer** ce que l'utilisateur connaît
    déjà : Home / Back / Forward, Pan, Zoom rectangle, marges, éditeur
    d'axes, enregistrement, et la lecture des coordonnées de la barre
    elle-même. Seuls deux points sont touchés, chacun pour une raison
    précise.

    **`home` recadre sur les données.** Le Home d'origine restaure la vue
    telle qu'elle était au premier affichage. Ici, les séries sont des
    *références* : un import, une saisie ou un `undo` changent les données
    sous le graphe, et cette vue initiale devient une position arbitraire
    du passé. Le bouton reprend donc le rôle de l'ancien « Ajuster », et
    la vue obtenue est empilée pour que Back y ramène.

    **Un bouton « Curseurs »** est ajouté : le réticule n'a pas
    d'équivalent dans la barre standard, et le perdre en adoptant celle-ci
    aurait été une régression.
    """

    def __init__(
        self,
        canvas: FigureCanvasQTAgg,
        renderer: MatplotlibRenderer,
        parent: QWidget | None = None,
    ) -> None:
        super().__init__(canvas, parent)
        self.setObjectName("graphNavigationToolbar")
        self._renderer = renderer
        self._translate_tooltips()

        # Inséré **avant** la dernière action, qui porte l'étiquette de
        # coordonnées : celle-ci est collée à droite, et ajouter à la suite
        # rejetterait le bouton loin des autres outils. `insertAction`
        # accepte `None` comme repère et se comporte alors comme `addAction`,
        # ce qui couvre le cas d'une barre construite sans coordonnées.
        actions = self.actions()
        before = actions[-1] if self.coordinates and actions else None
        action = QAction("Curseurs", self)
        action.setObjectName("graphCrosshairAction")
        action.setCheckable(True)
        action.setToolTip("Afficher un réticule suivant le curseur")
        action.toggled.connect(renderer.set_crosshair)
        self.insertSeparator(before)
        self.insertAction(before, action)
        self._crosshair_action = action

    def _translate_tooltips(self) -> None:
        for action in self.actions():
            tooltip = _TOOLBAR_TOOLTIPS.get(action.text())
            if tooltip:
                action.setToolTip(tooltip)

    def home(self, *args: Any) -> None:  # noqa: D102 - API Matplotlib
        self._renderer.auto_range()
        self.push_current()

    def sync_crosshair(self, enabled: bool) -> None:
        """Aligne le bouton sur l'état réel, sans le déclencher en retour.

        Pas de garde contre la récurrence : `set_crosshair` sort tout de
        suite quand l'état ne change pas, ce qui suffit à l'arrêter.
        """
        if self._crosshair_action.isChecked() != enabled:
            self._crosshair_action.setChecked(enabled)


class MatplotlibRenderer:
    """Peint un `PlotSpec` résolu dans un `FigureCanvasQTAgg`.

    Même contrat que `PyQtGraphRenderer` (voir `plotyx.ui.renderers`), avec
    les deux mêmes chemins :

    * :meth:`render` reconstruit tout — réponse à un changement de *spec* ;
    * :meth:`update_data` ne réécrit que les tableaux des artistes existants
      — réponse à un changement de *donnée*, le cas fréquent. Elle retourne
      `False` si la structure ne correspond plus, à charge pour l'appelant
      de repasser par `render`.
    """

    #: Ce renderer honore tout ce que le panneau de propriétés expose.
    supports_full_styling = True

    def __init__(self, canvas: FigureCanvasQTAgg) -> None:
        self._canvas = canvas
        self._figure = canvas.figure
        self._axes: list[Axes] = []
        self._right_axes: dict[int, Axes] = {}
        self._curves: dict[str, Any] = {}
        self._spec: PlotSpec | None = None
        self._crosshair = False
        self._crosshair_items: list[tuple[Any, Any]] = []
        #: Recadrage automatique `(x, y)` par panneau : désarmé par un geste
        #: de zoom/déplacement ou par une limite explicite du spec.
        self._auto: list[list[bool]] = []
        self._coords: Callable[[float, float] | None, None] | None = None
        self._background: Any = None
        self._panning: Axes | None = None
        self._toolbar: PlotNavigationToolbar | None = None
        self._detached = False
        self._cids = [
            canvas.mpl_connect("draw_event", self._on_draw),
            canvas.mpl_connect("scroll_event", self._on_scroll),
            canvas.mpl_connect("button_press_event", self._on_press),
            canvas.mpl_connect("motion_notify_event", self._on_motion),
            canvas.mpl_connect("button_release_event", self._on_release),
        ]

    # -- accès ---------------------------------------------------------------

    @property
    def widget(self) -> FigureCanvasQTAgg:
        return self._canvas

    @property
    def figure(self) -> Figure:
        return self._figure

    @property
    def axes(self) -> tuple[Axes, ...]:
        """Les axes des panneaux, dans l'ordre du spec (introspection, tests)."""
        return tuple(self._axes)

    @property
    def curves(self) -> dict[str, Any]:
        """Artistes indexés par identifiant de série (introspection, tests).

        Même périmètre que côté PyQtGraph : les artistes « ligne » — donc
        pas les barres, qui n'ont pas de chemin rapide.
        """
        return dict(self._curves)

    @property
    def crosshair_enabled(self) -> bool:
        return self._crosshair

    def series_xy(self, series_id: str) -> tuple[np.ndarray, np.ndarray] | None:
        """Les tableaux réellement tracés pour une série, ou `None`.

        Point d'introspection **neutre** : c'est par lui que les tests de
        `GraphWindow` vérifient qu'une saisie s'est propagée jusqu'au tracé,
        sans nommer de moteur.
        """
        artist = self._curves.get(series_id)
        if artist is None:
            return None
        if hasattr(artist, "get_xdata"):
            return (
                np.asarray(artist.get_xdata(), dtype=float),
                np.asarray(artist.get_ydata(), dtype=float),
            )
        # `StepPatch` d'un histogramme : bornes et effectifs.
        data = artist.get_data()
        return np.asarray(data.edges, dtype=float), np.asarray(data.values, dtype=float)

    def detach(self) -> None:
        """Coupe tout : appelé à la fermeture ou au changement de moteur.

        **Idempotent, et la barre part en premier.** `Figure.clear()`
        appelle `toolbar.update()` sur la barre que la `NavigationToolbar`
        s'est inscrite dans `canvas.toolbar` à sa construction. Or à la
        fermeture d'une sous-fenêtre MDI, Qt a déjà détruit la barre —
        enfant du widget — quand `destroyed` nous ramène ici : nettoyer la
        figure ferait alors toucher des `QAction` mortes. On désinscrit
        donc la barre avant, ce qui rend le nettoyage sans effet de bord.
        """
        if self._detached:
            return
        self._detached = True
        self._coords = None
        self._toolbar = None
        try:
            for cid in self._cids:
                self._canvas.mpl_disconnect(cid)
            self._canvas.toolbar = None
            self._figure.clear()
        except RuntimeError:
            # Canevas déjà détruit par Qt (sous-fenêtre `WA_DeleteOnClose`,
            # dont `destroyed` nous amène ici) : rien à nettoyer.
            pass
        self._cids.clear()

    def set_coords_callback(self, callback: Callable[..., None] | None) -> None:
        self._coords = callback

    # -- barre d'outils --------------------------------------------------------

    def create_toolbar(self, parent: QWidget | None = None) -> PlotNavigationToolbar:
        """La barre Matplotlib standard, montée sur ce canevas.

        Un seul renderer la construit — le visualiseur léger n'a pas
        d'équivalent et rend `None` (voir `plotyx.ui.renderers`).
        """
        self._toolbar = PlotNavigationToolbar(self._canvas, self, parent)
        return self._toolbar

    def _tool_is_armed(self) -> bool:
        """Vrai si Pan ou Zoom est actif dans la barre.

        Les gestes maison (molette, glissé) se taisent alors : deux
        mécanismes de déplacement simultanés ne se partagent pas une souris,
        et le rectangle de zoom serait purement et simplement cassé.
        """
        return self._toolbar is not None and bool(str(self._toolbar.mode))

    # -- rendu complet --------------------------------------------------------

    def render(self, resolved: ResolvedPlot) -> None:
        """Reconstruit entièrement la figure à partir du spec résolu."""
        spec = resolved.spec
        self._figure.clear()
        self._axes.clear()
        self._right_axes.clear()
        self._curves.clear()
        self._crosshair_items.clear()
        self._auto.clear()
        self._background = None
        self._spec = spec

        background = spec.background or DEFAULT_BACKGROUND
        self._figure.set_facecolor(background)

        columns = spec.columns
        for index, panel_data in enumerate(resolved.panels):
            panel = panel_data.spec
            position = panel.row * columns + panel.column + 1
            ax = self._figure.add_subplot(spec.rows, columns, position)
            ax.set_facecolor(background)
            self._axes.append(ax)
            self._auto.append([True, True])
            self._configure_panel(index, ax, panel)
            for series in panel_data.series:
                self._add_series(index, ax, series)
            self._apply_ranges(index, ax, panel)
            self._build_legend(index, ax, panel)

        if spec.title:
            # `suptitle` plutôt que le titre du premier panneau (le détour
            # imposé par PyQtGraph, où un label en ligne 0 décalerait la
            # grille) : Matplotlib réserve la place au-dessus de la grille
            # sans la déformer, donc autant utiliser l'idiome du moteur.
            self._figure.suptitle(spec.title)
        if self._crosshair:
            self._build_crosshair()
        self._canvas.draw_idle()

        # La pile de vues de la barre (Back/Forward) est remise à zéro par
        # le `Figure.clear()` du début — Matplotlib y appelle lui-même
        # `toolbar.update()`. Sans ça, Back ramènerait à des limites
        # calculées sur des axes qui n'existent plus.

    # -- configuration d'un panneau -------------------------------------------

    def _configure_panel(self, index: int, ax: Axes, panel: PanelSpec) -> None:
        if panel.title:
            ax.set_title(panel.title)
        ax.set_xlabel(panel.bottom.display_label)
        ax.set_ylabel(panel.left.display_label)
        ax.set_xscale("log" if panel.bottom.scale is AxisScale.LOG else "linear")
        ax.set_yscale("log" if panel.left.scale is AxisScale.LOG else "linear")
        # Une grille par axe : `ax.grid(False, axis=...)` doit pouvoir
        # éteindre l'un sans toucher l'autre, d'où les deux appels.
        ax.grid(panel.bottom.grid, axis="x")
        ax.grid(panel.left.grid, axis="y")
        ax.xaxis.set_visible(panel.bottom.visible)
        ax.yaxis.set_visible(panel.left.visible)
        if panel.uses_right_axis:
            self._ensure_right_axis(index, ax, panel.right)

    def _ensure_right_axis(self, index: int, ax: Axes, axis: AxisSpec) -> Axes:
        """Axe Y secondaire : un `twinx`, qui partage l'abscisse par construction.

        Là où PyQtGraph demande une seconde `ViewBox` liée à la main et
        resynchronisée à chaque redimensionnement, Matplotlib en fait une
        primitive — c'est le genre d'écart que la couche `PlotSpec` est
        faite pour absorber.
        """
        existing = self._right_axes.get(index)
        if existing is not None:
            return existing
        twin = ax.twinx()
        twin.set_ylabel(axis.display_label)
        twin.set_yscale("log" if axis.scale is AxisScale.LOG else "linear")
        twin.yaxis.set_inverted(axis.inverted)
        twin.set_facecolor("none")
        self._right_axes[index] = twin
        return twin

    def _apply_ranges(self, index: int, ax: Axes, panel: PanelSpec) -> None:
        """Limites d'axes : explicites si demandées, automatiques sinon.

        `set_inverted` est appliqué **après** les bornes : il ne dépend que
        du sens voulu, et reste idempotent quel que soit l'ordre dans lequel
        l'utilisateur a édité l'axe.
        """
        if panel.bottom.has_explicit_range:
            ax.set_xlim(panel.bottom.range_min, panel.bottom.range_max)
            self._auto[index][0] = False
        if panel.left.has_explicit_range:
            ax.set_ylim(panel.left.range_min, panel.left.range_max)
            self._auto[index][1] = False
        ax.xaxis.set_inverted(panel.bottom.inverted)
        ax.yaxis.set_inverted(panel.left.inverted)

        right = self._right_axes.get(index)
        if right is not None:
            if panel.right.has_explicit_range:
                right.set_ylim(panel.right.range_min, panel.right.range_max)
            right.yaxis.set_inverted(panel.right.inverted)

    def _build_legend(self, index: int, ax: Axes, panel: PanelSpec) -> None:
        """Une seule légende par panneau, axe secondaire compris.

        Les artistes déposés sur le `twinx` ne sont pas enfants de `ax` :
        sans cette collecte, la légende ignorerait les séries de l'axe de
        droite — le pendant exact du cas traité côté PyQtGraph.
        """
        if not panel.legend.visible:
            return
        handles, labels = ax.get_legend_handles_labels()
        right = self._right_axes.get(index)
        if right is not None:
            right_handles, right_labels = right.get_legend_handles_labels()
            handles += right_handles
            labels += right_labels
        if not handles:
            return
        ax.legend(handles, labels, loc=_LEGEND_LOCS[panel.legend.position])

    # -- séries ----------------------------------------------------------------

    def _target(self, panel_index: int, ax: Axes, series: ResolvedSeries) -> Axes:
        """Où déposer les artistes : le panneau, ou son axe secondaire."""
        if series.spec.axis is AxisSide.RIGHT:
            twin = self._right_axes.get(panel_index)
            if twin is not None:
                return twin
        return ax

    def _add_series(self, panel_index: int, ax: Axes, series: ResolvedSeries) -> None:
        """Ajoute les artistes d'une série. Une série rompue n'ajoute rien.

        Elle **n'est pas retirée du spec** pour autant : le bandeau de
        `GraphWindow` et le panneau de propriétés la signalent, et l'`undo`
        de la suppression de colonne la fait réapparaître d'elle-même.
        """
        if not series.spec.visible or not series.ok:
            return
        spec = series.spec
        target = self._target(panel_index, ax, series)
        # `_nolegend_` est la convention Matplotlib pour « présent mais hors
        # légende » ; l'omission du label produirait un `_childN` affiché.
        label = series.label if spec.show_in_legend else "_nolegend_"

        if spec.plot_type is PlotType.HISTOGRAM:
            edges, counts = self._histogram(series)
            patch = target.stairs(
                counts,
                edges,
                fill=True,
                color=spec.line.color,
                label=label,
            )
            self._curves[spec.id] = patch
        elif spec.plot_type is PlotType.BAR:
            target.bar(
                series.x,
                series.y,
                width=bar_width(series.x, spec.bar_width),
                color=spec.line.color,
                edgecolor=spec.line.color,
                label=label,
            )
        elif spec.plot_type is PlotType.ERROR_BAR:
            container = target.errorbar(
                series.x,
                series.y,
                yerr=series.y_error,
                xerr=series.x_error,
                label=label,
                capsize=0.0,
                **self._line_kwargs(spec),
                **self._marker_kwargs(spec),
            )
            self._curves[spec.id] = container.lines[0]
        else:
            (line,) = target.plot(
                series.x,
                series.y,
                label=label,
                **self._line_kwargs(spec),
                **self._marker_kwargs(spec),
            )
            self._curves[spec.id] = line

    @staticmethod
    def _histogram(series: ResolvedSeries) -> tuple[np.ndarray, np.ndarray]:
        """Le pré-calcul s'il existe (fait hors thread UI), sinon à la volée."""
        if series.histogram is not None:
            return series.histogram
        return histogram(series.y, series.spec.histogram_bins)

    @staticmethod
    def _line_kwargs(spec: Any) -> dict[str, Any]:
        if not spec.plot_type.draws_line or spec.line.style is LineStyle.NONE:
            return {"linestyle": "none", "color": spec.line.color}
        return {
            "linestyle": line_style(spec.line.style),
            "linewidth": spec.line.width,
            "color": spec.line.color,
        }

    @staticmethod
    def _marker_kwargs(spec: Any) -> dict[str, Any]:
        if not spec.plot_type.draws_symbols:
            return {}
        marker = marker_of(spec.symbol.shape)
        if marker is None:
            return {}
        return {
            "marker": marker,
            "markersize": spec.symbol.size * _POINTS_PER_PIXEL,
            "markerfacecolor": spec.symbol.color,
            "markeredgecolor": spec.symbol.edge_color or spec.symbol.color,
        }

    # -- mise à jour des seules données ----------------------------------------

    def update_data(self, resolved: ResolvedPlot) -> bool:
        """Réécrit les tableaux des artistes existants. `False` si à refaire.

        Chemin rapide du « modèle de série = référence » : quand une valeur
        change dans la feuille, il n'y a ni artiste à recréer, ni axe à
        reconfigurer — seulement des tableaux à repasser.

        Trois cas rendent la main à `render` : le spec n'a jamais été rendu,
        la structure a bougé (série ajoutée/supprimée, référence réparée par
        un undo), ou le graphe contient des barres / barres d'erreur — dont
        les artistes secondaires ne se réécrivent pas à la volée et
        resteraient périmés à côté d'une courbe à jour.
        """
        if self._spec is None:
            return False
        drawn: set[str] = set()
        for panel in resolved.panels:
            for series in panel.series:
                if not (series.spec.visible and series.ok):
                    continue
                if series.spec.plot_type in (PlotType.BAR, PlotType.ERROR_BAR):
                    return False
                drawn.add(series.spec.id)
        if drawn != set(self._curves):
            return False

        for index, panel in enumerate(resolved.panels):
            for series in panel.series:
                artist = self._curves.get(series.spec.id)
                if artist is None:
                    continue
                if series.spec.plot_type is PlotType.HISTOGRAM:
                    edges, counts = self._histogram(series)
                    artist.set_data(values=counts, edges=edges)
                else:
                    artist.set_data(series.x, series.y)
            self._rescale(index)
        self._background = None
        self._canvas.draw_idle()
        return True

    def _rescale(self, index: int) -> None:
        """Recadre les axes encore en automatique. Un axe zoomé ne bouge pas.

        C'est le pendant de `enableAutoRange` d'une `ViewBox` : PyQtGraph
        suit les données tant que l'utilisateur n'a pas pris la main,
        Matplotlib ne suit rien du tout tant qu'on ne le lui demande pas.
        """
        if index >= len(self._axes):
            return
        auto_x, auto_y = self._auto[index]
        if not (auto_x or auto_y):
            return
        ax = self._axes[index]
        # `set_xlim` (limites explicites du spec, ou geste de l'utilisateur)
        # désarme l'autoscale interne de Matplotlib, et `autoscale_view` le
        # respecte : sans ces deux lignes, un axe redevenu automatique
        # resterait figé sur ses anciennes bornes.
        ax.set_autoscalex_on(auto_x)
        ax.set_autoscaley_on(auto_y)
        ax.relim()
        ax.autoscale_view(scalex=auto_x, scaley=auto_y)

    # -- recadrage --------------------------------------------------------------

    def auto_range(self) -> None:
        """Recadre tous les panneaux sur leurs données (zoom/déplacement annulés)."""
        for index in range(len(self._axes)):
            self._auto[index] = [True, True]
            self._rescale(index)
            twin = self._right_axes.get(index)
            if twin is not None:
                twin.set_autoscale_on(True)
                twin.relim()
                twin.autoscale_view()
        self._background = None
        self._canvas.draw_idle()

    # -- curseurs ---------------------------------------------------------------

    def set_crosshair(self, enabled: bool) -> None:
        """Affiche ou retire le réticule sur tous les panneaux."""
        enabled = bool(enabled)
        if enabled == self._crosshair:
            return
        self._crosshair = enabled
        if enabled:
            self._build_crosshair()
        else:
            for vline, hline in self._crosshair_items:
                vline.remove()
                hline.remove()
            self._crosshair_items.clear()
        if self._toolbar is not None:
            # Le réticule s'allume aussi par la fenêtre (bascule de moteur,
            # API) : le bouton doit suivre, sinon il ment.
            self._toolbar.sync_crosshair(enabled)
        self._canvas.draw_idle()

    def _build_crosshair(self) -> None:
        """Deux lignes `animated` par panneau — donc hors du dessin normal.

        C'est ce qui rend le fond capturable une fois pour toutes et le
        déplacement du réticule indépendant du contenu du graphe.
        """
        self._crosshair_items.clear()
        for ax in self._axes:
            vline = ax.axvline(np.nan, color="#888888", linewidth=1, animated=True)
            hline = ax.axhline(np.nan, color="#888888", linewidth=1, animated=True)
            self._crosshair_items.append((vline, hline))

    def move_crosshair(self, panel_index: int, x: float, y: float) -> None:
        """Positionne le réticule d'un panneau (appelé sur le mouvement souris)."""
        if not self._crosshair or panel_index >= len(self._crosshair_items):
            return
        vline, hline = self._crosshair_items[panel_index]
        vline.set_xdata([x, x])
        hline.set_ydata([y, y])
        self._blit()

    def _blit(self) -> None:
        """Restaure le fond capturé et redessine les seules lignes du réticule."""
        if self._background is None:
            # Première utilisation, ou fond invalidé par un rendu : un
            # dessin complet est nécessaire pour le recapturer, et c'est
            # `_on_draw` qui s'en charge.
            self._canvas.draw()
            if self._background is None:  # canevas sans blit : repli honnête
                self._canvas.draw_idle()
                return
        self._canvas.restore_region(self._background)
        for ax, (vline, hline) in zip(self._axes, self._crosshair_items, strict=False):
            ax.draw_artist(vline)
            ax.draw_artist(hline)
        self._canvas.blit(self._figure.bbox)

    def _on_draw(self, _event: Any) -> None:
        """Capture le fond à chaque dessin complet — le réticule en est exclu."""
        if not self._crosshair:
            self._background = None
            return
        try:
            self._background = self._canvas.copy_from_bbox(self._figure.bbox)
        except Exception:  # noqa: BLE001 - canevas sans copie de région
            self._background = None

    # -- interactions ------------------------------------------------------------

    def panel_at(self, ax: Axes | None) -> int | None:
        """Index du panneau correspondant à des axes, ou `None`.

        L'axe secondaire d'un panneau renvoie l'index de ce panneau : du
        point de vue de l'utilisateur, survoler la moitié droite d'un
        panneau n'en fait pas un autre panneau.
        """
        if ax is None:
            return None
        for index, candidate in enumerate(self._axes):
            if candidate is ax or self._right_axes.get(index) is ax:
                return index
        return None

    def _on_scroll(self, event: Any) -> None:
        index = self.panel_at(event.inaxes)
        if index is None or event.xdata is None or event.ydata is None:
            return
        if self._tool_is_armed():
            return
        factor = _ZOOM_STEP ** (-event.step)
        ax = event.inaxes
        ax.set_xlim(
            *_scaled_range(*ax.get_xlim(), event.xdata, factor, ax.get_xscale() == "log")
        )
        ax.set_ylim(
            *_scaled_range(*ax.get_ylim(), event.ydata, factor, ax.get_yscale() == "log")
        )
        self._auto[index] = [False, False]
        self._background = None
        self._canvas.draw_idle()

    def _on_press(self, event: Any) -> None:
        index = self.panel_at(event.inaxes)
        if index is None or event.button != 1:
            return
        if self._tool_is_armed():
            # La barre prend la main. On note quand même que l'utilisateur
            # cadre lui-même : le recadrage automatique doit se taire, que
            # le geste vienne d'ici ou d'un outil de la barre.
            self._auto[index] = [False, False]
            return
        self._panning = event.inaxes
        self._auto[index] = [False, False]
        event.inaxes.start_pan(event.x, event.y, event.button)

    def _on_motion(self, event: Any) -> None:
        if self._panning is not None:
            self._panning.drag_pan(1, event.key, event.x, event.y)
            self._background = None
            self._canvas.draw_idle()
            return
        index = self.panel_at(event.inaxes)
        if index is None or event.xdata is None or event.ydata is None:
            if self._coords is not None:
                self._coords(None)
            return
        self.move_crosshair(index, event.xdata, event.ydata)
        if self._coords is not None:
            self._coords((float(event.xdata), float(event.ydata)))

    def _on_release(self, event: Any) -> None:
        if self._panning is None:
            return
        self._panning.end_pan()
        self._panning = None
        self._background = None
        self._canvas.draw_idle()
