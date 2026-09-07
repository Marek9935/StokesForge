"""Renderer PyQtGraph — le moteur du **visualiseur léger**.

Premier renderer écrit (phase 6, 2026-09-05), et rendu par défaut jusqu'au
2026-09-07 ; il est depuis le mode *secondaire*, réservé à l'acquisition
live et au streaming, Matplotlib (`mpl_renderer.py`) ayant pris la place du
défaut. Le raisonnement est dans
`docs/decision-rendu-matplotlib-2026-09-07.md` ; ce qui l'a sauvé de la
suppression tient en un point : le viewport OpenGL (`useOpenGL=True`) est
un déchargement GPU réel, que Matplotlib n'a pas en usage standard.

Ce que « léger » veut dire concrètement : le code ci-dessous reste
complet — il était déjà écrit et testé, l'amputer coûterait sans rien
rapporter — mais **rien de neuf n'y sera ajouté**. Une option de style
nouvelle va dans le renderer Matplotlib ; si elle n'a pas d'équivalent
ici, la vue directe l'ignore, et c'est le comportement attendu.

La règle qui garde les deux moteurs alignés tient en une phrase :
**toute la traduction du vocabulaire neutre vers le vocabulaire d'un
moteur est écrite ici**, dans les trois tables `_PEN_STYLES`, `_SYMBOLS`
et `_LEGEND_ANCHORS`, et nulle part ailleurs. Si un jour une option de
style ne peut pas s'exprimer sans nommer PyQtGraph, c'est le spec qu'il
faut enrichir, pas ce module qu'il faut contourner.

Ce module ne mute jamais le modèle : il *lit* un `PlotSpec` et un
`ResolvedPlot` et peint. Les interactions qui modifient le graphe (styles,
limites d'axes) passent par le panneau de propriétés, donc par le bus.

## Performance

Deux réglages, appliqués systématiquement, qui font tenir 10⁶ points :

* `setDownsampling(auto=True, mode="peak")` — PyQtGraph ne dessine qu'un
  point tous les *n* pixels, en conservant le min et le max de chaque
  paquet : la forme du signal (et surtout ses pics) est préservée, contrairement
  à un sous-échantillonnage naïf ;
* `setClipToView(True)` — seul le segment visible est envoyé au moteur de
  peinture, ce qui rend le coût d'un zoom indépendant de la longueur totale
  de la série.

Aucun tableau n'est copié : `ResolvedSeries.y` est la vue en lecture seule
du `ndarray` de la colonne, passée telle quelle à `setData`.
"""

from __future__ import annotations

from collections.abc import Callable
from typing import Any

import numpy as np
import pyqtgraph as pg
from PySide6.QtCore import Qt

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

__all__ = ["PyQtGraphRenderer", "new_plot_widget", "pen_style", "symbol_of"]

#: Vocabulaire neutre → vocabulaire Qt. Table unique, cf. docstring.
_PEN_STYLES: dict[LineStyle, Qt.PenStyle] = {
    LineStyle.SOLID: Qt.PenStyle.SolidLine,
    LineStyle.DASH: Qt.PenStyle.DashLine,
    LineStyle.DOT: Qt.PenStyle.DotLine,
    LineStyle.DASH_DOT: Qt.PenStyle.DashDotLine,
    LineStyle.NONE: Qt.PenStyle.NoPen,
}

#: Vocabulaire neutre → codes de symbole PyQtGraph (`'o'`, `'s'`, ...).
_SYMBOLS: dict[SymbolShape, str | None] = {
    SymbolShape.NONE: None,
    SymbolShape.CIRCLE: "o",
    SymbolShape.SQUARE: "s",
    SymbolShape.TRIANGLE: "t",
    SymbolShape.DIAMOND: "d",
    SymbolShape.PLUS: "+",
    SymbolShape.CROSS: "x",
}

#: Position de légende → ancrage PyQtGraph `(item, parent, offset)`.
_LEGEND_ANCHORS: dict[LegendPosition, tuple[float, float]] = {
    LegendPosition.TOP_LEFT: (10, 10),
    LegendPosition.TOP_RIGHT: (-10, 10),
    LegendPosition.BOTTOM_LEFT: (10, -10),
    LegendPosition.BOTTOM_RIGHT: (-10, -10),
}


def pen_style(style: LineStyle) -> Qt.PenStyle:
    return _PEN_STYLES[LineStyle(style)]


def symbol_of(shape: SymbolShape) -> str | None:
    return _SYMBOLS[SymbolShape(shape)]


def new_plot_widget() -> pg.GraphicsLayoutWidget:
    """Un widget vide, prêt à recevoir un `PyQtGraphRenderer`.

    Pendant de `mpl_renderer.new_canvas` : `plotyx.ui.renderers` construit
    l'un ou l'autre sans avoir à connaître le constructeur de chaque moteur.
    """
    widget = pg.GraphicsLayoutWidget()
    widget.setObjectName("graphPlotWidget")
    return widget


class PyQtGraphRenderer:
    """Peint un `PlotSpec` résolu dans un `pg.GraphicsLayoutWidget`.

    Deux chemins, volontairement distincts :

    * :meth:`render` reconstruit tout — c'est la réponse à un changement de
      *spec* (une série ajoutée, un axe passé en log) ;
    * :meth:`update_data` ne fait que réécrire les tableaux des courbes
      existantes — c'est la réponse à un changement de *donnée*, le cas
      fréquent (saisie dans la feuille, import, undo d'une saisie). Elle
      retourne `False` si la structure ne correspond plus, à charge pour
      l'appelant de repasser par `render`.
    """

    #: La vue directe est volontairement minimale : le panneau de propriétés
    #: expose plus de réglages qu'elle n'en honore, et `GraphWindow` le dit
    #: à l'utilisateur au lieu de laisser croire à une édition sans effet.
    supports_full_styling = False

    def __init__(self, widget: pg.GraphicsLayoutWidget) -> None:
        self._widget = widget
        self._plots: list[pg.PlotItem] = []
        self._right_views: dict[int, pg.ViewBox] = {}
        self._curves: dict[str, pg.PlotDataItem] = {}
        self._spec: PlotSpec | None = None
        self._crosshair = False
        self._crosshair_items: list[tuple[pg.InfiniteLine, pg.InfiniteLine]] = []
        self._coords: Callable[..., None] | None = None
        self._detached = False
        widget.scene().sigMouseMoved.connect(self._on_mouse_moved)

    # -- accès ---------------------------------------------------------------

    @property
    def widget(self) -> pg.GraphicsLayoutWidget:
        return self._widget

    @property
    def plot_items(self) -> tuple[pg.PlotItem, ...]:
        return tuple(self._plots)

    @property
    def curves(self) -> dict[str, pg.PlotDataItem]:
        """Courbes indexées par identifiant de série (introspection, tests)."""
        return dict(self._curves)

    @property
    def crosshair_enabled(self) -> bool:
        return self._crosshair

    def series_xy(self, series_id: str) -> tuple[np.ndarray, np.ndarray] | None:
        """Les tableaux réellement tracés pour une série, ou `None`.

        Point d'introspection **neutre**, commun aux deux moteurs : c'est
        par lui que les tests de `GraphWindow` vérifient qu'une saisie s'est
        propagée jusqu'au tracé, sans nommer de moteur.
        """
        curve = self._curves.get(series_id)
        if curve is None:
            return None
        x, y = curve.getData()
        return np.asarray(x, dtype=float), np.asarray(y, dtype=float)

    def detach(self) -> None:
        """Coupe tout : appelé à la fermeture ou au changement de moteur.

        **Idempotent**, et pas par excès de prudence : fermer une
        sous-fenêtre MDI appelle `GraphWindow.disconnect_signals` par
        `closeEvent`, souvent après un appel explicite. Un second
        `disconnect` sur un signal déjà coupé ne lève pas côté PySide — il
        émet un `RuntimeWarning` que le plugin `unraisableexception` a
        vocation à faire remonter (voir l'état du projet, §4). Autant ne
        pas le produire.
        """
        if self._detached:
            return
        self._detached = True
        try:
            self._widget.scene().sigMouseMoved.disconnect(self._on_mouse_moved)
        except (RuntimeError, TypeError):
            # Scène déjà détruite : il n'y a plus rien à couper, et lever
            # ici empêcherait la fenêtre de se fermer.
            pass
        self._coords = None
        self._widget.clear()

    def set_coords_callback(self, callback: Callable[..., None] | None) -> None:
        self._coords = callback

    # -- rendu complet --------------------------------------------------------

    def render(self, resolved: ResolvedPlot) -> None:
        """Reconstruit entièrement le graphe à partir du spec résolu."""
        spec = resolved.spec
        self._widget.clear()
        self._plots.clear()
        self._right_views.clear()
        self._curves.clear()
        self._crosshair_items.clear()
        self._spec = spec

        if spec.background:
            self._widget.setBackground(spec.background)

        for index, panel_data in enumerate(resolved.panels):
            panel = panel_data.spec
            plot = self._widget.addPlot(row=panel.row, col=panel.column)
            self._plots.append(plot)
            self._configure_panel(index, plot, panel)
            legend = None
            if panel.legend.visible:
                legend = plot.addLegend(offset=_LEGEND_ANCHORS[panel.legend.position])
            for series in panel_data.series:
                self._add_series(index, plot, series, legend)
            self._apply_ranges(plot, panel)

        if spec.title:
            # Titre du graphe : porté par le premier panneau plutôt que par
            # un `LabelItem` séparé, pour que la zone de tracé reste alignée
            # entre panneaux (un label ajouté en ligne 0 décalerait la grille).
            if self._plots:
                first = resolved.panels[0].spec
                self._plots[0].setTitle(
                    f"{spec.title} — {first.title}" if first.title else spec.title
                )
        if self._crosshair:
            self._build_crosshair()

    # -- configuration d'un panneau -------------------------------------------

    def _configure_panel(self, index: int, plot: pg.PlotItem, panel: PanelSpec) -> None:
        if panel.title and not (self._spec and self._spec.title and index == 0):
            plot.setTitle(panel.title)
        plot.setLabel("bottom", panel.bottom.display_label)
        plot.setLabel("left", panel.left.display_label)
        plot.showGrid(x=panel.bottom.grid, y=panel.left.grid)
        plot.setLogMode(
            x=panel.bottom.scale is AxisScale.LOG, y=panel.left.scale is AxisScale.LOG
        )
        plot.getViewBox().invertX(panel.bottom.inverted)
        plot.getViewBox().invertY(panel.left.inverted)
        plot.showAxis("bottom", panel.bottom.visible)
        plot.showAxis("left", panel.left.visible)
        if panel.uses_right_axis:
            self._ensure_right_view(index, plot, panel.right)

    @staticmethod
    def _enable_downsampling(curve: pg.PlotDataItem) -> None:
        """Les deux réglages qui rendent 10⁶ points tenables.

        Posés **courbe par courbe, après insertion**, et non sur le
        `PlotItem`, pour deux raisons constatées à l'usage :

        1. `PlotItem.setDownsampling` rappelle `setDownsampling` sur *tous*
           ses items, y compris un `BarGraphItem`, qui n'a pas cette
           méthode — l'exception part alors dans la boucle d'événements Qt ;
        2. armer `clipToView` sur un item pas encore rattaché à sa `ViewBox`
           fait évaluer la plage visible trop tôt.

        Poser le réglage sur la courbe une fois ajoutée évite les deux.
        """
        curve.setDownsampling(auto=True, method="peak")
        curve.setClipToView(True)

    def _ensure_right_view(self, index: int, plot: pg.PlotItem, axis: AxisSpec) -> pg.ViewBox:
        """Crée l'axe secondaire : une seconde `ViewBox` liée en X.

        C'est la recette PyQtGraph de l'axe Y secondaire — la vue de droite
        a sa propre échelle verticale mais partage l'abscisse, et suit la
        géométrie de la vue principale à chaque redimensionnement.
        """
        existing = self._right_views.get(index)
        if existing is not None:
            return existing
        view = pg.ViewBox()
        plot.showAxis("right")
        plot.scene().addItem(view)
        plot.getAxis("right").linkToView(view)
        view.setXLink(plot)
        plot.getAxis("right").setLabel(axis.display_label)
        if axis.scale is AxisScale.LOG:
            view.setLogMode("y", True)
        view.invertY(axis.inverted)

        def _sync(_source: Any = None, plot: pg.PlotItem = plot, view: pg.ViewBox = view) -> None:
            view.setGeometry(plot.getViewBox().sceneBoundingRect())
            view.linkedViewChanged(plot.getViewBox(), view.XAxis)

        plot.getViewBox().sigResized.connect(_sync)
        _sync()
        self._right_views[index] = view
        return view

    def _apply_ranges(self, plot: pg.PlotItem, panel: PanelSpec) -> None:
        """Limites d'axes : explicites si demandées, automatiques sinon."""
        view = plot.getViewBox()
        if panel.bottom.has_explicit_range:
            plot.setXRange(panel.bottom.range_min, panel.bottom.range_max, padding=0)
        else:
            view.enableAutoRange(axis=view.XAxis)
        if panel.left.has_explicit_range:
            plot.setYRange(panel.left.range_min, panel.left.range_max, padding=0)
        else:
            view.enableAutoRange(axis=view.YAxis)
        right = self._right_views.get(self._plots.index(plot))
        if right is not None:
            if panel.right.has_explicit_range:
                right.setYRange(panel.right.range_min, panel.right.range_max, padding=0)
            else:
                right.enableAutoRange(axis=right.YAxis)

    # -- séries ----------------------------------------------------------------

    def _target(self, panel_index: int, plot: pg.PlotItem, series: ResolvedSeries) -> Any:
        """Où déposer les items : la vue principale, ou celle de l'axe droit."""
        if series.spec.axis is AxisSide.RIGHT:
            view = self._right_views.get(panel_index)
            if view is not None:
                return view
        return plot

    def _add_series(
        self,
        panel_index: int,
        plot: pg.PlotItem,
        series: ResolvedSeries,
        legend: Any,
    ) -> None:
        """Ajoute les items d'une série. Une série rompue n'ajoute rien.

        Elle **n'est pas retirée du spec** pour autant : le bandeau de
        `GraphWindow` et le panneau de propriétés la signalent, et l'`undo`
        de la suppression de colonne la fait réapparaître d'elle-même.
        """
        if not series.spec.visible or not series.ok:
            return
        spec = series.spec
        target = self._target(panel_index, plot, series)
        items: list[Any] = []

        if spec.plot_type is PlotType.HISTOGRAM:
            edges, counts = self._histogram(series)
            curve = pg.PlotDataItem(
                x=edges,
                y=counts,
                stepMode="center",
                fillLevel=0,
                brush=pg.mkBrush(spec.line.color),
                pen=self._pen(spec),
                name=series.label if spec.show_in_legend else None,
            )
            items.append(curve)
            self._curves[spec.id] = curve
        elif spec.plot_type is PlotType.BAR:
            bar = pg.BarGraphItem(
                x=series.x,
                height=series.y,
                width=bar_width(series.x, spec.bar_width),
                brush=pg.mkBrush(spec.line.color),
                pen=pg.mkPen(spec.line.color),
            )
            items.append(bar)
        else:
            curve = pg.PlotDataItem(
                x=series.x,
                y=series.y,
                pen=self._pen(spec) if spec.plot_type.draws_line else None,
                name=series.label if spec.show_in_legend else None,
                **self._symbol_kwargs(spec),
            )
            items.append(curve)
            self._curves[spec.id] = curve
            if spec.plot_type is PlotType.ERROR_BAR:
                items.append(self._error_bars(series))

        for item in items:
            target.addItem(item)
        curve = self._curves.get(spec.id)
        if curve is not None:
            self._enable_downsampling(curve)
        if legend is not None and spec.show_in_legend and curve is not None:
            if series.spec.axis is AxisSide.RIGHT:
                # Une courbe posée dans la vue de droite n'est pas enfant du
                # `PlotItem` : la légende ne la récolte pas toute seule.
                legend.addItem(self._curves[spec.id], series.label)

    @staticmethod
    def _histogram(series: ResolvedSeries) -> tuple[np.ndarray, np.ndarray]:
        """Le pré-calcul s'il existe (fait hors thread UI), sinon à la volée."""
        if series.histogram is not None:
            return series.histogram
        return histogram(series.y, series.spec.histogram_bins)

    def _error_bars(self, series: ResolvedSeries) -> pg.ErrorBarItem:
        spec = series.spec
        kwargs: dict[str, Any] = {"x": series.x, "y": series.y, "pen": self._pen(spec)}
        if series.y_error is not None:
            kwargs["top"] = series.y_error
            kwargs["bottom"] = series.y_error
        if series.x_error is not None:
            kwargs["left"] = series.x_error
            kwargs["right"] = series.x_error
        kwargs.setdefault("beam", 0.0)
        return pg.ErrorBarItem(**kwargs)

    def _pen(self, spec: Any) -> Any:
        if spec.line.style is LineStyle.NONE:
            return None
        return pg.mkPen(
            color=spec.line.color, width=spec.line.width, style=pen_style(spec.line.style)
        )

    def _symbol_kwargs(self, spec: Any) -> dict[str, Any]:
        if not spec.plot_type.draws_symbols:
            return {}
        symbol = symbol_of(spec.symbol.shape)
        if symbol is None:
            return {}
        edge = spec.symbol.edge_color or spec.symbol.color
        return {
            "symbol": symbol,
            "symbolSize": spec.symbol.size,
            "symbolBrush": pg.mkBrush(spec.symbol.color),
            "symbolPen": pg.mkPen(edge),
        }

    # -- mise à jour des seules données ----------------------------------------

    def update_data(self, resolved: ResolvedPlot) -> bool:
        """Réécrit les tableaux des courbes existantes. `False` si à refaire.

        Chemin rapide du « modèle de série = référence » : quand une valeur
        change dans la feuille, il n'y a ni item à recréer, ni axe à
        reconfigurer — seulement des tableaux à repasser.

        Trois cas rendent la main à `render` : le spec n'a jamais été rendu,
        la structure a bougé (série ajoutée/supprimée, référence réparée par
        un undo), ou le graphe contient des barres / barres d'erreur. Ce
        dernier cas est un correctif : l'`ErrorBarItem` d'une série est un
        item distinct de sa courbe, que ce chemin ne réécrivait pas — des
        moustaches périmées restaient affichées à côté d'une courbe à jour.
        Même règle côté Matplotlib, pour la même raison.
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
        for panel in resolved.panels:
            for series in panel.series:
                curve = self._curves.get(series.spec.id)
                if curve is None:
                    continue
                if series.spec.plot_type is PlotType.HISTOGRAM:
                    edges, counts = self._histogram(series)
                    curve.setData(x=edges, y=counts)
                else:
                    curve.setData(x=series.x, y=series.y)
        return True

    # -- curseurs ---------------------------------------------------------------

    def set_crosshair(self, enabled: bool) -> None:
        """Affiche ou retire les curseurs (réticule) sur tous les panneaux."""
        if enabled == self._crosshair:
            return
        self._crosshair = enabled
        if enabled:
            self._build_crosshair()
        else:
            for plot, (vline, hline) in zip(self._plots, self._crosshair_items, strict=False):
                plot.removeItem(vline)
                plot.removeItem(hline)
            self._crosshair_items.clear()

    def _build_crosshair(self) -> None:
        self._crosshair_items.clear()
        for plot in self._plots:
            vline = pg.InfiniteLine(angle=90, movable=False, pen=pg.mkPen("#888888", width=1))
            hline = pg.InfiniteLine(angle=0, movable=False, pen=pg.mkPen("#888888", width=1))
            plot.addItem(vline, ignoreBounds=True)
            plot.addItem(hline, ignoreBounds=True)
            self._crosshair_items.append((vline, hline))

    def move_crosshair(self, panel_index: int, x: float, y: float) -> None:
        """Positionne le réticule d'un panneau (appelé sur le mouvement souris)."""
        if not self._crosshair or panel_index >= len(self._crosshair_items):
            return
        vline, hline = self._crosshair_items[panel_index]
        vline.setPos(x)
        hline.setPos(y)

    def panel_at(self, scene_pos: Any) -> int | None:
        """Index du panneau sous un point de la scène, ou `None`."""
        for index, plot in enumerate(self._plots):
            if plot.sceneBoundingRect().contains(scene_pos):
                return index
        return None

    def data_coords(self, panel_index: int, scene_pos: Any) -> tuple[float, float]:
        """Coordonnées *données* d'un point de la scène, dans un panneau."""
        view = self._plots[panel_index].getViewBox()
        point = view.mapSceneToView(scene_pos)
        return float(point.x()), float(point.y())

    def _on_mouse_moved(self, position: Any) -> None:
        """Suit la souris : réticule et lecture des coordonnées.

        Câblé ici et non dans `GraphWindow` : le suivi passe par la *scène*
        PyQtGraph, une notion que la fenêtre ne doit pas connaître si elle
        veut pouvoir héberger l'autre moteur sans rien changer.
        """
        index = self.panel_at(position)
        if index is None:
            if self._coords is not None:
                self._coords(None)
            return
        x, y = self.data_coords(index, position)
        self.move_crosshair(index, x, y)
        if self._coords is not None:
            self._coords((x, y))

    # -- recadrage ------------------------------------------------------------------

    def auto_range(self) -> None:
        """Recadre tous les panneaux sur leurs données (zoom/déplacement annulés)."""
        for plot in self._plots:
            plot.getViewBox().autoRange()
