"""Fenêtre principale de PlotYx.

Phase 0 : coquille vide — `QMainWindow` + `QMdiArea` central + trois
docks stubs (Project Explorer, Properties, Script Console), sans
aucune logique métier. Le contenu réel de ces docks arrive dans les
phases suivantes (3, 8, 9).
"""

from __future__ import annotations

from PySide6.QtCore import Qt
from PySide6.QtWidgets import (
    QDockWidget,
    QLabel,
    QMainWindow,
    QMdiArea,
)


def _make_stub_dock(title: str, object_name: str, placeholder_text: str) -> QDockWidget:
    """Crée un dock stub : juste un titre et un texte de substitution.

    Aucun contenu fonctionnel n'est attendu ici en phase 0.
    """
    dock = QDockWidget(title)
    dock.setObjectName(object_name)
    label = QLabel(placeholder_text)
    label.setAlignment(Qt.AlignmentFlag.AlignCenter)
    label.setEnabled(False)
    dock.setWidget(label)
    return dock


class MainWindow(QMainWindow):
    """Fenêtre principale : zone MDI centrale + docks ancrables."""

    def __init__(self) -> None:
        super().__init__()
        self.setObjectName("PlotYxMainWindow")
        self.setWindowTitle("PlotYx")
        self.resize(1200, 800)

        self._mdi_area = QMdiArea(self)
        self.setCentralWidget(self._mdi_area)

        self._project_explorer_dock = _make_stub_dock(
            "Project Explorer", "projectExplorerDock", "(à venir — phase 3)"
        )
        self._properties_dock = _make_stub_dock(
            "Properties", "propertiesDock", "(à venir — phase 3/6)"
        )
        self._script_console_dock = _make_stub_dock(
            "Script Console", "scriptConsoleDock", "(à venir — phase 9)"
        )

        self.addDockWidget(Qt.DockWidgetArea.LeftDockWidgetArea, self._project_explorer_dock)
        self.addDockWidget(Qt.DockWidgetArea.RightDockWidgetArea, self._properties_dock)
        self.addDockWidget(Qt.DockWidgetArea.BottomDockWidgetArea, self._script_console_dock)

    @property
    def mdi_area(self) -> QMdiArea:
        return self._mdi_area
