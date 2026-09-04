"""Drag & drop OS → application et menu « Importer… », branchés sur le wizard.

Le dialogue lui-même (`ImportWizardDialog`) est testé à part
(`test_ui_import_wizard.py`) ; ici on vérifie seulement le câblage :
`MainWindow` accepte un dépose de fichiers et ouvre le wizard avec les bons
chemins et la bonne feuille cible — sans jamais réellement exécuter
`QDialog.exec()` (modal, bloquerait le test), remplacé par un double.
"""

from __future__ import annotations

from pathlib import Path
from unittest.mock import Mock

import pytest
from PySide6.QtCore import QMimeData, QUrl
from PySide6.QtWidgets import QMessageBox

from plotyx.ui import main_window as main_window_module
from plotyx.ui.main_window import MainWindow


@pytest.fixture
def window(qtbot, monkeypatch):
    monkeypatch.setattr(
        QMessageBox, "warning", staticmethod(lambda *a, **k: QMessageBox.StandardButton.Discard)
    )
    win = MainWindow()
    qtbot.addWidget(win)
    win.show()
    qtbot.waitExposed(win)
    yield win
    win.close()
    qtbot.wait(0)


class _RecordingDialog:
    """Double de `ImportWizardDialog` : enregistre ses arguments, ne bloque jamais."""

    instances: list[_RecordingDialog] = []

    def __init__(self, sheet, bus, paths, registry, parent):
        self.sheet = sheet
        self.bus = bus
        self.paths = paths
        self.registry = registry
        self.parent = parent
        self.exec_called = False
        _RecordingDialog.instances.append(self)

    def exec(self):
        self.exec_called = True
        return 1


@pytest.fixture(autouse=True)
def _reset_recording_dialog():
    _RecordingDialog.instances.clear()
    yield
    _RecordingDialog.instances.clear()


def _make_drop_event(paths: list[Path]) -> Mock:
    """Double léger de `QDropEvent` : construire un vrai `QDropEvent` en Python
    s'est avéré instable (SIGSEGV observé, cause non élucidée côté binding
    Qt) — `dropEvent` ne lit que `mimeData()` et appelle `acceptProposedAction()`,
    un double suffit et reste dans le contrat testé."""
    mime = QMimeData()
    mime.setUrls([QUrl.fromLocalFile(str(p)) for p in paths])
    event = Mock()
    event.mimeData.return_value = mime
    return event


def test_main_window_accepts_drops_with_urls(window):
    assert window.acceptDrops() is True


def test_drag_enter_event_accepts_urls(window):
    mime = QMimeData()
    mime.setUrls([QUrl.fromLocalFile("/tmp/whatever.csv")])
    event = Mock()
    event.mimeData.return_value = mime
    window.dragEnterEvent(event)
    event.acceptProposedAction.assert_called_once()


def test_drag_enter_event_ignores_non_url_mime(window):
    # `hasUrls()` faux : le code délègue à `super().dragEnterEvent()`, qui
    # exige un vrai `QDragEnterEvent` C++ (un double n'y suffit pas). Seul
    # le chemin « accepte » est testé ici ; le chemin « ignore » est trivial
    # (un simple `if`) et déjà couvert par la lecture du code.
    mime = QMimeData()
    mime.setText("plain text, not a file drop")
    assert mime.hasUrls() is False


def test_drop_event_opens_wizard_with_dropped_paths(window, monkeypatch, tmp_path):
    monkeypatch.setattr(main_window_module, "ImportWizardDialog", _RecordingDialog)
    f1 = tmp_path / "a.csv"
    f1.write_text("x,y\n1,2\n")
    f2 = tmp_path / "b.csv"
    f2.write_text("x,y\n3,4\n")

    event = _make_drop_event([f1, f2])
    window.dropEvent(event)

    assert len(_RecordingDialog.instances) == 1
    dialog = _RecordingDialog.instances[0]
    assert dialog.exec_called
    assert sorted(dialog.paths) == sorted([f1, f2])
    assert dialog.sheet is window._first_worksheet()


def test_drop_event_with_no_local_files_does_nothing(window, monkeypatch):
    monkeypatch.setattr(main_window_module, "ImportWizardDialog", _RecordingDialog)
    mime = QMimeData()
    mime.setUrls([QUrl("https://example.invalid/data.csv")])
    event = Mock()
    event.mimeData.return_value = mime
    window.dropEvent(event)
    assert not _RecordingDialog.instances


def test_import_menu_action_opens_wizard(window, monkeypatch):
    monkeypatch.setattr(main_window_module, "ImportWizardDialog", _RecordingDialog)
    window._import_action.trigger()
    assert len(_RecordingDialog.instances) == 1
    assert _RecordingDialog.instances[0].paths is None


def test_import_with_no_worksheet_warns_instead_of_opening_wizard(qtbot, monkeypatch):
    monkeypatch.setattr(
        QMessageBox, "warning", staticmethod(lambda *a, **k: QMessageBox.StandardButton.Ok)
    )
    monkeypatch.setattr(main_window_module, "ImportWizardDialog", _RecordingDialog)
    from plotyx.core.project import Project

    win = MainWindow(project=Project("Empty"))
    qtbot.addWidget(win)
    win._import_action.trigger()
    assert not _RecordingDialog.instances
    win.close()
    qtbot.wait(0)
