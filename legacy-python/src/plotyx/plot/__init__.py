"""`plotyx.plot` — description de graphe **neutre vis-à-vis du moteur de rendu**.

La roadmap identifie « deux moteurs de rendu (PyQtGraph pour l'écran,
Matplotlib pour l'export publication) » comme un risque de divergence, et
demande de le mitiger par une couche `PlotSpec` neutre **conçue en phase 6,
pas en phase 10**. C'est ce paquet.

```
plotyx.plot.spec     PlotSpec / PanelSpec / SeriesSpec / AxisSpec — stdlib seule
plotyx.plot.edit     transformations pures spec → spec (base de l'undo)
plotyx.plot.resolve  clés de registre → colonnes du projet (+ plotyx.core, numpy)
```

Aucun de ces modules n'importe Qt, PyQtGraph ou Matplotlib.
`tests/test_plot_spec_is_engine_neutral.py` le vérifie dans un interpréteur
où ces trois paquets sont rendus introuvables — le même dispositif que
`tests/test_core_no_qt.py` pour la frontière `core`/Qt.

Le renderer PyQtGraph vit dans `plotyx.ui.plot_renderer` ; celui de
Matplotlib s'ajoutera à côté en phase 10, sans que ce paquet bouge.
"""

from plotyx.plot.edit import (
    add_panel,
    add_series,
    move_series,
    next_series_color,
    remove_panel,
    remove_series,
    series_for_column,
    update_axis,
    update_legend,
    update_panel,
    update_series,
    update_title,
)
from plotyx.plot.spec import (
    DEFAULT_PALETTE,
    AxisScale,
    AxisSide,
    AxisSpec,
    LegendPosition,
    LegendSpec,
    LineSpec,
    LineStyle,
    PanelSpec,
    PlotSpec,
    PlotType,
    SeriesSpec,
    SymbolShape,
    SymbolSpec,
    new_id,
    palette_color,
)

__all__ = [
    "DEFAULT_PALETTE",
    "AxisScale",
    "AxisSide",
    "AxisSpec",
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
    "add_panel",
    "add_series",
    "move_series",
    "new_id",
    "next_series_color",
    "palette_color",
    "remove_panel",
    "remove_series",
    "series_for_column",
    "update_axis",
    "update_legend",
    "update_panel",
    "update_series",
    "update_title",
]
