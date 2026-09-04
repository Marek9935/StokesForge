"""Point d'entrée applicatif : construit le `QApplication` et affiche
la fenêtre principale.
"""

from __future__ import annotations

import sys

from PySide6.QtWidgets import QApplication
from qt_material import apply_stylesheet

from plotyx.ui.main_window import MainWindow


def create_application() -> QApplication:
    """Crée (ou récupère) le `QApplication` et lui applique le thème."""
    app = QApplication.instance() or QApplication(sys.argv)
    apply_stylesheet(app, theme="dark_teal.xml")
    return app


def main() -> int:
    app = create_application()
    window = MainWindow()
    window.show()
    return app.exec()


if __name__ == "__main__":
    sys.exit(main())
