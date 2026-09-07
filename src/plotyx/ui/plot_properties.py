"""Panneau de propriétés d'un graphe — le contenu du dock « Properties ».

Le dock existe depuis la phase 0 et n'était qu'un `QLabel` de substitution.
Ce module le remplit ; `MainWindow` y branche le panneau quand une
sous-fenêtre de graphe devient active, et remet le texte de substitution
sinon. Aucun second dock n'est créé.

## La règle d'or, appliquée à chaque widget

**Aucun contrôle n'écrit dans le modèle.** Chacun construit une commande
avec les fabriques de `plotyx.commands.plot` et la pousse sur le bus :
changer une couleur ou passer un axe en logarithmique s'annule au Ctrl+Z
comme une saisie de cellule. Le garde-fou de `plotyx.commands.guard` reste
donc satisfait, y compris ici.

Conséquence de conception : le panneau est **entièrement reconstruit à
partir du spec** à chaque changement (`reload`). Il n'a aucun état propre,
donc rien à resynchroniser après un `undo` — ce qui est exactement la
raison pour laquelle le spec est immuable.

## Séries orphelines

Une série dont la colonne a été supprimée reste listée, préfixée d'un
« ⚠ », et reste modifiable et supprimable. Elle ne disparaît pas
silencieusement, et l'`undo` de la suppression la remet en état.
"""

from __future__ import annotations

from typing import Any

from PySide6.QtCore import Qt, QTimer
from PySide6.QtWidgets import (
    QCheckBox,
    QColorDialog,
    QComboBox,
    QDoubleSpinBox,
    QFormLayout,
    QGroupBox,
    QHBoxLayout,
    QLabel,
    QLineEdit,
    QListWidget,
    QListWidgetItem,
    QPushButton,
    QScrollArea,
    QSpinBox,
    QVBoxLayout,
    QWidget,
)

from plotyx.commands import CommandBus
from plotyx.commands.plot import (
    add_panel_command,
    remove_series_command,
    set_graph_title_command,
    update_axis_command,
    update_legend_command,
    update_panel_command,
    update_series_command,
)
from plotyx.core.nodes import Graph
from plotyx.plot.resolve import resolve_spec
from plotyx.plot.spec import (
    AxisScale,
    AxisSide,
    LineStyle,
    PlotSpec,
    PlotType,
    SymbolShape,
)

__all__ = ["PlotPropertiesPanel"]

#: Mutations du projet qui peuvent rendre une série orpheline, ou la
#: réparer : ce sont les seules qui obligent le panneau à se relire.
_RESOLUTION_KINDS = frozenset(
    {"column_inserted", "column_removed", "child_inserted", "child_removed", "node_renamed"}
)

_PLOT_TYPE_LABELS = {
    PlotType.LINE: "Ligne",
    PlotType.SCATTER: "Nuage de points",
    PlotType.LINE_SCATTER: "Points + ligne",
    PlotType.BAR: "Barres",
    PlotType.HISTOGRAM: "Histogramme",
    PlotType.ERROR_BAR: "Barres d'erreur",
}

_LINE_STYLE_LABELS = {
    LineStyle.SOLID: "Continu",
    LineStyle.DASH: "Tirets",
    LineStyle.DOT: "Pointillés",
    LineStyle.DASH_DOT: "Tiret-point",
    LineStyle.NONE: "Aucun",
}

_SYMBOL_LABELS = {
    SymbolShape.NONE: "Aucun",
    SymbolShape.CIRCLE: "Cercle",
    SymbolShape.SQUARE: "Carré",
    SymbolShape.TRIANGLE: "Triangle",
    SymbolShape.DIAMOND: "Losange",
    SymbolShape.PLUS: "Plus",
    SymbolShape.CROSS: "Croix",
}


def _combo(labels: dict[Any, str], current: Any) -> QComboBox:
    combo = QComboBox()
    for value, text in labels.items():
        combo.addItem(text, value)
    combo.setCurrentIndex(combo.findData(current))
    return combo


class PlotPropertiesPanel(QWidget):
    """Édite le `PlotSpec` d'un graphe. Chaque modification est une commande."""

    def __init__(
        self,
        graph: Graph,
        project: Any,
        bus: CommandBus,
        parent: QWidget | None = None,
    ) -> None:
        super().__init__(parent)
        self.setObjectName("plotPropertiesPanel")
        self._graph = graph
        self._project = project
        self._bus = bus
        self._panel_index = 0
        self._selected_series: str | None = None
        self._loading = False

        self._body = QWidget()
        self._body_layout = QVBoxLayout(self._body)
        self._body_layout.setContentsMargins(6, 6, 6, 6)

        scroll = QScrollArea()
        scroll.setObjectName("plotPropertiesScroll")
        scroll.setWidgetResizable(True)
        scroll.setWidget(self._body)

        outer = QVBoxLayout(self)
        outer.setContentsMargins(0, 0, 0, 0)
        outer.addWidget(scroll)

        # Deux abonnements, deux régimes :
        #  * le graphe lui-même — un changement de spec est une action
        #    utilisateur isolée, on se relit immédiatement ;
        #  * le projet — une colonne supprimée ou rendue par un `undo`
        #    change l'état « référence rompue » des séries. Un import en
        #    macro en produit des dizaines, d'où la coalescence par un
        #    `QTimer` à 0 ms, comme dans `GraphWindow`.
        self._structure_conn = graph.structure_changed.connect(self._on_structure_changed)
        self._project_conn = project.structure_changed.connect(self._on_project_changed)
        self._reload_timer = QTimer(self)
        self._reload_timer.setSingleShot(True)
        self._reload_timer.setInterval(0)
        self._reload_timer.timeout.connect(self.reload)
        self.reload()

    # -- cycle de vie ----------------------------------------------------------

    def disconnect_signals(self) -> None:
        self._reload_timer.stop()
        self._structure_conn.disconnect()
        self._project_conn.disconnect()

    def _on_structure_changed(self, event: Any) -> None:
        """Le graphe a changé (y compris par un `undo`) : on se relit.

        Différé par le même `_reload_timer` que `_on_project_changed`, et
        pour une raison plus grave qu'une simple coalescence ici : cette
        notification part typiquement **pendant** `editingFinished` d'un
        `QLineEdit` du panneau lui-même (`_push`/`_push_title` poussent la
        commande depuis ce signal). Un `reload()` synchrone détruirait ce
        widget alors que Qt traite encore son propre événement — c'est le
        crash constaté en usage réel à l'édition du titre du graphe et du
        titre d'un axe (les tests qui déclenchent `editingFinished` par
        `.emit()` direct ne le reproduisaient pas : seul un vrai cycle
        d'événements Qt, focus perdu par clic/Tab/Entrée, l'exhibe). Voir
        `GraphWindow.refresh`, jamais synchrone pour la même famille de
        raisons.
        """
        if event.kind in ("plot_spec_changed", "node_renamed"):
            self._reload_timer.start()

    def _on_project_changed(self, event: Any) -> None:
        """Une colonne a bougé : l'état « référence rompue » a pu changer."""
        if event.kind in _RESOLUTION_KINDS:
            self._reload_timer.start()

    # -- accès ------------------------------------------------------------------

    @property
    def graph(self) -> Graph:
        return self._graph

    @property
    def panel_index(self) -> int:
        return self._panel_index

    @property
    def selected_series(self) -> str | None:
        return self._selected_series

    def spec(self) -> PlotSpec:
        spec = self._graph.plot_spec
        return spec if isinstance(spec, PlotSpec) else PlotSpec.empty(self._graph.name)

    # -- (re)construction ---------------------------------------------------------

    def reload(self) -> None:
        """Reconstruit tous les contrôles à partir du spec courant."""
        self._loading = True
        try:
            while self._body_layout.count():
                item = self._body_layout.takeAt(0)
                widget = item.widget()
                if widget is not None:
                    widget.setParent(None)
            spec = self.spec()
            self._panel_index = min(self._panel_index, len(spec.panels) - 1)
            self._body_layout.addWidget(self._build_graph_box(spec))
            self._body_layout.addWidget(self._build_panel_box(spec))
            self._body_layout.addWidget(self._build_series_box(spec))
            self._body_layout.addWidget(self._build_axes_box(spec))
            self._body_layout.addStretch(1)
        finally:
            self._loading = False

    # -- section « graphe » --------------------------------------------------------

    def _build_graph_box(self, spec: PlotSpec) -> QGroupBox:
        box = QGroupBox("Graphe")
        form = QFormLayout(box)

        title = QLineEdit(spec.title)
        title.setObjectName("plotTitleEdit")
        title.editingFinished.connect(lambda: self._push_title(title.text()))
        form.addRow("Titre", title)

        panel_combo = QComboBox()
        panel_combo.setObjectName("plotPanelCombo")
        for index, panel in enumerate(spec.panels):
            panel_combo.addItem(panel.title or f"Panneau {index + 1}", index)
        panel_combo.setCurrentIndex(self._panel_index)
        panel_combo.currentIndexChanged.connect(self._on_panel_selected)
        form.addRow("Panneau courant", panel_combo)

        add_panel = QPushButton("Ajouter un panneau")
        add_panel.setObjectName("plotAddPanelButton")
        add_panel.clicked.connect(self._push_add_panel)
        form.addRow(add_panel)
        return box

    def _on_panel_selected(self, index: int) -> None:
        if self._loading:
            return
        self._panel_index = max(0, index)
        self._selected_series = None
        self.reload()

    # -- section « panneau » ---------------------------------------------------------

    def _build_panel_box(self, spec: PlotSpec) -> QGroupBox:
        panel = spec.panels[self._panel_index]
        box = QGroupBox(f"Panneau {self._panel_index + 1}")
        form = QFormLayout(box)

        title = QLineEdit(panel.title)
        title.setObjectName("panelTitleEdit")
        title.editingFinished.connect(
            lambda: self._push(update_panel_command, self._panel_index, title=title.text())
        )
        form.addRow("Titre", title)

        legend = QCheckBox("Afficher la légende")
        legend.setObjectName("panelLegendBox")
        legend.setChecked(panel.legend.visible)
        legend.toggled.connect(
            lambda checked: self._push(update_legend_command, self._panel_index, visible=checked)
        )
        form.addRow(legend)
        return box

    # -- section « séries » ------------------------------------------------------------

    def _build_series_box(self, spec: PlotSpec) -> QGroupBox:
        panel = spec.panels[self._panel_index]
        resolved = resolve_spec(self._project, spec).panels[self._panel_index]
        box = QGroupBox("Séries")
        layout = QVBoxLayout(box)

        listing = QListWidget()
        listing.setObjectName("plotSeriesList")
        for series in panel.series:
            data = resolved.find(series.id)
            label = series.name or (data.label if data is not None else series.y_key)
            if data is not None and data.broken:
                # Orpheline : signalée, jamais retirée en douce. Voir le
                # docstring du module.
                label = f"⚠ {label} (référence rompue)"
            elif not series.visible:
                label = f"({label})"
            item = QListWidgetItem(label)
            item.setData(Qt.ItemDataRole.UserRole, series.id)
            listing.addItem(item)
        if panel.series:
            current = 0
            if self._selected_series is not None:
                for row in range(listing.count()):
                    if listing.item(row).data(Qt.ItemDataRole.UserRole) == self._selected_series:
                        current = row
                        break
            listing.setCurrentRow(current)
            self._selected_series = listing.item(current).data(Qt.ItemDataRole.UserRole)
        listing.currentRowChanged.connect(self._on_series_selected)
        self._series_list = listing
        layout.addWidget(listing)

        if not panel.series:
            layout.addWidget(QLabel("Aucune série. Tracez des colonnes depuis une feuille."))
            return box

        series = panel.find_series(self._selected_series)
        if series is None:
            return box
        layout.addLayout(self._build_series_form(series))
        return box

    def _on_series_selected(self, row: int) -> None:
        if self._loading or row < 0:
            return
        item = self._series_list.item(row)
        if item is None:
            return
        self._selected_series = item.data(Qt.ItemDataRole.UserRole)
        self.reload()

    def _build_series_form(self, series: Any) -> QFormLayout:
        form = QFormLayout()
        series_id = series.id

        name = QLineEdit(series.name)
        name.setObjectName("seriesNameEdit")
        name.setPlaceholderText("(nom de la colonne)")
        name.editingFinished.connect(
            lambda: self._push(update_series_command, series_id, name=name.text())
        )
        form.addRow("Nom", name)

        kind = _combo(_PLOT_TYPE_LABELS, series.plot_type)
        kind.setObjectName("seriesTypeCombo")
        kind.currentIndexChanged.connect(
            lambda: self._push(update_series_command, series_id, plot_type=kind.currentData())
        )
        form.addRow("Type", kind)

        color = QPushButton(series.line.color)
        color.setObjectName("seriesColorButton")
        color.clicked.connect(lambda: self._pick_color(series_id, series.line.color))
        form.addRow("Couleur", color)

        style = _combo(_LINE_STYLE_LABELS, series.line.style)
        style.setObjectName("seriesLineStyleCombo")
        style.currentIndexChanged.connect(
            lambda: self._push(
                update_series_command, series_id, line={"style": style.currentData()}
            )
        )
        form.addRow("Trait", style)

        width = QDoubleSpinBox()
        width.setObjectName("seriesLineWidthSpin")
        width.setRange(0.1, 20.0)
        width.setSingleStep(0.5)
        width.setValue(series.line.width)
        width.editingFinished.connect(
            lambda: self._push(
                update_series_command, series_id, line={"width": width.value()}
            )
        )
        form.addRow("Épaisseur", width)

        symbol = _combo(_SYMBOL_LABELS, series.symbol.shape)
        symbol.setObjectName("seriesSymbolCombo")
        symbol.currentIndexChanged.connect(
            lambda: self._push(
                update_series_command, series_id, symbol={"shape": symbol.currentData()}
            )
        )
        form.addRow("Symbole", symbol)

        size = QDoubleSpinBox()
        size.setObjectName("seriesSymbolSizeSpin")
        size.setRange(1.0, 40.0)
        size.setValue(series.symbol.size)
        size.editingFinished.connect(
            lambda: self._push(
                update_series_command, series_id, symbol={"size": size.value()}
            )
        )
        form.addRow("Taille", size)

        bins = QSpinBox()
        bins.setObjectName("seriesBinsSpin")
        bins.setRange(2, 1000)
        bins.setValue(series.histogram_bins)
        bins.editingFinished.connect(
            lambda: self._push(update_series_command, series_id, histogram_bins=bins.value())
        )
        form.addRow("Classes (histogramme)", bins)

        axis = QComboBox()
        axis.setObjectName("seriesAxisCombo")
        axis.addItem("Axe gauche", AxisSide.LEFT)
        axis.addItem("Axe droit (secondaire)", AxisSide.RIGHT)
        axis.setCurrentIndex(axis.findData(series.axis))
        axis.currentIndexChanged.connect(
            lambda: self._push(update_series_command, series_id, axis=axis.currentData())
        )
        form.addRow("Rattachement", axis)

        visible = QCheckBox("Visible")
        visible.setObjectName("seriesVisibleBox")
        visible.setChecked(series.visible)
        visible.toggled.connect(
            lambda checked: self._push(update_series_command, series_id, visible=checked)
        )
        form.addRow(visible)

        remove = QPushButton("Supprimer la série")
        remove.setObjectName("seriesRemoveButton")
        remove.clicked.connect(lambda: self._push(remove_series_command, series_id))
        form.addRow(remove)
        return form

    def _pick_color(self, series_id: str, current: str) -> None:
        from PySide6.QtGui import QColor

        chosen = QColorDialog.getColor(QColor(current), self, "Couleur de la série")
        if chosen.isValid():
            self.set_series_color(series_id, chosen.name())

    def set_series_color(self, series_id: str, color: str) -> None:
        """Change la couleur d'une série (trait et symbole ensemble).

        Extraite du `QColorDialog` pour être appelable sans dialogue modal —
        par un test, et demain par la console de la phase 9.
        """
        self._push(
            update_series_command,
            series_id,
            line={"color": color},
            symbol={"color": color},
            text="Couleur de la série",
        )

    # -- section « axes » ----------------------------------------------------------------

    def _build_axes_box(self, spec: PlotSpec) -> QGroupBox:
        panel = spec.panels[self._panel_index]
        box = QGroupBox("Axes")
        layout = QVBoxLayout(box)
        sides = [(AxisSide.BOTTOM, "X"), (AxisSide.LEFT, "Y")]
        if panel.uses_right_axis:
            sides.append((AxisSide.RIGHT, "Y droite"))
        for side, title in sides:
            layout.addWidget(self._build_axis_group(panel.axis(side), side, title))
        return box

    def _build_axis_group(self, axis: Any, side: AxisSide, title: str) -> QGroupBox:
        group = QGroupBox(f"Axe {title}")
        form = QFormLayout(group)
        prefix = str(side)

        label = QLineEdit(axis.label)
        label.setObjectName(f"axis_{prefix}_labelEdit")
        label.editingFinished.connect(
            lambda: self._push(update_axis_command, self._panel_index, side, label=label.text())
        )
        form.addRow("Titre", label)

        unit = QLineEdit(axis.unit)
        unit.setObjectName(f"axis_{prefix}_unitEdit")
        unit.editingFinished.connect(
            lambda: self._push(update_axis_command, self._panel_index, side, unit=unit.text())
        )
        form.addRow("Unité", unit)

        log = QCheckBox("Échelle logarithmique")
        log.setObjectName(f"axis_{prefix}_logBox")
        log.setChecked(axis.scale is AxisScale.LOG)
        log.toggled.connect(
            lambda checked: self._push(
                update_axis_command,
                self._panel_index,
                side,
                scale=AxisScale.LOG if checked else AxisScale.LINEAR,
            )
        )
        form.addRow(log)

        grid = QCheckBox("Grille")
        grid.setObjectName(f"axis_{prefix}_gridBox")
        grid.setChecked(axis.grid)
        grid.toggled.connect(
            lambda checked: self._push(update_axis_command, self._panel_index, side, grid=checked)
        )
        form.addRow(grid)

        auto = QCheckBox("Limites automatiques")
        auto.setObjectName(f"axis_{prefix}_autoBox")
        auto.setChecked(axis.auto_range)
        auto.toggled.connect(
            lambda checked: self._push(
                update_axis_command, self._panel_index, side, auto_range=checked
            )
        )
        form.addRow(auto)

        low = QDoubleSpinBox()
        low.setObjectName(f"axis_{prefix}_minSpin")
        high = QDoubleSpinBox()
        high.setObjectName(f"axis_{prefix}_maxSpin")
        for spin, value in ((low, axis.range_min), (high, axis.range_max)):
            spin.setRange(-1e12, 1e12)
            spin.setDecimals(6)
            spin.setEnabled(not axis.auto_range)
            if value is not None:
                spin.setValue(value)
        low.editingFinished.connect(
            lambda: self._push(
                update_axis_command, self._panel_index, side, range_min=low.value()
            )
        )
        high.editingFinished.connect(
            lambda: self._push(
                update_axis_command, self._panel_index, side, range_max=high.value()
            )
        )
        limits = QHBoxLayout()
        limits.addWidget(low)
        limits.addWidget(high)
        container = QWidget()
        container.setLayout(limits)
        form.addRow("Min / Max", container)
        return group

    # -- poussée sur le bus ---------------------------------------------------------------

    def _push(self, factory: Any, *args: Any, **kwargs: Any) -> None:
        """Construit la commande et la pousse. Seul chemin d'écriture du panneau.

        Une commande qui ne changerait rien n'est **pas** poussée : les
        signaux Qt d'édition (`editingFinished`, `currentIndexChanged`)
        partent aussi quand un widget perd le focus ou est reconstruit, et
        empiler une entrée d'historique sans effet rendrait le Ctrl+Z
        illisible. La comparaison est exacte, le spec étant immuable.
        """
        if self._loading:
            return
        command = factory(self._graph, *args, **kwargs)
        if command.spec == self.spec():
            return
        self._bus.push(command)

    def _push_title(self, title: str) -> None:
        if self._loading or title == self.spec().title:
            return
        self._bus.push(set_graph_title_command(self._graph, title))

    def _push_add_panel(self) -> None:
        self._bus.push(add_panel_command(self._graph))
        self._panel_index = len(self.spec().panels) - 1
        self.reload()
