"""Frontière d'architecture : le `PlotSpec` ne connaît **aucun** moteur de rendu.

C'est le test de vérification exigé par la phase 6 pour la décision la plus
coûteuse à revenir dessus du projet : « un `PlotSpec` doit être
constructible et inspectable sans importer PyQtGraph ni Qt ». Sans ce
garde-fou, un `import pyqtgraph` glissé dans `plotyx/plot/` pour une
conversion de couleur suffirait à faire du second renderer une réécriture
plutôt qu'un ajout.

**Ce n'est plus une hypothèse.** Le 2026-09-07, Matplotlib a remplacé
PyQtGraph comme rendu par défaut ; l'opération a coûté un module
(`plotyx/ui/mpl_renderer.py`) et **zéro ligne** de `plotyx/plot/`. Ce
fichier est ce qui a rendu ça possible — et le reste : le second moteur
existe désormais pour de bon, donc la moindre fuite de vocabulaire vers la
couche neutre se paierait tout de suite, dans les deux renderers.

Même dispositif que `tests/test_core_no_qt.py`, étendu à trois familles de
paquets :

* **statique** : aucune mention de PySide6/PyQt/qtpy/pyqtgraph/matplotlib
  dans les sources de `plotyx.plot` ;
* **dynamique** : un `PlotSpec` complet est construit, sérialisé et relu
  dans un interpréteur où l'import de ces paquets lève.

`plotyx.plot.resolve` a le droit d'importer `plotyx.core` et numpy — mais
pas Qt : c'est lui qui traduit les clés de registre en colonnes, un service
dont les deux renderers ont également besoin.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

import plotyx.plot

PLOT_DIR = Path(plotyx.plot.__file__).parent
ENGINE_PATTERN = re.compile(
    r"^\s*(?:from|import)\s+(PySide\d|PyQt\d|qtpy|shiboken\d|pyqtgraph|matplotlib)",
    re.MULTILINE,
)

#: Les paquets que `plotyx.plot` n'a pas le droit de toucher.
FORBIDDEN = ("PySide6", "PySide2", "PyQt5", "PyQt6", "qtpy", "pyqtgraph", "matplotlib")


def _plot_sources() -> list[Path]:
    return sorted(PLOT_DIR.rglob("*.py"))


def test_plot_package_has_sources_to_check():
    # Garde-fou du garde-fou : un paquet vide ferait passer le test pour rien.
    assert len(_plot_sources()) >= 4


def test_no_render_engine_import_in_plot_sources():
    offenders = [path for path in _plot_sources() if ENGINE_PATTERN.search(path.read_text())]
    assert offenders == [], f"import de moteur de rendu interdit dans plot/ : {offenders}"


def test_a_plot_spec_is_built_and_inspected_without_qt_or_pyqtgraph():
    """Le test exigé par la phase 6, dans un interpréteur sans moteur de rendu."""
    script = f"""
import sys

class EngineBlocker:
    def find_spec(self, name, path=None, target=None):
        if name.split('.')[0] in {set(FORBIDDEN)!r}:
            raise ImportError('moteur de rendu interdit dans plotyx.plot')
        return None

sys.meta_path.insert(0, EngineBlocker())

from plotyx.plot import (
    AxisScale, AxisSide, LineStyle, PanelSpec, PlotSpec, PlotType,
    SeriesSpec, SymbolShape, add_panel, add_series, update_axis, update_series,
)

# Construction
spec = PlotSpec.empty("Mesures")
spec = add_series(spec, SeriesSpec(y_key="k-y", x_key="k-x", plot_type=PlotType.LINE_SCATTER))
spec = add_panel(spec)
spec = add_series(spec, SeriesSpec(y_key="k-z", plot_type=PlotType.HISTOGRAM), 1)
spec = update_axis(spec, 0, AxisSide.LEFT, scale=AxisScale.LOG, label="Tension", unit="V")

# Inspection
assert spec.series_count() == 2
assert spec.rows == 2 and spec.columns == 1
assert spec.panels[0].left.display_label == "Tension (V)"
assert set(spec.keys()) == {{"k-x", "k-y", "k-z"}}
first = spec.panels[0].series[0]
assert first.plot_type.draws_line and first.plot_type.draws_symbols
assert spec.find_series(first.id)[0] == 0

# Modification + sérialisation
spec = update_series(spec, first.id, line={{"style": LineStyle.DASH}}, visible=False)
restored = PlotSpec.from_dict(spec.to_dict())
assert restored == spec, "aller-retour to_dict/from_dict non fidèle"
assert restored.panels[0].series[0].symbol.shape is SymbolShape.CIRCLE

loaded = [n for n in sys.modules if n.split('.')[0] in {set(FORBIDDEN)!r}]
assert not loaded, loaded
print('OK')
"""
    result = subprocess.run(
        [sys.executable, "-c", script], capture_output=True, text=True, check=False
    )
    assert result.returncode == 0, result.stderr
    assert "OK" in result.stdout


def test_resolve_layer_stays_free_of_qt_even_though_it_knows_core():
    """`plotyx.plot.resolve` connaît `core` et numpy — jamais Qt.

    C'est la frontière exacte : la résolution des clés de registre est
    partagée par les deux renderers, elle ne peut donc appartenir à aucun
    des deux.
    """
    script = f"""
import sys

class EngineBlocker:
    def find_spec(self, name, path=None, target=None):
        if name.split('.')[0] in {set(FORBIDDEN)!r}:
            raise ImportError('moteur de rendu interdit')
        return None

sys.meta_path.insert(0, EngineBlocker())

import numpy as np
from plotyx.core import Project
from plotyx.plot import PlotSpec, SeriesSpec, add_series
from plotyx.plot.resolve import resolve_spec

project = Project("P")
book = project.add_workbook("Book1")
sheet = book.sheet("Sheet1")
column = sheet.add_column("A", [1.0, 2.0, 3.0])
spec = add_series(PlotSpec.empty(), SeriesSpec(y_key=column.key))
resolved = resolve_spec(project, spec)
assert resolved.panels[0].series[0].ok
assert list(resolved.panels[0].series[0].y) == [1.0, 2.0, 3.0]
print('OK')
"""
    result = subprocess.run(
        [sys.executable, "-c", script], capture_output=True, text=True, check=False
    )
    assert result.returncode == 0, result.stderr
    assert "OK" in result.stdout
