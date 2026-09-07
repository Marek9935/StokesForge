"""Save / Save As / Open câblés dans l'UI, prompt de fermeture, autosave, recovery.

Les dialogues Qt modaux (`QFileDialog`, `QMessageBox`) sont remplacés par
des méthodes statiques monkeypatchées, à l'identique du procédé déjà en
place dans `test_main_window_ui.py` pour `QInputDialog.getText`.
"""

from __future__ import annotations

import uuid

import numpy as np
import pytest
from PySide6.QtGui import QKeySequence
from PySide6.QtWidgets import QFileDialog, QMessageBox

from plotyx.commands import SetCellsCommand, mutation_guard
from plotyx.core.project import Project
from plotyx.persistence import autosave_path, list_recoverable, load_project, save_project
from plotyx.ui.main_window import MainWindow


@pytest.fixture(autouse=True)
def isolated_autosave_dir(tmp_path, monkeypatch):
    monkeypatch.setenv("PLOTYX_AUTOSAVE_DIR", str(tmp_path / "autosave"))
    return tmp_path


@pytest.fixture
def window(qtbot, monkeypatch):
    # `qtbot.addWidget` programme sa propre fermeture automatique dans le
    # hook `pytest_runtest_teardown` de pytest-qt, qui s'exécute avant le
    # code placé après le `yield` ci-dessous : le filet doit donc être posé
    # en amont pour rester actif quel que soit qui appelle `close()` en
    # premier — voir le commentaire équivalent dans `test_main_window_ui.py`.
    # Chaque test qui modifie le document le nettoie explicitement (undo ou
    # sauvegarde) avant sa fin ; ce filet ne couvre que l'imprévu.
    monkeypatch.setattr(
        QMessageBox,
        "warning",
        staticmethod(lambda *a, **k: QMessageBox.StandardButton.Discard),
    )
    win = MainWindow(autosave_interval_ms=0)  # minuteur désarmé : appelé à la main
    qtbot.addWidget(win)
    win.show()
    qtbot.waitExposed(win)
    yield win
    win.close()
    qtbot.wait(0)


def test_save_and_save_as_shortcuts_use_standard_key(window):
    assert window._save_action.shortcut() == QKeySequence(QKeySequence.StandardKey.Save)
    assert window._save_as_action.shortcut() == QKeySequence(QKeySequence.StandardKey.SaveAs)
    assert window._open_action.shortcut() == QKeySequence(QKeySequence.StandardKey.Open)


def test_save_as_writes_a_file_and_clears_modified_state(window, tmp_path, monkeypatch):
    path = tmp_path / "saved.plx"
    monkeypatch.setattr(
        QFileDialog, "getSaveFileName", staticmethod(lambda *a, **k: (str(path), ""))
    )
    sheet = window._first_worksheet()
    with mutation_guard():
        window.bus.push(SetCellsCommand(sheet.column("A"), 0, [5.0]))
    assert window.bus.is_modified

    ok = window._on_save_as()

    assert ok is True
    assert path.exists()
    assert window.current_path == path
    assert not window.bus.is_modified
    assert window.windowTitle() == "PlotYx"


def test_save_appends_extension_when_missing(window, tmp_path, monkeypatch):
    path = tmp_path / "no_extension"
    monkeypatch.setattr(
        QFileDialog, "getSaveFileName", staticmethod(lambda *a, **k: (str(path), ""))
    )
    window._on_save_as()
    assert window.current_path == path.with_name("no_extension.plx")
    assert window.current_path.exists()


def test_save_without_a_path_delegates_to_save_as(window, tmp_path, monkeypatch):
    path = tmp_path / "delegated.plx"
    calls = []
    monkeypatch.setattr(
        QFileDialog,
        "getSaveFileName",
        staticmethod(lambda *a, **k: (calls.append(1) or str(path), "")),
    )
    assert window.current_path is None
    ok = window._on_save()
    assert ok is True
    assert calls  # save_as a bien été sollicité
    assert window.current_path == path


def test_save_as_cancelled_dialog_does_nothing(window, monkeypatch):
    monkeypatch.setattr(QFileDialog, "getSaveFileName", staticmethod(lambda *a, **k: ("", "")))
    assert window._on_save_as() is False
    assert window.current_path is None


def test_open_replaces_the_live_project_and_resets_undo(window, tmp_path):
    other = Project("Reopened")
    book = other.add_workbook("BookX")
    book.sheet("Sheet1").add_column("Z", [1.0, 2.0, 3.0])
    path = tmp_path / "other.plx"
    save_project(other, path)

    sheet = window._first_worksheet()
    with mutation_guard():
        window.bus.push(SetCellsCommand(sheet.column("A"), 0, [9.0]))
    old_bus = window.bus
    assert old_bus.can_undo

    ok = window.open_path(path)

    assert ok is True
    assert window.project.name == "Reopened"
    assert window.project.find_window("BookX") is not None
    assert window.bus is not old_bus
    assert not window.bus.can_undo  # pile neuve : rien à annuler vers l'ancien état
    assert not window.bus.is_modified
    assert window.current_path == path


def test_open_reopens_a_visible_worksheet(window, tmp_path):
    other = Project("Reopened")
    book = other.add_workbook("BookX")
    book.sheet("Sheet1").add_column("Z", [1.0])
    path = tmp_path / "other.plx"
    save_project(other, path)

    window.open_path(path)

    assert len(window.mdi_area.subWindowList()) == 1


def test_open_rejects_a_future_schema_version_without_crashing(window, tmp_path, monkeypatch):
    import h5py

    from plotyx.persistence.schema import (
        ATTR_FORMAT,
        ATTR_SCHEMA_VERSION,
        CURRENT_SCHEMA_VERSION,
        FORMAT_MARKER,
        GROUP_PROJECT,
    )

    path = tmp_path / "future.plx"
    with h5py.File(path, "w") as file:
        file.attrs[ATTR_FORMAT] = FORMAT_MARKER
        file.attrs[ATTR_SCHEMA_VERSION] = CURRENT_SCHEMA_VERSION + 1
        group = file.create_group(GROUP_PROJECT)
        group.attrs["key"] = "k"
        group.attrs["name"] = "n"
        group.attrs["type"] = "folder"
        group.attrs.create("children_order", data=[], dtype=h5py.string_dtype())

    seen = []
    monkeypatch.setattr(
        QMessageBox, "critical", staticmethod(lambda *a, **k: seen.append(a) or None)
    )
    project_before = window.project

    ok = window.open_path(path)

    assert ok is False
    assert seen  # un message d'erreur a bien été affiché
    assert window.project is project_before  # le projet en cours n'a pas bougé


def test_close_prompts_to_save_when_modified(qtbot, tmp_path, monkeypatch):
    win = MainWindow(autosave_interval_ms=0)
    qtbot.addWidget(win)
    win.show()
    qtbot.waitExposed(win)
    sheet = win._first_worksheet()
    with mutation_guard():
        win.bus.push(SetCellsCommand(sheet.column("A"), 0, [1.0]))

    path = tmp_path / "closed.plx"
    monkeypatch.setattr(
        QFileDialog, "getSaveFileName", staticmethod(lambda *a, **k: (str(path), ""))
    )
    monkeypatch.setattr(
        QMessageBox,
        "warning",
        staticmethod(lambda *a, **k: QMessageBox.StandardButton.Save),
    )
    win.close()
    qtbot.wait(0)
    assert path.exists()


def test_close_cancel_keeps_the_window_open(qtbot, monkeypatch):
    win = MainWindow(autosave_interval_ms=0)
    qtbot.addWidget(win)
    win.show()
    qtbot.waitExposed(win)
    sheet = win._first_worksheet()
    with mutation_guard():
        win.bus.push(SetCellsCommand(sheet.column("A"), 0, [1.0]))

    monkeypatch.setattr(
        QMessageBox,
        "warning",
        staticmethod(lambda *a, **k: QMessageBox.StandardButton.Cancel),
    )
    accepted = win.close()
    assert accepted is False
    win_bus_modified = win.bus.is_modified
    qtbot.wait(0)
    assert win_bus_modified
    # nettoyage manuel : on force la fermeture pour ne pas fuiter le widget
    monkeypatch.setattr(
        QMessageBox,
        "warning",
        staticmethod(lambda *a, **k: QMessageBox.StandardButton.Discard),
    )
    win.close()
    qtbot.wait(0)


def test_close_discards_the_sessions_autosave_file(qtbot, monkeypatch):
    win = MainWindow(autosave_interval_ms=0)
    qtbot.addWidget(win)
    win.show()
    qtbot.waitExposed(win)
    session_id = win._session_id
    win._on_autosave_tick()  # rien à sauver : pas encore modifié
    sheet = win._first_worksheet()
    with mutation_guard():
        win.bus.push(SetCellsCommand(sheet.column("A"), 0, [1.0]))
    win._on_autosave_tick()
    assert autosave_path(session_id).exists()
    monkeypatch.setattr(
        QMessageBox,
        "warning",
        staticmethod(lambda *a, **k: QMessageBox.StandardButton.Discard),
    )
    win.close()
    qtbot.wait(0)
    assert not autosave_path(session_id).exists()


def test_autosave_tick_is_a_no_op_when_document_is_clean(window):
    window._on_autosave_tick()
    assert not autosave_path(window._session_id).exists()


def test_autosave_tick_never_raises_on_write_failure(window, monkeypatch):
    sheet = window._first_worksheet()
    with mutation_guard():
        window.bus.push(SetCellsCommand(sheet.column("A"), 0, [1.0]))

    def _boom(*a, **k):
        raise OSError("disque plein")

    monkeypatch.setattr("plotyx.ui.main_window.write_autosave", _boom)
    window._on_autosave_tick()  # ne doit pas lever

    # Repropre pour la fermeture de la fixture : pas de dialogue modal bloquant.
    window.bus.undo()


def test_recovery_offers_a_crashed_sessions_autosave(qtbot, tmp_path, monkeypatch):
    crashed_project = Project("Crashed")
    crashed_project.add_workbook("Recovered")
    crashed_session = uuid.uuid4().hex
    from plotyx.persistence import write_autosave

    write_autosave(crashed_project, crashed_session, None)
    assert list_recoverable()

    monkeypatch.setattr(
        QMessageBox, "question", staticmethod(lambda *a, **k: QMessageBox.StandardButton.Yes)
    )
    win = MainWindow(check_recovery=True, autosave_interval_ms=0)
    qtbot.addWidget(win)
    try:
        assert win.project.find_window("Recovered") is not None
        # la sauvegarde récupérée continue de protéger la session courante
        assert autosave_path(win._session_id).exists()
        assert not autosave_path(crashed_session).exists()
    finally:
        win.close()
        qtbot.wait(0)


def test_recovery_declined_removes_the_stale_autosave(qtbot, tmp_path, monkeypatch):
    crashed_session = uuid.uuid4().hex
    from plotyx.persistence import write_autosave

    write_autosave(Project("Crashed"), crashed_session, None)

    monkeypatch.setattr(
        QMessageBox, "question", staticmethod(lambda *a, **k: QMessageBox.StandardButton.No)
    )
    win = MainWindow(check_recovery=True, autosave_interval_ms=0)
    qtbot.addWidget(win)
    try:
        assert win.project.name != "Crashed"
        assert not autosave_path(crashed_session).exists()
    finally:
        win.close()
        qtbot.wait(0)


def test_no_recovery_prompt_when_nothing_to_recover(qtbot, monkeypatch):
    calls = []
    monkeypatch.setattr(
        QMessageBox, "question", staticmethod(lambda *a, **k: calls.append(1) or None)
    )
    win = MainWindow(check_recovery=True, autosave_interval_ms=0)
    qtbot.addWidget(win)
    try:
        assert not calls
    finally:
        win.close()
        qtbot.wait(0)


def test_check_recovery_defaults_to_false(qtbot, monkeypatch):
    """Défaut sûr pour la suite de tests : pas de dialogue modal surprise."""
    calls = []
    monkeypatch.setattr(
        QMessageBox, "question", staticmethod(lambda *a, **k: calls.append(1) or None)
    )
    crashed_session = uuid.uuid4().hex
    from plotyx.persistence import write_autosave

    write_autosave(Project("Crashed"), crashed_session, None)
    win = MainWindow(autosave_interval_ms=0)
    qtbot.addWidget(win)
    try:
        assert not calls
    finally:
        win.close()
        qtbot.wait(0)
        autosave_path(crashed_session).unlink(missing_ok=True)


def test_new_project_action_prompts_and_resets(window, tmp_path, monkeypatch):
    sheet = window._first_worksheet()
    with mutation_guard():
        window.bus.push(SetCellsCommand(sheet.column("A"), 0, [1.0]))
    monkeypatch.setattr(
        QMessageBox,
        "warning",
        staticmethod(lambda *a, **k: QMessageBox.StandardButton.Discard),
    )
    old_project = window.project
    window._on_new()
    assert window.project is not old_project
    assert not window.bus.is_modified


def test_full_ui_round_trip_save_close_reopen(qtbot, tmp_path):
    """Bout en bout : éditer, sauver via l'action Ctrl+S, fermer, rouvrir."""
    win = MainWindow(autosave_interval_ms=0)
    qtbot.addWidget(win)
    win.show()
    qtbot.waitExposed(win)

    sheet = win._first_worksheet()
    with mutation_guard():
        win.bus.push(SetCellsCommand(sheet.column("A"), 0, [1.0, 2.0, np.nan]))

    path = tmp_path / "roundtrip.plx"
    win._current_path = path
    ok = win._on_save()
    assert ok

    win.close()
    qtbot.wait(0)

    reloaded = load_project(path)
    reloaded_sheet = reloaded.resolve_path("[Book1]Sheet1")
    np.testing.assert_array_equal(
        reloaded_sheet.column("A").values, np.array([1.0, 2.0, np.nan])
    )
