"""Frontière d'architecture : `plotyx.core` n'importe jamais Qt.

Le README en fait un principe non négociable. La roadmap parle de « signaux
Qt émis par le modèle » : la contradiction est levée par un mécanisme
d'observateur en Python pur (`plotyx.core.signals`), la couche `ui` faisant
l'adaptation. Ce test est le garde-fou automatique de cette décision.

Deux vérifications complémentaires :

* **statique** : aucune mention de PySide6/PyQt/qtpy dans les sources ;
* **dynamique** : `plotyx.core` et tous ses sous-modules s'importent dans un
  interpréteur où l'import de Qt est rendu impossible.
"""

from __future__ import annotations

import pkgutil
import re
import subprocess
import sys
from pathlib import Path

import plotyx.core

CORE_DIR = Path(plotyx.core.__file__).parent
QT_PATTERN = re.compile(r"^\s*(?:from|import)\s+(PySide\d|PyQt\d|qtpy|shiboken\d)", re.MULTILINE)


def _core_sources() -> list[Path]:
    return sorted(CORE_DIR.rglob("*.py"))


def test_core_has_sources_to_check():
    # Garde-fou du garde-fou : un paquet vide ferait passer le test pour rien.
    assert len(_core_sources()) >= 8


def test_no_qt_import_statement_in_core_sources():
    offenders = [path for path in _core_sources() if QT_PATTERN.search(path.read_text())]
    assert offenders == [], f"import Qt interdit dans core/ : {offenders}"


def test_core_imports_with_qt_unavailable():
    """Import de tout `plotyx.core` avec Qt rendu introuvable."""
    modules = ["plotyx.core"] + [
        f"plotyx.core.{info.name}" for info in pkgutil.iter_modules([str(CORE_DIR)])
    ]
    script = f"""
import importlib, sys

class QtBlocker:
    def find_module(self, name, path=None):
        return self.find_spec(name, path)

    def find_spec(self, name, path=None, target=None):
        if name.split('.')[0] in {{'PySide6', 'PySide2', 'PyQt5', 'PyQt6', 'qtpy'}}:
            raise ImportError('Qt interdit dans plotyx.core')
        return None

sys.meta_path.insert(0, QtBlocker())
for name in {modules!r}:
    importlib.import_module(name)
loaded = [n for n in sys.modules if n.split('.')[0] in
          ('PySide6', 'PySide2', 'PyQt5', 'PyQt6', 'qtpy')]
assert not loaded, loaded
print('OK')
"""
    result = subprocess.run(
        [sys.executable, "-c", script], capture_output=True, text=True, check=False
    )
    assert result.returncode == 0, result.stderr
    assert "OK" in result.stdout
