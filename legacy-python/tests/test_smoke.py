"""Test smoke phase 0 : la fenêtre principale s'ouvre et se ferme.

Ce test ne vérifie aucune logique métier (il n'y en a pas encore) :
seulement que l'application se construit et s'affiche sans lever
d'exception, en environnement headless (`QT_QPA_PLATFORM=offscreen`).
"""

from __future__ import annotations

from PySide6.QtWidgets import QDockWidget

from plotyx.ui.main_window import MainWindow


def test_main_window_opens_and_closes(qtbot):
    window = MainWindow()
    qtbot.addWidget(window)

    window.show()
    qtbot.waitExposed(window)

    assert window.isVisible()
    assert window.windowTitle() == "PlotYx"
    assert window.mdi_area is window.centralWidget()

    window.close()
    assert not window.isVisible()


def test_main_window_has_three_stub_docks(qtbot):
    window = MainWindow()
    qtbot.addWidget(window)

    dock_names = {dock.objectName() for dock in window.findChildren(QDockWidget)}
    assert "projectExplorerDock" in dock_names
    assert "propertiesDock" in dock_names
    assert "scriptConsoleDock" in dock_names
