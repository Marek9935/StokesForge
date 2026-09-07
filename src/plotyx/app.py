"""Point d'entrée applicatif : construit le `QApplication` et affiche
la fenêtre principale.
"""

from __future__ import annotations

import os
import sys

import pyqtgraph as pg
from PySide6.QtWidgets import QApplication
from qt_material import apply_stylesheet

from plotyx.ui.main_window import MainWindow

#: Thème qt-material. Clair par défaut — axe de développement principal
#: décidé le 2026-09-07 ; « dark_teal » reste utilisable en changeant cette
#: seule constante si un thème sombre redevient nécessaire.
DEFAULT_THEME = "light_teal.xml"


def select_linux_platform(env: dict[str, str] | None = None) -> str | None:
    """Force `xcb` sous Wayland. Retourne la plateforme imposée, ou None.

    ## Pourquoi

    Wayland interdit par conception à une application de connaître ou de
    fixer la position de ses propres fenêtres de premier niveau. Or Qt
    déplace un `QDockWidget` détaché en repositionnant sa fenêtre : une
    fois flottant, le dock devient immobile et seul le bouton d'ancrage
    le ramène. La même impasse touchera le détachement d'un graphe sur un
    second écran (phase 6) et la restauration de la disposition des
    fenêtres (phase 11).

    ## Pourquoi ce n'est pas réparable côté application

    Le protocole `xdg_toplevel_drag_v1` existe précisément pour ça — c'est
    lui qui permet d'arracher un onglet de navigateur sous Wayland. Relevé
    le 2026-09-04 sur Fedora / GNOME Shell 50 / Qt 6.11.2 : le compositeur
    l'implémente (`libmutter-18`) **et** Qt embarque les bindings client
    (`libQt6WaylandClient`). Ce qui manque est le câblage interne de
    `QDockWidget`, et ces classes Qt sont privées — inatteignables depuis
    PySide6. Le correctif ne peut venir que de Qt.

    ## Quand supprimer ce contournement

    Lancer l'application avec `QT_QPA_PLATFORM=wayland`, détacher un dock
    et le déplacer. Si ça fonctionne, Qt a câblé le protocole : inverser
    le défaut ci-dessous, et garder xcb comme repli.

    ## Règles

    Trois cas où l'on ne force rien : un `QT_QPA_PLATFORM` explicite
    (le choix de l'utilisateur prime toujours), une session qui n'est pas
    Wayland, et l'absence de `DISPLAY` — sans XWayland, imposer `xcb`
    empêcherait l'application de démarrer.
    """
    environ = os.environ if env is None else env
    if sys.platform != "linux":
        return None
    if environ.get("QT_QPA_PLATFORM"):
        return None
    if environ.get("XDG_SESSION_TYPE") != "wayland":
        return None
    if not environ.get("DISPLAY"):
        return None
    environ["QT_QPA_PLATFORM"] = "xcb"
    return "xcb"


def create_application() -> QApplication:
    """Crée (ou récupère) le `QApplication` et lui applique le thème.

    PyQtGraph a son propre système de couleurs, indépendant de la feuille
    de style Qt : sans configuration explicite, un `GraphicsLayoutWidget`
    garde son fond noir par défaut même sous un thème d'application clair.
    On aligne les deux ici, au même point d'entrée, plutôt que dans
    `plot_renderer.py` — `PlotSpec.background` reste prioritaire quand un
    graphe fixe sa propre couleur (voir `PyQtGraphRenderer`).
    """
    app = QApplication.instance() or QApplication(sys.argv)
    apply_stylesheet(app, theme=DEFAULT_THEME)
    pg.setConfigOptions(background="w", foreground="k")
    return app


def main() -> int:
    select_linux_platform()  # avant toute création de QApplication
    app = create_application()
    window = MainWindow(check_recovery=True)
    window.show()
    return app.exec()


if __name__ == "__main__":
    sys.exit(main())
